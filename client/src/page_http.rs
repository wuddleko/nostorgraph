use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Instant;

use url::Url;

use super::transport::{
    arm_stream, connect_deadline, connect_tcp, resolve_host, socks5_connect, tls_connector,
    tls_handshake, wait_budget,
};
use super::RelaySocks;

const HEAD_MAX: usize = 8 * 1024;

pub enum PageError {
    Down,
    TooLarge,
}

pub fn fetch_http_page(
    page_url: &str,
    request: &str,
    deadline: Instant,
    max_body: usize,
    socks: RelaySocks,
) -> Result<String, PageError> {
    let url = Url::parse(page_url).map_err(|_| PageError::Down)?;
    let https = match url.scheme() {
        "https" => true,
        "http" => false,
        _ => return Err(PageError::Down),
    };
    let host = url.host_str().ok_or(PageError::Down)?.to_string();
    let port = url.port_or_known_default().ok_or(PageError::Down)?;
    let tcp = connect_page(&host, port, page_url, deadline, socks)?;
    if https {
        let connector = tls_connector().map_err(|_| PageError::Down)?;
        let mut tls =
            tls_handshake(connector, &host, tcp, deadline, None).map_err(|_| PageError::Down)?;
        write_tls(&mut tls, request.as_bytes(), deadline)?;
        pull(|buf| read_tls(&mut tls, buf, deadline), max_body)
    } else {
        let mut tcp = tcp;
        write_plain(&mut tcp, request.as_bytes(), deadline)?;
        pull(|buf| read_plain(&mut tcp, buf, deadline), max_body)
    }
}

fn connect_page(
    host: &str,
    port: u16,
    page_url: &str,
    deadline: Instant,
    socks: RelaySocks,
) -> Result<TcpStream, PageError> {
    let connect_by = connect_deadline(deadline);
    match socks {
        RelaySocks::Disabled => Err(PageError::Down),
        RelaySocks::Proxy(proxy) => {
            let tcp = connect_tcp(proxy, connect_by, None).map_err(|_| PageError::Down)?;
            let user = socks_user(page_url, host);
            socks5_connect(&tcp, host, port, &user, connect_by, None)
                .map_err(|_| PageError::Down)?;
            Ok(tcp)
        }
        RelaySocks::Direct => {
            if host.ends_with(".onion") {
                return Err(PageError::Down);
            }
            let addresses =
                resolve_host(host, port, connect_by, None).map_err(|_| PageError::Down)?;
            let mut last = PageError::Down;
            for address in addresses {
                match connect_tcp(address, connect_by, None) {
                    Ok(tcp) => return Ok(tcp),
                    Err(_) => last = PageError::Down,
                }
            }
            Err(last)
        }
    }
}

fn socks_user(page_url: &str, host: &str) -> String {
    if !page_url.is_empty() && page_url.len() <= 255 {
        page_url.to_string()
    } else if !host.is_empty() && host.len() <= 255 {
        host.to_string()
    } else {
        "ntgraph".to_string()
    }
}

fn write_plain(tcp: &mut TcpStream, bytes: &[u8], deadline: Instant) -> Result<(), PageError> {
    arm(tcp, deadline)?;
    tcp.write_all(bytes).map_err(|_| PageError::Down)?;
    arm(tcp, deadline)?;
    tcp.flush().map_err(|_| PageError::Down)
}

fn write_tls(
    tls: &mut native_tls::TlsStream<TcpStream>,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), PageError> {
    arm(tls.get_ref(), deadline)?;
    tls.write_all(bytes).map_err(|_| PageError::Down)?;
    arm(tls.get_ref(), deadline)?;
    tls.flush().map_err(|_| PageError::Down)
}

fn read_plain(tcp: &mut TcpStream, buf: &mut [u8], deadline: Instant) -> Result<usize, PageError> {
    loop {
        arm(tcp, deadline)?;
        match tcp.read(buf) {
            Ok(read) => return Ok(read),
            Err(error) if paused(&error) => {
                if Instant::now() >= deadline {
                    return Err(PageError::Down);
                }
            }
            Err(_) => return Err(PageError::Down),
        }
    }
}

fn read_tls(
    tls: &mut native_tls::TlsStream<TcpStream>,
    buf: &mut [u8],
    deadline: Instant,
) -> Result<usize, PageError> {
    loop {
        arm(tls.get_ref(), deadline)?;
        match tls.read(buf) {
            Ok(read) => return Ok(read),
            Err(error) if paused(&error) => {
                if Instant::now() >= deadline {
                    return Err(PageError::Down);
                }
            }
            Err(_) => return Err(PageError::Down),
        }
    }
}

fn arm(tcp: &TcpStream, deadline: Instant) -> Result<(), PageError> {
    arm_stream(
        tcp,
        wait_budget(deadline, None).map_err(|_| PageError::Down)?,
    )
    .map_err(|_| PageError::Down)
}

fn paused(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::Interrupted
    )
}

fn pull(
    mut read: impl FnMut(&mut [u8]) -> Result<usize, PageError>,
    max_body: usize,
) -> Result<String, PageError> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        if let Some(body) = complete(&buf, max_body, false)? {
            return Ok(body);
        }
        let read = read(&mut tmp)?;
        if read == 0 {
            return complete(&buf, max_body, true)?.ok_or(PageError::Down);
        }
        buf.extend_from_slice(&tmp[..read]);
        if buf.len() > HEAD_MAX + max_body {
            return Err(PageError::TooLarge);
        }
    }
}

fn complete(bytes: &[u8], max_body: usize, eof: bool) -> Result<Option<String>, PageError> {
    let Some(split) = header_end(bytes) else {
        if eof || bytes.len() > HEAD_MAX {
            return Err(PageError::Down);
        }
        return Ok(None);
    };
    if split > HEAD_MAX {
        return Err(PageError::Down);
    }
    let head = std::str::from_utf8(&bytes[..split]).map_err(|_| PageError::Down)?;
    let length = content_length(head)?;
    let body = &bytes[split..];
    if let Some(length) = length {
        if length > max_body {
            return Err(PageError::TooLarge);
        }
        if body.len() < length {
            if eof {
                return Err(PageError::Down);
            }
            return Ok(None);
        }
        return text(&body[..length]);
    }
    if body.len() > max_body {
        return Err(PageError::TooLarge);
    }
    if eof {
        text(body)
    } else {
        Ok(None)
    }
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|at| at + 4)
}

fn content_length(head: &str) -> Result<Option<usize>, PageError> {
    for line in head.split("\r\n").skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let length = value.trim().parse().map_err(|_| PageError::Down)?;
            return Ok(Some(length));
        }
    }
    Ok(None)
}

fn text(body: &[u8]) -> Result<Option<String>, PageError> {
    String::from_utf8(body.to_vec())
        .map(Some)
        .map_err(|_| PageError::Down)
}
