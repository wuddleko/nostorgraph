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
