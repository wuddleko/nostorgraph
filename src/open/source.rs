use url::Url;

use super::link::{Link, Target};

#[derive(Debug)]
pub(super) struct Loaded {
    pub(super) event: String,
    pub(super) locator: String,
}

pub(super) fn load(link: &Link) -> Result<Loaded, Error> {
    let config = crate::config::Config::load().map_err(Error::Config)?;
    load_with(&config, link)
}

fn load_with(config: &crate::config::Config, link: &Link) -> Result<Loaded, Error> {
    let socks = match config.socks_addr() {
        Ok(Some(addr)) => crate::nostr::RelaySocks::Proxy(addr),
        Ok(None) => crate::nostr::RelaySocks::Direct,
        Err(()) => return Err(Error::Socks),
    };
    crate::nostr::set_relay_socks(socks);
    match &link.target {
        Target::Long {
            locator,
            event_id,
            relays,
        } => {
            if relays.is_empty() {
                return Err(Error::Relays);
            }
            let event = crate::nostr::fetch_sealed(
                relays,
                event_id,
                locator,
                config.limits.title_max_length,
                config.limits.alias_max_length,
                config.limits.content_max_length,
                crate::relay_set::relay_timeout(config),
            )
            .ok_or(Error::Relays)?;
            Ok(Loaded {
                event,
                locator: locator.clone(),
            })
        }
        Target::Short { origin, locator } => fetch_host(config, socks, origin, locator, link),
        Target::Title { origin, slug } => fetch_host(config, socks, origin, slug, link),
    }
}

fn fetch_host(
    config: &crate::config::Config,
    socks: crate::nostr::RelaySocks,
    origin: &str,
    segment: &str,
    link: &Link,
) -> Result<Loaded, Error> {
    if host_is_onion(origin) && !matches!(socks, crate::nostr::RelaySocks::Proxy(_)) {
        return Err(Error::Fetch(
            "An onion host needs [nostr].socks.".to_string(),
        ));
    }
    let request = page_request(origin, segment);
    let page = page_target(origin, segment);
    let deadline = std::time::Instant::now() + crate::relay_set::relay_timeout(config);
    let max_body = crate::seal::sealed_payload_max(
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
    )
    .saturating_add(PAGE_SHELL);
    let html = match crate::nostr::fetch_http_page(&page, &request, deadline, max_body, socks) {
        Ok(html) => html,
        Err(crate::nostr::PageError::TooLarge) => {
            return Err(Error::Fetch("The page is too large.".to_string()));
        }
        Err(crate::nostr::PageError::Down) => {
            return Err(Error::Fetch("The page did not respond.".to_string()));
        }
    };
    let (event, locator) = event_from_page(link, &html, config)?;
    Ok(Loaded { event, locator })
}

const PAGE_SHELL: usize = 256 * 1024;

fn host_is_onion(origin: &str) -> bool {
    Url::parse(&format!("{origin}/"))
        .ok()
        .and_then(|url| url.host_str().map(|host| host.ends_with(".onion")))
        .unwrap_or(false)
}

fn page_target(origin: &str, segment: &str) -> String {
    let mut url = Url::parse(&format!("{origin}/")).expect("page origin");
    url.path_segments_mut()
        .expect("page origin")
        .clear()
        .push(segment);
    url.to_string()
}

pub(super) fn page_request(origin: &str, segment: &str) -> String {
    let url = page_target(origin, segment);
    let url = Url::parse(&url).expect("page origin");
    let host = host_header(&url).expect("page host");
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n",
        path = url.path()
    )
}

fn host_header(url: &Url) -> Option<String> {
    let host = match url.host()? {
        url::Host::Domain(domain) => domain.to_string(),
        url::Host::Ipv4(address) => address.to_string(),
        url::Host::Ipv6(address) => format!("[{address}]"),
    };
    match url.port() {
        Some(port) => Some(format!("{host}:{port}")),
        None => Some(host),
    }
}

