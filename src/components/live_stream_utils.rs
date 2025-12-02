//! Shared utilities for NIP-53 live streaming events

use nostr_sdk::Event as NostrEvent;
use nostr_sdk::nips::nip53::LiveEvent;
use nostr_sdk::{PublicKey, Alphabet, SingleLetterTag, TagKind};
use nostr_sdk::secp256k1::schnorr::Signature;
use std::str::FromStr;

/// Extracted host information with verification status
#[derive(Clone, Debug)]
pub struct ParsedLiveHost {
    /// Host's public key (hex string)
    pub public_key: String,
    /// Relay URL hint for the host
    #[allow(dead_code)]
    pub relay_url: Option<String>,
    /// Proof signature (hex string) - used during verification, stored for display/debugging
    #[allow(dead_code)]
    pub proof: Option<String>,
    /// Whether the proof signature is valid per NIP-53
    pub is_verified: bool,
}

/// Parse a NIP-53 Kind 30311 live streaming event into a LiveEvent struct.
/// This is the canonical parser used by all live stream components.
///
/// Returns `Some(LiveEvent)` on successful parse, `None` on failure (with warning logged).
pub fn parse_nip53_live_event(event: &NostrEvent) -> Option<LiveEvent> {
    match LiveEvent::try_from(event.tags.clone().to_vec()) {
        Ok(le) => Some(le),
        Err(e) => {
            log::warn!("Failed to parse LiveEvent from tags: {}", e);
            None
        }
    }
}

/// Extract host from live event with case-insensitive fallback.
///
/// The nostr-sdk LiveEventMarker parser is case-sensitive and only matches "Host".
/// Many streaming services use lowercase "host", so we need a fallback.
///
/// This function:
/// 1. Tries the SDK-parsed host first (case-sensitive "Host")
/// 2. Falls back to manual case-insensitive p tag parsing
/// 3. Verifies the proof signature if provided
pub fn extract_live_event_host(event: &NostrEvent, live_event: &LiveEvent) -> Option<ParsedLiveHost> {
    // 1. Try SDK-parsed host first (case-sensitive "Host")
    if let Some(host) = &live_event.host {
        let is_verified = verify_host_proof(event, &host.public_key, host.proof.as_ref());
        return Some(ParsedLiveHost {
            public_key: host.public_key.to_string(),
            relay_url: host.relay_url.as_ref().map(|u| u.to_string()),
            proof: host.proof.as_ref().map(|s| s.to_string()),
            is_verified,
        });
    }

    // 2. Fall back: manual case-insensitive p tag parsing
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) != Some("p") {
            continue;
        }

        // Check marker at index 3 (case-insensitive)
        if let Some(marker) = tag_vec.get(3) {
            if marker.to_lowercase() == "host" {
                let pubkey_str = match tag_vec.get(1) {
                    Some(pk) => pk,
                    None => continue,
                };
                let relay_url = tag_vec.get(2).filter(|s| !s.is_empty()).map(|s| s.to_string());
                let proof_str = tag_vec.get(4).filter(|s| !s.is_empty()).map(|s| s.to_string());

                // Try to parse pubkey
                let parsed_pubkey = match PublicKey::parse(pubkey_str) {
                    Ok(pk) => pk,
                    Err(_) => continue,
                };

                // Try to parse and verify proof
                let proof_sig = proof_str.as_ref()
                    .and_then(|p| Signature::from_str(p).ok());
                let is_verified = verify_host_proof(event, &parsed_pubkey, proof_sig.as_ref());

                return Some(ParsedLiveHost {
                    public_key: parsed_pubkey.to_string(),
                    relay_url,
                    proof: proof_str,
                    is_verified,
                });
            }
        }
    }

    None
}

/// Verify host proof per NIP-53.
///
/// Per NIP-53: "The proof is a signed SHA256 of the complete `a` Tag of the event
/// (`kind:pubkey:dTag`) by each `p`'s private key, encoded in hex."
///
/// Returns true if proof is valid, false if missing or invalid.
fn verify_host_proof(
    event: &NostrEvent,
    host_pubkey: &PublicKey,
    proof: Option<&Signature>,
) -> bool {
    use sha2::{Sha256, Digest};
    use nostr_sdk::secp256k1::{Secp256k1, Message, XOnlyPublicKey};

    let Some(proof_sig) = proof else {
        return false;  // No proof = unverified
    };

    // Get d tag from event
    let d_tag = event.tags.iter()
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D)))
        .and_then(|t| t.content())
        .unwrap_or("");

    // Message is SHA256 of "30311:{event_pubkey}:{d_tag}"
    let message = format!("30311:{}:{}", event.pubkey, d_tag);
    let message_hash = Sha256::digest(message.as_bytes());

    // Verify schnorr signature
    let secp = Secp256k1::verification_only();
    let msg = match Message::from_digest_slice(message_hash.as_ref()) {
        Ok(m) => m,
        Err(_) => return false,
    };

    // Get x-only pubkey (drop the parity byte)
    let pubkey_bytes = host_pubkey.to_bytes();
    // PublicKey is 33 bytes (compressed), x-only is 32 bytes (no prefix)
    let xonly_bytes = if pubkey_bytes.len() == 33 {
        &pubkey_bytes[1..]
    } else {
        &pubkey_bytes
    };

    match XOnlyPublicKey::from_slice(xonly_bytes) {
        Ok(xonly) => secp.verify_schnorr(proof_sig, &msg, &xonly).is_ok(),
        Err(_) => false,
    }
}
