use nostr_sdk::prelude::*;
/// Extract mentioned public keys from content
///
/// Parses the content for `nostr:npub1...` and `nostr:nprofile1...` mentions
/// and returns a list of unique public keys that should be added as `p` tags
pub fn extract_mentioned_pubkeys(content: &str) -> Vec<PublicKey> {
    let mut pubkeys = Vec::new();
    let parser = NostrParser::new();
    let tokens: Vec<_> = parser.parse(content).collect();
    for token in tokens {
        if let Token::Nostr(nip21) = token {
            match nip21 {
                Nip21::Pubkey(pubkey) => {
                    pubkeys.push(pubkey);
                }
                Nip21::Profile(profile) => {
                    pubkeys.push(profile.public_key);
                }
                _ => {}
            }
        }
    }
    pubkeys.sort();
    pubkeys.dedup();
    pubkeys
}
/// Create `p` tags from mentioned public keys
///
/// For each mentioned public key, create a `["p", "<hex-pubkey>"]` tag
pub fn create_mention_tags(pubkeys: &[PublicKey]) -> Vec<Tag> {
    pubkeys.iter().map(|pk| Tag::public_key(*pk)).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_extract_npub_mention() {
        let pk = PublicKey::from_slice(&[0u8; 32]).unwrap();
        let npub = pk.to_bech32().unwrap();
        let content = format!("Hello nostr:{}! How are you?", npub);
        let pubkeys = extract_mentioned_pubkeys(&content);
        assert_eq!(pubkeys.len(), 1);
        assert_eq!(pubkeys[0], pk);
    }
    #[test]
    fn test_extract_no_mentions() {
        let content = "This is a normal post without any mentions.";
        let pubkeys = extract_mentioned_pubkeys(content);
        assert_eq!(pubkeys.len(), 0);
    }
    #[test]
    fn test_extract_multiple_mentions() {
        let pk1 = PublicKey::from_slice(&[1u8; 32]).unwrap();
        let pk2 = PublicKey::from_slice(&[2u8; 32]).unwrap();
        let npub1 = pk1.to_bech32().unwrap();
        let npub2 = pk2.to_bech32().unwrap();
        let content = format!(
            "Hey nostr:{} and nostr:{}, check this out!",
            npub1,
            npub2,
        );
        let pubkeys = extract_mentioned_pubkeys(&content);
        assert_eq!(pubkeys.len(), 2);
        assert!(pubkeys.contains(&pk1));
        assert!(pubkeys.contains(&pk2));
    }
    #[test]
    fn test_extract_duplicate_mentions() {
        let pk = PublicKey::from_slice(&[3u8; 32]).unwrap();
        let npub = pk.to_bech32().unwrap();
        let content = format!(
            "nostr:{} said something, and nostr:{} replied",
            npub,
            npub,
        );
        let pubkeys = extract_mentioned_pubkeys(&content);
        assert_eq!(pubkeys.len(), 1);
        assert_eq!(pubkeys[0], pk);
    }
    #[test]
    fn test_create_mention_tags() {
        let pk1 = PublicKey::from_slice(&[4u8; 32]).unwrap();
        let pk2 = PublicKey::from_slice(&[5u8; 32]).unwrap();
        let tags = create_mention_tags(&[pk1, pk2]);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].kind(), nostr_sdk::TagKind::p());
        assert_eq!(tags[1].kind(), nostr_sdk::TagKind::p());
    }
}