fn read_marks(html: &str) -> Result<PageMarks, MarkError> {
    let events = element_text(html, "sealed-event").ok_or(MarkError::Event)?;
    let events: Vec<serde_json::Value> =
        serde_json::from_str(events).map_err(|_| MarkError::Json)?;
    let locator = match element_text(html, "sealed-locator") {
        None => None,
        Some(locator) if crate::nostr::sealed_locator(locator) => Some(locator.to_string()),
        Some(_) => return Err(MarkError::Locator),
    };
    Ok(PageMarks { events, locator })
}

fn event_from_page(
    link: &Link,
    html: &str,
    config: &crate::config::Config,
) -> Result<(String, String), Error> {
    let marks = read_marks(html).map_err(|_| Error::Page)?;
    let wanted = match &link.target {
        Target::Short { locator, .. } => locator.clone(),
        Target::Title { .. } => marks.locator.clone().ok_or(Error::Page)?,
        Target::Long { .. } => return Err(Error::Shape),
    };
    for event in &marks.events {
        let Some((_, locator)) = crate::nostr::sealed_note_from_json(
            event,
            config.limits.title_max_length,
            config.limits.alias_max_length,
            config.limits.content_max_length,
        ) else {
            continue;
        };
        if locator == wanted {
            let event = serde_json::to_string(event).map_err(|_| Error::Page)?;
            return Ok((event, wanted));
        }
    }
    Err(Error::Page)
}

fn element_text<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let marker = format!("id=\"{id}\"");
    let start = html.find(&marker)?;
    let after = &html[start + marker.len()..];
    let tag_end = after.find('>')?;
    let body = &after[tag_end + 1..];
    let close = body.find("</div>")?;
    Some(body[..close].trim())
}

