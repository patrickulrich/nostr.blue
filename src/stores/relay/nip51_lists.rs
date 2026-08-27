//! Shared NIP-51 private relay-list codec.
//!
//! Relay-list kinds 10006 (blocked), 10007 (search), 10012 (relay feeds),
//! 10013 (private outbox), and the Amethyst-convention kinds 10086 (indexer),
//! 10087 (proxy), 10088 (broadcast), 10089 (trusted) all use the same wire
//! shape: plain replaceable events whose relay URLs may appear as public
//! `["relay", url]` tags and/or as a JSON tag array NIP-44-encrypted to the
//! author's own key and stored in `.content`. A conformant reader merges both
//! sources (NIP-51). An earlier revision of NIP-51 used NIP-04 for the
//! content encryption; per its compatibility clause readers detect the scheme
//! from the ciphertext (`?iv=` suffix = NIP-04) — we try NIP-44 first and fall
//! back to NIP-04 only when the payload carries the NIP-04 marker.
//!
//! Kind 10006/10007/10012 are standard NIP-51 lists; 10013 is defined by
//! NIP-37; 10086–10089 are **not** official NIP kinds — they follow the
//! Amethyst (quartz) convention, which is the interoperability target for
//! this app. Events of those kinds always carry a NIP-31 `alt` tag.

use std::collections::HashMap;
use std::sync::Arc;

use nostr::signer::NostrSigner;
use nostr_sdk::{Client, Event, EventBuilder, Kind, Tag, TagKind};

/// Every own relay-list kind handled by the unified loader in
/// [`crate::stores::relay::nip65`]. All share the codec in this module.
pub const OWN_RELAY_LIST_KINDS: [Kind; 8] = [
    Kind::BlockedRelays,   // 10006
    Kind::SearchRelays,    // 10007
    Kind::Custom(10012),   // relay feeds / favorites
    Kind::Custom(10013),   // private outbox (NIP-37)
    Kind::Custom(10086),   // indexer relays (Amethyst convention)
    Kind::Custom(10087),   // proxy relays (Amethyst convention)
    Kind::Custom(10088),   // broadcast relays (Amethyst convention)
    Kind::Custom(10089),   // trusted relays (Amethyst convention)
];

/// Whether an encrypted payload looks like NIP-04 (`base64?iv=base64`).
///
/// NIP-44 v2 payloads are plain base64 and never contain `?iv=`, so this is
/// a precise discriminator (Amethyst's `EncryptedInfo.isNIP04`).
pub fn is_nip04_content(content: &str) -> bool {
    content.contains("?iv=")
}

/// Serialize relay URLs as the NIP-51 private-items JSON tag array:
/// `[["relay","wss://a"],["relay","wss://b"]]`.
pub fn encode_relay_tags_json(urls: &[String]) -> String {
    let arr: Vec<Vec<String>> = urls
        .iter()
        .map(|url| vec!["relay".to_string(), url.clone()])
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}

/// Parse the NIP-51 private-items JSON tag array back into relay URLs.
///
/// Accepts both `"relay"` and `"r"` item tags (defensive: other clients have
/// historically emitted both for relay lists).
pub fn decode_relay_tags_json(json: &str) -> Vec<String> {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Vec<Vec<String>>>(trimmed) {
        Ok(arrays) => arrays
            .into_iter()
            .filter_map(|arr| match arr.first().map(|s| s.as_str()) {
                Some("relay" | "r") => arr.get(1).cloned(),
                _ => None,
            })
            .collect(),
        Err(e) => {
            log::warn!("Invalid private relay tags JSON: {}", e);
            Vec::new()
        }
    }
}

/// Merge two URL lists, deduplicating on a normalized form while preserving
/// first-seen order. Normalization matches the app's tag parsers
/// (`upgrade_to_secure_relay_url` + trailing-slash trim).
pub fn merge_urls(public: Vec<String>, private: Vec<String>) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for url in public.into_iter().chain(private) {
        let normalized = crate::utils::relay::upgrade_to_secure_relay_url(&url)
            .trim_end_matches('/')
            .to_lowercase();
        if seen.insert(normalized) {
            out.push(crate::utils::relay::upgrade_to_secure_relay_url(&url));
        }
    }
    out
}

