mod browse;
mod document;
mod link;
mod note;
mod source;

pub fn run() -> i32 {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match attempt(&arguments) {
        Ok(path) => {
            println!("{path}");
            0
        }
        Err(failure) => {
            eprintln!("{}", failure.message);
            failure.code
        }
    }
}

fn attempt(arguments: &[String]) -> Result<String, Failure> {
    let [input] = arguments else {
        return Err(Failure {
            message: "Usage: ntgraph <link>".to_string(),
            code: 2,
        });
    };
    let parsed = link::parse(input).map_err(link_message)?;
    let loaded = source::load(&parsed).map_err(|error| failure(error.to_string()))?;
    let opened = note::open(&loaded.event, &parsed.key, &loaded.locator)
        .map_err(|error| failure(error.to_string()))?;
    let page = document::render(&opened).map_err(failure)?;
    let path = browse::open(&loaded.locator, &page).map_err(|error| failure(error.to_string()))?;
    Ok(path.display().to_string())
}

fn link_message(error: link::Error) -> Failure {
    let message = match error {
        link::Error::Fragment => {
            "The link has no key. Quote the URL so the shell keeps the part after #."
        }
        link::Error::Key => "The key is not 32 bytes.",
        link::Error::Address | link::Error::Path => "The link is not a note link.",
    };
    failure(message.to_string())
}

fn failure(message: String) -> Failure {
    Failure { message, code: 1 }
}

struct Failure {
    message: String,
    code: i32,
}

#[cfg(test)]
mod tests {
    use super::attempt;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn zero_or_two_arguments_are_usage() {
        let missing = attempt(&[]).unwrap_err();
        assert_eq!(missing.code, 2);
        assert_eq!(missing.message, "Usage: ntgraph <link>");
        let two = attempt(&arguments(&[
            "https://example.test/0123456789abcdef0123",
            "https://example.test/other",
        ]))
        .unwrap_err();
        assert_eq!(two.code, 2);
        assert_eq!(two.message, "Usage: ntgraph <link>");
    }

    #[test]
    fn a_missing_fragment_says_to_quote_the_url() {
        let failure =
            attempt(&arguments(&["https://example.test/0123456789abcdef0123"])).unwrap_err();
        assert_eq!(failure.code, 1);
        assert_eq!(
            failure.message,
            "The link has no key. Quote the URL so the shell keeps the part after #."
        );
        assert!(!failure.message.contains("example.test"));
    }

    #[test]
    fn a_long_link_with_no_relay_stops_with_one_line() {
        let key = "A".repeat(43);
        let link = format!(
            "https://example.test/0123456789abcdef0123/{}#{key}",
            "ab".repeat(32)
        );
        let failure = attempt(&arguments(&[&link])).unwrap_err();
        assert_eq!(failure.code, 1);
        assert_eq!(failure.message, "The relays did not return this note.");
        assert!(!failure.message.contains(&key));
        assert!(!failure.message.contains("example.test"));
    }
}