#[derive(Debug, PartialEq, Eq)]
struct PageMarks {
    events: Vec<serde_json::Value>,
    locator: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum MarkError {
    Event,
    Locator,
    Json,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Error {
    Config(String),
    Socks,
    Relays,
    Shape,
    Page,
    Fetch(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Config(message) => f.write_str(message),
            Error::Socks => {
                f.write_str("[nostr].socks is not a socket address; relay connections are disabled")
            }
            Error::Relays => f.write_str("The relays did not return this note."),
            Error::Shape => f.write_str("That link is not a long link."),
            Error::Page => f.write_str("This page has no sealed note."),
            Error::Fetch(message) => f.write_str(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn long_link(relays: &[&str]) -> Link {
        Link {
            key: [0; nostorgraph_nip44::SECRET_LEN],
            target: Target::Long {
                locator: "0123456789abcdef0123".to_string(),
                event_id: "ab".repeat(32),
                relays: relays.iter().map(|relay| (*relay).to_string()).collect(),
            },
        }
    }

    #[test]
    fn load_reads_config_and_rejects_an_empty_relay_list() {
        let error = load(&long_link(&[])).unwrap_err();
        assert_eq!(error.to_string(), "The relays did not return this note.");
    }

    #[test]
    fn an_empty_relay_list_fails_before_a_fetch() {
        let error = load_with(&Config::default(), &long_link(&[])).unwrap_err();
        assert_eq!(error.to_string(), "The relays did not return this note.");
    }

    #[test]
    fn a_short_and_title_page_request_keeps_the_host_and_drops_the_key() {
        let key = "A".repeat(43);
        let short = crate::open::link::parse(&format!(
            "https://example.test/s/0123456789abcdef0123#{key}"
        ))
        .expect("short link");
        let short_request = request_for(&short);
        assert!(short_request.starts_with("GET /0123456789abcdef0123 HTTP/1.1\r\n"));
        assert!(short_request.contains("\r\nHost: example.test\r\n"));
        assert!(!short_request.contains("/s/"));
        assert!(!short_request.contains('?'));
        assert!(!short_request.contains('#'));
        assert!(!short_request.contains(&key));

        let title = crate::open::link::parse(&format!("http://example.test:8443/My%20Note#{key}"))
            .expect("title link");
        let title_request = request_for(&title);
        assert!(title_request.starts_with("GET /My%20Note HTTP/1.1\r\n"));
        assert!(title_request.contains("\r\nHost: example.test:8443\r\n"));
        assert!(!title_request.contains('?'));
        assert!(!title_request.contains('#'));
        assert!(!title_request.contains(&key));
    }

    fn request_for(link: &Link) -> String {
        match &link.target {
            Target::Short { origin, locator } => page_request(origin, locator),
            Target::Title { origin, slug } => page_request(origin, slug),
            Target::Long { .. } => panic!("a long link has no host page"),
        }
    }

    #[test]
    fn saved_pages_expose_the_event_and_the_locator() {
        let short = r#"<div id="sealed-event" hidden>[{"content":"hello"}]</div>"#;
        let short = read_marks(short).expect("short page");
        assert!(short.locator.is_none());
        assert_eq!(short.events[0]["content"], "hello");

        let title = r#"<div id="sealed-locator" hidden>0123456789abcdef0123</div><div id="sealed-event" hidden>[{"content":"note"}]</div>"#;
        let title = read_marks(title).expect("title page");
        assert_eq!(title.locator.as_deref(), Some("0123456789abcdef0123"));
        assert_eq!(title.events[0]["content"], "note");

        let missing = r#"<div id="sealed-locator" hidden>0123456789abcdef0123</div>"#;
        assert_eq!(read_marks(missing), Err(MarkError::Event));

        let escaped = r#"<div id="sealed-event" hidden>[{"content":"a\u003cb"}]</div>"#;
        let escaped = read_marks(escaped).expect("escaped page");
        assert_eq!(escaped.events[0]["content"], "a<b");

        let bad_locator = r#"<div id="sealed-locator" hidden>not-a-locator</div><div id="sealed-event" hidden>[]</div>"#;
        assert_eq!(read_marks(bad_locator), Err(MarkError::Locator));
    }

    const LOCATOR: &str = "0123456789abcdef0123";
    const OTHER: &str = "fedcba9876543210abcd";

    #[test]
    fn the_first_matching_event_is_kept() {
        let config = Config::default();
        let right = signed_event(LOCATOR, "hello");
        let other = signed_event(OTHER, "nope");
        let array = serde_json::to_string(&vec![other, right.clone()]).expect("array");
        let short_page = format!(r#"<div id="sealed-event" hidden>{array}</div>"#);
        let (kept, locator) =
            event_from_page(&short_link(LOCATOR), &short_page, &config).expect("short");
        assert_eq!(locator, LOCATOR);
        let kept: serde_json::Value = serde_json::from_str(&kept).expect("event");
        assert_eq!(kept["content"], right["content"]);

        let empty = r#"<div id="sealed-event" hidden>[]</div>"#;
        assert_eq!(
            event_from_page(&short_link(LOCATOR), empty, &config)
                .unwrap_err()
                .to_string(),
            "This page has no sealed note."
        );
        assert_eq!(
            event_from_page(&short_link(LOCATOR), "<p>none</p>", &config)
                .unwrap_err()
                .to_string(),
            "This page has no sealed note."
        );

        let content = right["content"].clone();
        let title_page = format!(
            r#"<div id="sealed-locator" hidden>{LOCATOR}</div><div id="sealed-event" hidden>{}</div>"#,
            serde_json::to_string(&vec![right]).expect("array")
        );
        let (kept, locator) = event_from_page(&title_link(), &title_page, &config).expect("title");
        assert_eq!(locator, LOCATOR);
        let kept: serde_json::Value = serde_json::from_str(&kept).expect("event");
        assert_eq!(kept["content"], content);

        let mismatch = format!(
            r#"<div id="sealed-event" hidden>{}</div>"#,
            serde_json::to_string(&vec![signed_event(OTHER, "nope")]).expect("array")
        );
        assert_eq!(
            event_from_page(&short_link(LOCATOR), &mismatch, &config)
                .unwrap_err()
                .to_string(),
            "This page has no sealed note."
        );
    }

    fn signed_event(locator: &str, body: &str) -> serde_json::Value {
        let payload = nostorgraph_nip44::seal("Title", "Ada", body, locator, &[7; 32], &[9; 32])
            .expect("seal");
        let note = crate::nostr::sign_sealed_note(&payload, locator, 1_700_000_000);
        serde_json::from_str(&note.event_json).expect("event json")
    }

    fn short_link(locator: &str) -> Link {
        Link {
            key: [0; nostorgraph_nip44::SECRET_LEN],
            target: Target::Short {
                origin: "https://example.test".to_string(),
                locator: locator.to_string(),
            },
        }
    }

    fn title_link() -> Link {
        Link {
            key: [0; nostorgraph_nip44::SECRET_LEN],
            target: Target::Title {
                origin: "https://example.test".to_string(),
                slug: "My Note".to_string(),
            },
        }
    }

    #[test]
    fn an_onion_host_needs_the_proxy() {
        let link = Link {
            key: [0; nostorgraph_nip44::SECRET_LEN],
            target: Target::Short {
                origin: "http://example.onion".to_string(),
                locator: "0123456789abcdef0123".to_string(),
            },
        };
        let error = load_with(&Config::default(), &link).unwrap_err();
        assert_eq!(error.to_string(), "An onion host needs [nostr].socks.");
    }

    #[test]
    fn a_short_page_is_one_request_and_a_redirect_is_not_followed() {
        let event = signed_event("0123456789abcdef0123", "from-page");
        let body = format!(
            r#"<div id="sealed-event" hidden>{}</div>"#,
            serde_json::to_string(&vec![event.clone()]).expect("array")
        );
        let (origin, seen) = serve_once(200, &body);
        let mut config = Config::default();
        config.nostr.timeout_secs = 3;
        let loaded = load_with(&config, &short_at(&origin)).expect("page");
        assert_eq!(loaded.locator, "0123456789abcdef0123");
        let kept: serde_json::Value = serde_json::from_str(&loaded.event).expect("event");
        assert_eq!(kept["content"], event["content"]);
        let request = seen
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("request");
        assert!(request.starts_with("GET /0123456789abcdef0123 HTTP/1.1\r\n"));
        assert!(!request.contains('#'));

        let (origin, seen) = serve_once(302, "<p>moved</p>");
        let error = load_with(&config, &short_at(&origin)).unwrap_err();
        assert_eq!(error.to_string(), "This page has no sealed note.");
        let request = seen
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("redirect request");
        assert_eq!(request.matches("GET ").count(), 1);
    }

    fn short_at(origin: &str) -> Link {
        Link {
            key: [0; nostorgraph_nip44::SECRET_LEN],
            target: Target::Short {
                origin: origin.to_string(),
                locator: "0123456789abcdef0123".to_string(),
            },
        }
    }

    fn serve_once(status: u16, body: &str) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("port").port();
        let body = body.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            loop {
                match sock.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(read) => {
                        buf.extend_from_slice(&tmp[..read]);
                        if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\nLocation: http://127.0.0.1/elsewhere\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(response.as_bytes());
        });
        (format!("http://127.0.0.1:{port}"), rx)
    }

    #[test]
    fn a_bad_socks_address_fails_before_a_fetch() {
        let mut config = Config::default();
        config.nostr.socks = "not-a-socket".to_string();
        let error = load_with(&config, &long_link(&["wss://relay.damus.io"])).unwrap_err();
        assert_eq!(
            error.to_string(),
            "[nostr].socks is not a socket address; relay connections are disabled"
        );
    }
}
