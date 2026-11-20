use serde_json::{Map, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Instant;

use super::transport::{
    arm_stream, relay_wait_is_over, wait_budget, with_relay_tls, RELAY_TIMEOUT,
};
use super::RelaySocks;

pub(super) const NIP11_BODY_MAX: usize = 64 * 1024;
const NIP11_HEAD_MAX: usize = 8 * 1024;

pub fn document_is_public(document: &Value) -> bool {
    let Some(limitation) = document.get("limitation").and_then(Value::as_object) else {
        return document.is_object();
    };
    if required(limitation, "auth_required")
        || required(limitation, "payment_required")
        || required(limitation, "restricted_writes")
    {
        return false;
    }
    accepts_note_kind(limitation)
}

fn required(limitation: &Map<String, Value>, name: &str) -> bool {
    limitation.get(name).and_then(Value::as_bool) == Some(true)
}

fn accepts_note_kind(limitation: &Map<String, Value>) -> bool {
    let Some(kinds) = limitation.get("kinds") else {
        return true;
    };
    kinds.as_array().is_some_and(|kinds| {
        kinds
            .iter()
            .any(|kind| kind.as_u64() == Some(u64::from(nostorgraph_nip44::KIND)))
    })
}

pub fn fetch_nip11(relay: &str, deadline: Instant, socks: RelaySocks) -> Option<String> {
    let request = nip11_request(relay).ok()?;
    with_relay_tls(relay, deadline, None, true, socks, |mut tls| {
        write_tls(&mut tls, request.as_bytes(), deadline)?;
        read_nip11_body(&mut tls, deadline)
    })
    .ok()
}

fn nip11_request(relay: &str) -> Result<String, String> {
    let url = url::Url::parse(relay).map_err(|error| error.to_string())?;
    if url.scheme() != "wss" {
        return Err("relay url has no host".to_string());
    }
    let host = url.host_str().ok_or("relay url has no host")?;
    let mut target = url.path().to_string();
    if target.is_empty() {
        target.push('/');
    }
    if let Some(query) = url.query() {
        target.push('?');
        target.push_str(query);
    }
    let mut authority = host.to_string();
    if let Some(port) = url.port() {
        if port != 443 {
            authority.push(':');
            authority.push_str(&port.to_string());
        }
    }
    let mut request = String::from("GET ");
    request.push_str(&target);
    request.push_str(" HTTP/1.1\r\nHost: ");
    request.push_str(&authority);
    request.push_str("\r\nAccept: application/nostr+json\r\nConnection: close\r\n\r\n");
    Ok(request)
}

fn write_tls(
    tls: &mut native_tls::TlsStream<TcpStream>,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), String> {
    arm_stream(tls.get_ref(), wait_budget(deadline, None)?)?;
    tls.write_all(bytes).map_err(nip11_io)?;
    arm_stream(tls.get_ref(), wait_budget(deadline, None)?)?;
    tls.flush().map_err(nip11_io)
}

fn read_tls(
    tls: &mut native_tls::TlsStream<TcpStream>,
    buf: &mut [u8],
    deadline: Instant,
) -> Result<usize, String> {
    loop {
        arm_stream(tls.get_ref(), wait_budget(deadline, None)?)?;
        match tls.read(buf) {
            Ok(read) => return Ok(read),
            Err(error)
                if error.kind() == std::io::ErrorKind::TimedOut
                    || error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::Interrupted =>
            {
                if relay_wait_is_over(deadline, None) {
                    return Err(RELAY_TIMEOUT.to_string());
                }
            }
            Err(error) => return Err(nip11_io(error)),
        }
    }
}

fn nip11_io(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::TimedOut
        || error.kind() == std::io::ErrorKind::WouldBlock
    {
        RELAY_TIMEOUT.to_string()
    } else {
        error.to_string()
    }
}

fn read_nip11_body(
    tls: &mut native_tls::TlsStream<TcpStream>,
    deadline: Instant,
) -> Result<String, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        if let Some(body) = decode_http_message(&buf, false)? {
            return Ok(body);
        }
        let read = read_tls(tls, &mut tmp, deadline)?;
        if read == 0 {
            return decode_http_message(&buf, true)?
                .ok_or_else(|| "relay sent no nip11 document".to_string());
        }
        buf.extend_from_slice(&tmp[..read]);
        if buf.len() > NIP11_HEAD_MAX + NIP11_BODY_MAX {
            return Err("relay sent no nip11 document".to_string());
        }
    }
}

