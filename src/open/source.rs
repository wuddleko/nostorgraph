use url::Url;

use super::link::{Link, Target};

pub(super) fn load(link: &Link) -> Result<String, Error> {
    let config = crate::config::Config::load().map_err(Error::Config)?;
    load_with(&config, link)
}

fn load_with(config: &crate::config::Config, link: &Link) -> Result<String, Error> {
    let Target::Long {
        locator,
        event_id,
        relays,
    } = &link.target
    else {
        return Err(Error::Shape);
    };
    if relays.is_empty() {
        return Err(Error::Relays);
    }
    let socks = match config.socks_addr() {
        Ok(Some(addr)) => crate::nostr::RelaySocks::Proxy(addr),
        Ok(None) => crate::nostr::RelaySocks::Direct,
        Err(()) => return Err(Error::Socks),
    };
    crate::nostr::set_relay_socks(socks);
    crate::nostr::fetch_sealed(
        relays,
        event_id,
        locator,
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
        crate::relay_set::relay_timeout(config),
    )
    .ok_or(Error::Relays)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn page_request(origin: &str, segment: &str) -> String {
    let mut url = Url::parse(&format!("{origin}/")).expect("page origin");
    url.path_segments_mut()
        .expect("page origin")
        .clear()
        .push(segment);
    let host = host_header(&url).expect("page host");
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n",
        path = url.path()
    )
}

#[cfg_attr(not(test), allow(dead_code))]
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

#[cfg_attr(not(test), allow(dead_code))]
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
#[cfg_attr(not(test), allow(dead_code))]
struct PageMarks {
    events: Vec<serde_json::Value>,
    locator: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
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
