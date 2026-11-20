const NOTE_JSON_SKELETON: usize = 50;
const JSON_ESCAPE_EXPANSION: usize = 6;

pub fn sealed_payload_max(title_max: usize, alias_max: usize, content_max: usize) -> usize {
    worst_note_json_len(title_max, alias_max, content_max)
        .and_then(nostorgraph_nip44::encrypted_len)
        .map(|len| len.min(nostorgraph_nip44::MAX_PAYLOAD_CHARS))
        .unwrap_or(nostorgraph_nip44::MAX_PAYLOAD_CHARS)
}

fn worst_note_json_len(title_max: usize, alias_max: usize, content_max: usize) -> Option<usize> {
    let expanded = |len: usize| len.checked_mul(JSON_ESCAPE_EXPANSION);
    NOTE_JSON_SKELETON
        .checked_add(expanded(title_max)?)?
        .checked_add(expanded(alias_max)?)?
        .checked_add(expanded(content_max)?)?
        .checked_add(nostorgraph_nip44::LOCATOR_LEN)
}

#[cfg(test)]
mod tests {
    use super::{sealed_payload_max, worst_note_json_len, NOTE_JSON_SKELETON};

    #[test]
    fn sealed_payload_max_is_the_ciphertext_a_full_note_can_produce() {
        let empty = serde_json::json!({
            "title": "",
            "author": "",
            "content": "",
            "locator": "",
        })
        .to_string();
        assert_eq!(empty.len(), NOTE_JSON_SKELETON);

        let key = [1u8; 32];
        let nonce = [2u8; 32];
        for len in [1usize, 32, 33, 65_535, 65_536] {
            let payload = nostorgraph_nip44::encrypt(&"a".repeat(len), &key, &nonce).unwrap();
            assert_eq!(
                payload.len(),
                nostorgraph_nip44::encrypted_len(len).unwrap(),
                "{len}"
            );
        }

        let title_max = 8;
        let alias_max = 4;
        let content_max = 64;
        let title = "\u{1}".repeat(title_max);
        let author = "\u{1}".repeat(alias_max);
        let content = "\u{1}".repeat(content_max);
        let locator = "0123456789abcdef0123";
        let json = serde_json::json!({
            "title": title,
            "author": author,
            "content": content,
            "locator": locator,
        })
        .to_string();
        assert_eq!(
            json.len(),
            worst_note_json_len(title_max, alias_max, content_max).unwrap()
        );
        let secret = [3u8; 32];
        let sealed =
            nostorgraph_nip44::seal(&title, &author, &content, locator, &secret, &nonce).unwrap();
        let cap = sealed_payload_max(title_max, alias_max, content_max);
        assert_eq!(sealed.len(), cap);
        assert!(cap > content_max);
    }

    #[test]
    fn sealed_payload_max_stops_at_the_payload_decrypt_accepts() {
        assert_eq!(
            sealed_payload_max(128, 32, 256_000),
            nostorgraph_nip44::MAX_PAYLOAD_CHARS
        );
        assert_eq!(
            sealed_payload_max(usize::MAX, usize::MAX, usize::MAX),
            nostorgraph_nip44::MAX_PAYLOAD_CHARS
        );
    }
}
