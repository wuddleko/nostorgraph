pub(super) fn open(
    event_json: &str,
    key: &[u8; nostorgraph_nip44::SECRET_LEN],
    locator: &str,
) -> Result<nostorgraph_nip44::Note, Error> {
    let event: serde_json::Value = serde_json::from_str(event_json).map_err(|_| Error::Event)?;
    let Some(content) = event.get("content").and_then(|value| value.as_str()) else {
        return Err(Error::Event);
    };
    match nostorgraph_nip44::open(content, key, locator) {
        Ok(note) => Ok(note),
        Err(
            nostorgraph_nip44::Error::Key
            | nostorgraph_nip44::Error::Mac
            | nostorgraph_nip44::Error::Locator,
        ) => Err(Error::Key),
        Err(_) => Err(Error::Event),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Error {
    Event,
    Key,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Error::Event => "The event is not a sealed note.",
            Error::Key => "The key does not open this note.",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCATOR: &str = "0123456789abcdef0123";
    const KEY: [u8; nostorgraph_nip44::SECRET_LEN] = [7; nostorgraph_nip44::SECRET_LEN];

    fn sealed_event() -> String {
        let payload = nostorgraph_nip44::seal("Title", "Ada", "hello", LOCATOR, &KEY, &[9; 32])
            .expect("seal");
        serde_json::json!({ "content": payload }).to_string()
    }

    #[test]
    fn a_sealed_event_opens() {
        let note = open(&sealed_event(), &KEY, LOCATOR).expect("open");
        assert_eq!(note.title, "Title");
        assert_eq!(note.author, "Ada");
        assert_eq!(note.content, "hello");
        assert_eq!(note.locator, LOCATOR);
    }

    #[test]
    fn a_wrong_key_does_not_open_the_note() {
        let wrong = [8; nostorgraph_nip44::SECRET_LEN];
        let error = open(&sealed_event(), &wrong, LOCATOR).unwrap_err();
        assert_eq!(error.to_string(), "The key does not open this note.");
    }

    #[test]
    fn a_locator_mismatch_uses_the_same_message() {
        let error = open(&sealed_event(), &KEY, "fedcba9876543210abcd").unwrap_err();
        assert_eq!(error.to_string(), "The key does not open this note.");
    }
}
