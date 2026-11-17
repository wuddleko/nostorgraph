use url::Url;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Link {
    pub(super) key: [u8; nostorgraph_nip44::SECRET_LEN],
    pub(super) target: Target,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Target {
    Long {
        locator: String,
        event_id: String,
        relays: Vec<String>,
    },
    Short {
        origin: String,
        locator: String,
    },
    Title {
        origin: String,
        slug: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Error {
    Address,
    Fragment,
    Key,
    Path,
}

pub(super) fn parse(input: &str) -> Result<Link, Error> {
    let url = Url::parse(input).map_err(|_| Error::Address)?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::Address);
    }
    let origin = origin(&url).ok_or(Error::Address)?;
    let key = match url.fragment() {
        None | Some("") => return Err(Error::Fragment),
        Some(fragment) => decode_key(fragment).ok_or(Error::Key)?,
    };
    let mut segments = Vec::new();
    for segment in url.path_segments().ok_or(Error::Path)? {
        if segment.is_empty() {
            continue;
        }
        segments.push(decode_segment(segment)?);
    }
    if segments.first().map(String::as_str) == Some("s") {
        segments.remove(0);
    }
    let target = match segments.as_slice() {
        [locator, event_id] => {
            if !crate::nostr::sealed_locator(locator) || !crate::nostr::sealed_event_id(event_id) {
                return Err(Error::Path);
            }
            Target::Long {
                locator: locator.clone(),
                event_id: event_id.clone(),
                relays: relays(&url),
            }
        }
        [locator] if crate::nostr::sealed_locator(locator) => Target::Short {
            origin,
            locator: locator.clone(),
        },
        [slug] => Target::Title {
            origin,
            slug: slug.clone(),
        },
        _ => return Err(Error::Path),
    };
    Ok(Link { key, target })
}

fn origin(url: &Url) -> Option<String> {
    let host = match url.host()? {
        url::Host::Domain(domain) => domain.to_string(),
        url::Host::Ipv4(address) => address.to_string(),
        url::Host::Ipv6(address) => format!("[{address}]"),
    };
    let scheme = url.scheme();
    match url.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}")),
        None => Some(format!("{scheme}://{host}")),
    }
}

fn decode_segment(segment: &str) -> Result<String, Error> {
    let mut bytes = Vec::with_capacity(segment.len());
    let input = segment.as_bytes();
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' {
            if index + 2 >= input.len() {
                return Err(Error::Path);
            }
            let hex = std::str::from_utf8(&input[index + 1..index + 3]).map_err(|_| Error::Path)?;
            let byte = u8::from_str_radix(hex, 16).map_err(|_| Error::Path)?;
            bytes.push(byte);
            index += 3;
            continue;
        }
        bytes.push(input[index]);
        index += 1;
    }
    String::from_utf8(bytes).map_err(|_| Error::Path)
}

fn relays(url: &Url) -> Vec<String> {
    let mut relays = Vec::new();
    for (name, value) in url.query_pairs() {
        if name != "r" {
            continue;
        }
        let Some(relay) = crawler::canonical_public_relay(&value) else {
            continue;
        };
        if relays.iter().any(|have| have == &relay) {
            continue;
        }
        relays.push(relay);
    }
    relays
}

fn decode_key(text: &str) -> Option<[u8; nostorgraph_nip44::SECRET_LEN]> {
    const KEY_CHARS: usize = (nostorgraph_nip44::SECRET_LEN * 4 + 2) / 3;
    if text.len() != KEY_CHARS {
        return None;
    }
    let mut key = [0u8; nostorgraph_nip44::SECRET_LEN];
    let mut written = 0;
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            if written == key.len() {
                return None;
            }
            key[written] = (buffer >> bits) as u8;
            written += 1;
        }
    }
    if written == key.len() {
        Some(key)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const LOCATOR: &str = "0123456789abcdef0123";
    const EVENT_ID: &str = "abababababababababababababababababababababababababababababababab";

    fn parsed(input: &str) -> Link {
        parse(input).unwrap_or_else(|error| panic!("parse {input}: {error:?}"))
    }

    #[test]
    fn each_link_shape_parses() {
        let long = parsed(&format!(
            "https://example.test/s/{LOCATOR}/{EVENT_ID}?r=wss://relay.damus.io#{KEY}"
        ));
        assert_eq!(long.key, [0; 32]);
        match long.target {
            Target::Long {
                locator,
                event_id,
                relays,
            } => {
                assert_eq!(locator, LOCATOR);
                assert_eq!(event_id, EVENT_ID);
                assert_eq!(relays, vec!["wss://relay.damus.io".to_string()]);
            }
            Target::Short { .. } | Target::Title { .. } => panic!("expected a long link"),
        }

        let short = parsed(&format!("http://example.test:8443/{LOCATOR}#{KEY}"));
        assert_eq!(short.key, [0; 32]);
        match short.target {
            Target::Short { origin, locator } => {
                assert_eq!(origin, "http://example.test:8443");
                assert_eq!(locator, LOCATOR);
            }
            Target::Long { .. } | Target::Title { .. } => panic!("expected a short link"),
        }

        let title = parsed(&format!("https://example.test/My%20Note#{KEY}"));
        assert_eq!(title.key, [0; 32]);
        match title.target {
            Target::Title { origin, slug } => {
                assert_eq!(origin, "https://example.test");
                assert_eq!(slug, "My Note");
            }
            Target::Long { .. } | Target::Short { .. } => panic!("expected a title link"),
        }
    }

    #[test]
    fn a_missing_fragment_is_rejected() {
        assert_eq!(
            parse(&format!("https://example.test/{LOCATOR}")).unwrap_err(),
            Error::Fragment
        );
    }

    #[test]
    fn a_short_fragment_is_rejected() {
        assert_eq!(
            parse(&format!("https://example.test/{LOCATOR}#AA")).unwrap_err(),
            Error::Key
        );
    }

    #[test]
    fn a_bad_locator_is_rejected() {
        assert_eq!(
            parse(&format!(
                "https://example.test/0123456789ABCDEF0123/{EVENT_ID}#{KEY}"
            ))
            .unwrap_err(),
            Error::Path
        );
    }

    #[test]
    fn a_relay_that_is_not_public_is_dropped() {
        let link = parsed(&format!(
            "https://example.test/{LOCATOR}/{EVENT_ID}?r=wss://relay.damus.io&r=wss://192.168.1.1&r=wss://Relay.Damus.io/#{KEY}"
        ));
        match link.target {
            Target::Long { relays, .. } => {
                assert_eq!(relays, vec!["wss://relay.damus.io".to_string()]);
            }
            Target::Short { .. } | Target::Title { .. } => panic!("expected a long link"),
        }
    }
}