pub(super) fn decode_http_message(bytes: &[u8], eof: bool) -> Result<Option<String>, String> {
    let Some(split) = header_end(bytes) else {
        if eof || bytes.len() > NIP11_HEAD_MAX {
            return Err("relay sent no nip11 document".to_string());
        }
        return Ok(None);
    };
    if split > NIP11_HEAD_MAX {
        return Err("relay sent no nip11 document".to_string());
    }
    let head = std::str::from_utf8(&bytes[..split])
        .map_err(|_| "relay sent no nip11 document".to_string())?;
    let (status, headers) = parse_http_head(head)?;
    if status != 200 || content_encoded(&headers) {
        return Err("relay sent no nip11 document".to_string());
    }
    let body = &bytes[split..];
    if transfer_chunked(&headers) {
        return match read_chunks(body)? {
            Some(body) => nip11_text(body),
            None if eof => Err("relay sent no nip11 document".to_string()),
            None => Ok(None),
        };
    }
    if let Some(length) = content_length(&headers)? {
        if length > NIP11_BODY_MAX || (eof && body.len() < length) {
            return Err("relay sent no nip11 document".to_string());
        }
        if body.len() < length {
            return Ok(None);
        }
        return nip11_text(body[..length].to_vec());
    }
    if body.len() > NIP11_BODY_MAX {
        return Err("relay sent no nip11 document".to_string());
    }
    if eof {
        nip11_text(body.to_vec())
    } else {
        Ok(None)
    }
}

fn nip11_text(body: Vec<u8>) -> Result<Option<String>, String> {
    let body = String::from_utf8(body).map_err(|_| "relay sent no nip11 document".to_string())?;
    if body.is_empty() {
        Err("relay sent no nip11 document".to_string())
    } else {
        Ok(Some(body))
    }
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|at| at + 4)
}

fn parse_http_head(head: &str) -> Result<(u16, Vec<(String, String)>), String> {
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| "relay sent no nip11 document".to_string())?;
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err("relay sent no nip11 document".to_string());
        };
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    Ok((status, headers))
}

fn content_encoded(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name == "content-encoding" && !value.is_empty() && !value.eq_ignore_ascii_case("identity")
    })
}

fn transfer_chunked(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name == "transfer-encoding"
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
    })
}

fn content_length(headers: &[(String, String)]) -> Result<Option<usize>, String> {
    let mut found = None;
    for (name, value) in headers {
        if name != "content-length" {
            continue;
        }
        let length = value
            .parse()
            .map_err(|_| "relay sent no nip11 document".to_string())?;
        if matches!(found, Some(previous) if previous != length) {
            return Err("relay sent no nip11 document".to_string());
        }
        found = Some(length);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::document_is_public;

    fn document(raw: &str) -> serde_json::Value {
        serde_json::from_str(raw).expect("nip11 fixture")
    }

    #[test]
    fn an_open_document_is_public() {
        let document = document(include_str!("../../test/nip11/open.json"));
        assert!(document_is_public(&document));
    }

    #[test]
    fn auth_required_is_not_public() {
        let document = document(include_str!("../../test/nip11/auth.json"));
        assert!(!document_is_public(&document));
    }

    #[test]
    fn payment_required_is_not_public() {
        let document = document(include_str!("../../test/nip11/payment.json"));
        assert!(!document_is_public(&document));
    }

    #[test]
    fn restricted_writes_are_not_public() {
        let document = document(include_str!("../../test/nip11/restricted.json"));
        assert!(!document_is_public(&document));
    }

    #[test]
    fn a_kind_list_that_contains_30323_is_public() {
        let document = document(include_str!("../../test/nip11/kinds-with.json"));
        assert!(document_is_public(&document));
    }

    #[test]
    fn a_kind_list_without_30323_is_not_public() {
        let document = document(include_str!("../../test/nip11/kinds-without.json"));
        assert!(!document_is_public(&document));
    }
}

fn read_chunks(mut rest: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let mut out = Vec::new();
    loop {
        let Some(line_end) = rest.windows(2).position(|window| window == b"\r\n") else {
            return Ok(None);
        };
        let line = std::str::from_utf8(&rest[..line_end])
            .map_err(|_| "relay sent no nip11 document".to_string())?;
        let size_hex = line.split(';').next().unwrap_or("").trim();
        if size_hex.is_empty() {
            return Err("relay sent no nip11 document".to_string());
        }
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| "relay sent no nip11 document".to_string())?;
        if size > NIP11_BODY_MAX || out.len().saturating_add(size) > NIP11_BODY_MAX {
            return Err("relay sent no nip11 document".to_string());
        }
        let data_at = line_end + 2;
        if size == 0 {
            if rest.len() < data_at + 2 {
                return Ok(None);
            }
            if &rest[data_at..data_at + 2] == b"\r\n" {
                return Ok(Some(out));
            }
            if rest[data_at..]
                .windows(4)
                .any(|window| window == b"\r\n\r\n")
            {
                return Ok(Some(out));
            }
            return Ok(None);
        }
        let data_end = data_at + size;
        if rest.len() < data_end + 2 {
            return Ok(None);
        }
        if &rest[data_end..data_end + 2] != b"\r\n" {
            return Err("relay sent no nip11 document".to_string());
        }
        out.extend_from_slice(&rest[data_at..data_end]);
        rest = &rest[data_end + 2..];
    }
}
