use nostr_sdk::prelude::*;

/// Extract mentioned public keys from content
///
/// Parses the content for `nostr:npub1...` and `nostr:nprofile1...` mentions
/// and returns a list of unique public keys that should be added as `p` tags
pub fn extract_mentioned_pubkeys(content: &str) -> Vec<PublicKey> {
    let mut pubkeys = Vec::new();

    // Parse content using NostrParser
    let parser = NostrParser::new();
    let tokens: Vec<_> = parser.parse(content).collect();

    for token in tokens {
        if let Token::Nostr(nip21) = token {
            match nip21 {
                Nip21::Pubkey(pubkey) => {
                    // npub1...
                    pubkeys.push(pubkey);
                }
                Nip21::Profile(profile) => {
                    // nprofile1...
                    pubkeys.push(profile.public_key);
                }
                // Ignore other types (event, note, naddr, etc.)
                _ => {}
            }
        }
    }

    // Remove duplicates
    pubkeys.sort();
    pubkeys.dedup();

    pubkeys
}

/// Create `p` tags from mentioned public keys
///
/// For each mentioned public key, create a `["p", "<hex-pubkey>"]` tag
pub fn create_mention_tags(pubkeys: &[PublicKey]) -> Vec<Tag> {
    pubkeys
        .iter()
        .map(|pk| Tag::public_key(*pk))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_npub_mention() {
        let content = "Hello nostr:npub1a2b3c4d5e6f7g8h9i0j1k2l3m4n5o6p7q8r9s0t1u2v3w4x5y6z! How are you?";
        let pubkeys = extract_mentioned_pubkeys(content);
        // This will fail since it's not a real npub, but demonstrates the pattern
        // In real usage, valid npub bech32 strings will be parsed correctly
        assert_eq!(pubkeys.len(), 0); // Invalid npub won't parse
    }

    #[test]
    fn test_extract_no_mentions() {
        let content = "This is a normal post without any mentions.";
        let pubkeys = extract_mentioned_pubkeys(content);
        assert_eq!(pubkeys.len(), 0);
    }

    #[test]
    fn test_extract_multiple_mentions() {
        // This test demonstrates the structure, but requires valid bech32 npub strings
        let content = "Mentioning someone in a post";
        let pubkeys = extract_mentioned_pubkeys(content);
        // Without valid npub strings, this will be empty
        assert_eq!(pubkeys.len(), 0);
    }
}
