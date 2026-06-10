//! Mostro source tag parser
//!
//! The Mostro daemon embeds routing metadata in kind 38383 order events via a
//! custom `source` tag with the format:
//!
//! ```text
//! mostro:{order_id}?relays={relay1},{relay2},...&mostro={node_pubkey_hex}
//! ```
//!
//! This module parses that string into structured routing info that a client
//! uses to:
//!
//! - Identify which Mostro daemon published the order
//! - Know which relays the order was published to (for trade-message routing)
//! - Look up the order's UUID (which is also its NIP-33 `d` tag)
//!
//! Reference: `mostro/src/nip33.rs:create_source_tag` (verified format).

use nostr::prelude::*;

/// Routing metadata extracted from a Mostro `source` tag.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ParsedSourceTag {
    /// Order UUID (also the NIP-33 `d` tag for the kind 38383 event).
    pub order_id: uuid::Uuid,
    /// Relay list embedded in the tag. Comma-separated, not URL-encoded.
    /// May be empty if the daemon omitted the `relays=` param.
    pub relays: Vec<String>,
    /// Hex-encoded public key of the Mostro daemon that published the order.
    pub mostro_pubkey: PublicKey,
}

impl ParsedSourceTag {
    /// Format back to the canonical source-tag string.
    #[allow(dead_code)]
    pub fn to_source_string(&self) -> String {
        let relays = self.relays.join(",");
        format!(
            "mostro:{}?relays={}&mostro={}",
            self.order_id,
            relays,
            self.mostro_pubkey.to_hex()
        )
    }
}

/// Parse a Mostro `source` tag string into structured routing info.
///
/// Returns `None` for any structural problem (missing scheme, malformed
/// UUID, missing or invalid node pubkey, etc.). Missing `relays=` is allowed
/// (returns an empty relay list) since some daemons may omit it.
#[allow(dead_code)]
pub fn parse_source_tag(source: &str) -> Option<ParsedSourceTag> {
    let source = source.trim();
    if !source.starts_with("mostro:") {
        return None;
    }
    let after_scheme = source.strip_prefix("mostro:")?;
    let (id_str, query) = after_scheme.split_once('?')?;
    let order_id = uuid::Uuid::parse_str(id_str).ok()?;

    let mut relays: Vec<String> = Vec::new();
    let mut mostro_pubkey: Option<PublicKey> = None;

    for pair in query.split('&') {
        if let Some(relay_csv) = pair.strip_prefix("relays=") {
            relays = relay_csv
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        } else if let Some(pk_hex) = pair.strip_prefix("mostro=") {
            if let Ok(pk) = PublicKey::from_hex(pk_hex) {
                mostro_pubkey = Some(pk);
            } else {
                return None;
            }
        }
        // Unknown params are ignored for forward-compat.
    }

    Some(ParsedSourceTag {
        order_id,
        relays,
        mostro_pubkey: mostro_pubkey?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real-world example from the mostro daemon's docs
    // (docs/SOURCE_TAG_PUBKEY.md:25)
    const REAL_EXAMPLE: &str = "mostro:e215c07e-b1f9-45b0-9640-0295067ee99a?relays=wss://relay.mostro.network,wss://nos.lol&mostro=82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390";

    #[test]
    fn test_parse_real_example() {
        let parsed = parse_source_tag(REAL_EXAMPLE).expect("should parse real example");
        assert_eq!(
            parsed.order_id.to_string(),
            "e215c07e-b1f9-45b0-9640-0295067ee99a"
        );
        assert_eq!(parsed.relays.len(), 2);
        assert_eq!(parsed.relays[0], "wss://relay.mostro.network");
        assert_eq!(parsed.relays[1], "wss://nos.lol");
        assert_eq!(
            parsed.mostro_pubkey.to_hex(),
            "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390"
        );
    }

    #[test]
    fn test_roundtrip() {
        let parsed = parse_source_tag(REAL_EXAMPLE).unwrap();
        let formatted = parsed.to_source_string();
        let re_parsed = parse_source_tag(&formatted).unwrap();
        assert_eq!(re_parsed.order_id, parsed.order_id);
        assert_eq!(re_parsed.relays, parsed.relays);
        assert_eq!(re_parsed.mostro_pubkey.to_hex(), parsed.mostro_pubkey.to_hex());
    }

    #[test]
    fn test_no_relays_param() {
        let src = "mostro:e215c07e-b1f9-45b0-9640-0295067ee99a?mostro=82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390";
        let parsed = parse_source_tag(src).expect("missing relays should still parse");
        assert!(parsed.relays.is_empty());
        assert!(!parsed.mostro_pubkey.to_hex().is_empty());
    }

    #[test]
    fn test_missing_mostro_param_returns_none() {
        let src = "mostro:e215c07e-b1f9-45b0-9640-0295067ee99a?relays=wss://relay.mostro.network";
        assert!(parse_source_tag(src).is_none());
    }

    #[test]
    fn test_invalid_uuid_returns_none() {
        let src = "mostro:not-a-uuid?mostro=82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390";
        assert!(parse_source_tag(src).is_none());
    }

    #[test]
    fn test_invalid_pubkey_returns_none() {
        let src = "mostro:e215c07e-b1f9-45b0-9640-0295067ee99a?mostro=zzzz";
        assert!(parse_source_tag(src).is_none());
    }

    #[test]
    fn test_wrong_scheme_returns_none() {
        assert!(parse_source_tag("notmostro:e215c07e-b1f9-45b0-9640-0295067ee99a?mostro=82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390").is_none());
        assert!(parse_source_tag("nostr:e215c07e").is_none());
    }

    #[test]
    fn test_missing_query_returns_none() {
        assert!(parse_source_tag("mostro:e215c07e-b1f9-45b0-9640-0295067ee99a").is_none());
    }

    #[test]
    fn test_empty_relay_entries_skipped() {
        // The daemon shouldn't emit empty entries, but be defensive.
        let src = "mostro:e215c07e-b1f9-45b0-9640-0295067ee99a?relays=wss://a,,wss://b,&mostro=82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390";
        let parsed = parse_source_tag(src).unwrap();
        assert_eq!(parsed.relays.len(), 2);
    }

    #[test]
    fn test_unknown_params_ignored() {
        let src = "mostro:e215c07e-b1f9-45b0-9640-0295067ee99a?relays=wss://x&mostro=82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390&future_param=ignored";
        let parsed = parse_source_tag(src).expect("unknown params should be ignored");
        assert_eq!(parsed.relays.len(), 1);
    }
}