/// Extract the public `["relay", url]` tags from an event.
pub fn extract_public_relay_tags(event: &Event) -> Vec<String> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            if tag.kind() == TagKind::Relay {
                tag.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Result of merging an event's public relay tags with its decrypted content.
#[derive(Clone, Debug, PartialEq)]
pub enum MergedRelayList {
    /// Both sources (or an empty content field) were read successfully —
    /// the merged set is authoritative and may legitimately be empty (the
    /// user cleared the list).
    Complete(Vec<String>),
    /// Decryption failed (e.g. a signer without NIP-44 support declined);
    /// only the public portion is known. Callers must not clobber seeded
    /// state with a partial read.
    Partial(Vec<String>),
}

impl MergedRelayList {
    pub fn urls(&self) -> &[String] {
        match self {
            MergedRelayList::Complete(v) | MergedRelayList::Partial(v) => v,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, MergedRelayList::Complete(_))
    }
}

/// Decrypt an event's `.content` with the supplied signer (NIP-44 first,
/// legacy NIP-04 when the payload carries the `?iv=` marker).
async fn decrypt_content_with_signer(
    signer: &dyn NostrSigner,
    event: &Event,
) -> Result<Vec<String>, String> {
    if event.content.is_empty() {
        return Ok(Vec::new());
    }
    let decrypted = match signer.nip44_decrypt(&event.pubkey, &event.content).await {
        Ok(json) => json,
        Err(e44) => {
            if !is_nip04_content(&event.content) {
                return Err(format!("NIP-44 decrypt failed: {e44}"));
            }
            signer
                .nip04_decrypt(&event.pubkey, &event.content)
                .await
                .map_err(|e04| format!("NIP-44: {e44}; NIP-04: {e04}"))?
        }
    };
    Ok(decode_relay_tags_json(&decrypted))
}

/// Merge an event's public relay tags with its decrypted private content.
///
/// Decrypt failures degrade to [`MergedRelayList::Partial`] (public tags
/// only) instead of blocking boot — the caller decides whether that is
/// enough to act on.
pub async fn merge_relay_list_event_with_signer(
    signer: &dyn NostrSigner,
    event: &Event,
) -> MergedRelayList {
    let public = extract_public_relay_tags(event);
    if event.content.is_empty() {
        return MergedRelayList::Complete(public);
    }
    match decrypt_content_with_signer(signer, event).await {
        Ok(private) => MergedRelayList::Complete(merge_urls(public, private)),
        Err(e) => {
            log::warn!(
                "Failed to decrypt private relay list (kind {}, event {}): {}",
                event.kind.as_u16(),
                event.id.to_hex().chars().take(8).collect::<String>(),
                e
            );
            MergedRelayList::Partial(public)
        }
    }
}

/// [`merge_relay_list_event_with_signer`] using the app's current signer.
pub async fn merge_relay_list_event(event: &Event) -> MergedRelayList {
    let Some(client) = crate::stores::nostr_client::get_client() else {
        return MergedRelayList::Partial(extract_public_relay_tags(event));
    };
    let Ok(signer) = client.signer().await else {
        return MergedRelayList::Partial(extract_public_relay_tags(event));
    };
    merge_relay_list_event_with_signer(signer.as_ref(), event).await
}

/// Encrypt a relay-URL list as NIP-44-to-self ciphertext for `.content`.
///
/// Encrypts `"[]"` when the list is empty (establishes the encrypted-list
/// pattern, matching the existing people-list behavior in
/// `utils/list_encryption.rs`).
pub async fn encrypt_relay_list_content(urls: &[String]) -> Result<String, String> {
    let client =
        crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let json = encode_relay_tags_json(urls);
    signer
        .nip44_encrypt(&pubkey, &json)
        .await
        .map_err(|e| format!("Failed to encrypt: {}", e))
}

/// Publish one of the user's private relay lists: relay URLs go into the
/// NIP-44-encrypted `.content` (no public relay tags — private-by-default,
/// matching Amethyst), plus a NIP-31 `alt` tag so other clients can describe
/// the event.
pub async fn publish_private_relay_list(
    kind: Kind,
    urls: Vec<String>,
    _client: Arc<Client>,
    alt: &str,
) -> Result<String, String> {
    let content = encrypt_relay_list_content(&urls).await?;
    let tags = vec![Tag::custom(
        TagKind::Custom("alt".into()),
        vec![alt.to_string()],
    )];
    let builder = EventBuilder::new(kind, content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}

/// Fold a mixed event batch to the newest event per list kind.
///
/// Uses the SDK `Ord for Event` semantics (descending `created_at`, then
/// ascending id): `current < incoming` means `current` is newer and wins —
/// the same fold as `relay::persistence::newest_relay_list_events`.
pub fn fold_newest_per_kind(events: Vec<Event>) -> HashMap<u16, Event> {
    let mut best: HashMap<u16, Event> = HashMap::new();
    for event in events {
        let kind = event.kind.as_u16();
        let replace = best.get(&kind).is_none_or(|current| current >= &event);
        if replace {
            best.insert(kind, event);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::nips::nip44::Version;
    use nostr::secp256k1::rand::rngs::OsRng;

    fn self_encrypt_nip44(keys: &nostr_sdk::Keys, json: &str) -> String {
        nostr::nips::nip44::encrypt_with_rng(
            &mut OsRng,
            keys.secret_key(),
            &keys.public_key(),
            json,
            Version::V2,
        )
        .unwrap()
    }

    fn self_encrypt_nip04(keys: &nostr_sdk::Keys, json: &str) -> String {
        nostr::nips::nip04::encrypt(keys.secret_key(), &keys.public_key(), json).unwrap()
    }

    fn signed_list_event(
        keys: &nostr_sdk::Keys,
        kind: Kind,
        content: &str,
        public_urls: &[&str],
        created_at: u64,
    ) -> Event {
        let tags: Vec<Tag> = public_urls
            .iter()
            .map(|u| Tag::custom(TagKind::Relay, vec![u.to_string()]))
            .collect();
        EventBuilder::new(kind, content.to_string())
            .tags(tags)
            .custom_created_at(nostr_sdk::Timestamp::from(created_at))
            .sign_with_keys(keys)
            .unwrap()
    }

    #[test]
    fn json_round_trip() {
        let urls = vec![
            "wss://relay.example".to_string(),
            "wss://other.example".to_string(),
        ];
        let json = encode_relay_tags_json(&urls);
        assert_eq!(json, r#"[["relay","wss://relay.example"],["relay","wss://other.example"]]"#);
        assert_eq!(decode_relay_tags_json(&json), urls);
        assert!(decode_relay_tags_json("[]").is_empty());
        assert!(decode_relay_tags_json("").is_empty());
        // Malformed JSON degrades to empty, not panic.
        assert!(decode_relay_tags_json("not json").is_empty());
        // Accepts legacy "r" item tags.
        assert_eq!(
            decode_relay_tags_json(r#"[["r","wss://relay.example"]]"#),
            vec!["wss://relay.example".to_string()]
        );
    }

    #[test]
    fn merge_urls_dedups_normalized() {
        let merged = merge_urls(
            vec!["wss://a.example".to_string()],
            vec![
                "wss://a.example/".to_string(),      // same relay, trailing slash
                "wss://A.example".to_string(),       // same relay, case
                "wss://b.example".to_string(),
            ],
        );
        assert_eq!(
            merged,
            vec![
                "wss://a.example".to_string(),
                "wss://b.example".to_string()
            ]
        );
    }

    #[test]
    fn nip04_detection() {
        assert!(is_nip04_content("c29tZXRoaW5n?iv=c29tZXRoaW5n"));
        assert!(!is_nip04_content("AgxydW5kb21vbWVudGF0aW9u"));
        assert!(!is_nip04_content(""));
    }

    #[tokio::test]
    async fn merge_public_and_nip44_content() {
        let keys = nostr_sdk::Keys::generate();
        let signer: Arc<dyn NostrSigner> = Arc::new(keys.clone());
        let content =
            self_encrypt_nip44(&keys, r#"[["relay","wss://private.example"]]"#);
        let event = signed_list_event(
            &keys,
            Kind::Custom(10086),
            &content,
            &["wss://public.example"],
            100,
        );
        let merged = merge_relay_list_event_with_signer(signer.as_ref(), &event).await;
        assert_eq!(
            merged,
            MergedRelayList::Complete(vec![
                "wss://public.example".to_string(),
                "wss://private.example".to_string()
            ])
        );
    }

    #[tokio::test]
    async fn merge_falls_back_to_nip04_content() {
        let keys = nostr_sdk::Keys::generate();
        let signer: Arc<dyn NostrSigner> = Arc::new(keys.clone());
        let content =
            self_encrypt_nip04(&keys, r#"[["relay","wss://legacy.example"]]"#);
        let event = signed_list_event(&keys, Kind::SearchRelays, &content, &[], 100);
        let merged = merge_relay_list_event_with_signer(signer.as_ref(), &event).await;
        assert_eq!(
            merged,
            MergedRelayList::Complete(vec!["wss://legacy.example".to_string()])
        );
    }

    #[tokio::test]
    async fn merge_decrypt_failure_is_partial() {
        // Encrypt with one key, decrypt with another -> failure.
        let author = nostr_sdk::Keys::generate();
        let other = nostr_sdk::Keys::generate();
        let signer: Arc<dyn NostrSigner> = Arc::new(other);
        let content =
            self_encrypt_nip44(&author, r#"[["relay","wss://private.example"]]"#);
        let event = signed_list_event(
            &author,
            Kind::Custom(10087),
            &content,
            &["wss://public.example"],
            100,
        );
        let merged = merge_relay_list_event_with_signer(signer.as_ref(), &event).await;
        assert_eq!(
            merged,
            MergedRelayList::Partial(vec!["wss://public.example".to_string()])
        );
    }

    #[tokio::test]
    async fn empty_content_is_complete_public_only() {
        let keys = nostr_sdk::Keys::generate();
        let signer: Arc<dyn NostrSigner> = Arc::new(keys.clone());
        let event =
            signed_list_event(&keys, Kind::BlockedRelays, "", &["wss://a.example"], 1);
        let merged = merge_relay_list_event_with_signer(signer.as_ref(), &event).await;
        assert_eq!(
            merged,
            MergedRelayList::Complete(vec!["wss://a.example".to_string()])
        );
    }

    #[test]
    fn fold_prefers_newest_per_kind() {
        let keys = nostr_sdk::Keys::generate();
        let older = signed_list_event(
            &keys,
            Kind::Custom(10086),
            "",
            &["wss://old.example"],
            100,
        );
        let newer = signed_list_event(
            &keys,
            Kind::Custom(10086),
            "",
            &["wss://new.example"],
            200,
        );
        let search = signed_list_event(&keys, Kind::SearchRelays, "", &["wss://s.example"], 50);
        // Older event arrives last — fold must be newest-wins.
        let folded = fold_newest_per_kind(vec![search.clone(), newer.clone(), older]);
        assert_eq!(folded.get(&10086).unwrap().id, newer.id);
        assert_eq!(folded.get(&10007).unwrap().id, search.id);
        assert_eq!(folded.len(), 2);
    }

    #[test]
    fn fold_tie_breaks_on_smaller_id() {
        let keys = nostr_sdk::Keys::generate();
        let a = signed_list_event(&keys, Kind::Custom(10088), "", &["wss://a.example"], 100);
        let b = signed_list_event(&keys, Kind::Custom(10088), "", &["wss://b.example"], 100);
        let (smaller, larger) = if a.id < b.id { (a, b) } else { (b, a) };
        let folded = fold_newest_per_kind(vec![smaller.clone(), larger]);
        assert_eq!(folded.get(&10088).unwrap().id, smaller.id);
    }
}
