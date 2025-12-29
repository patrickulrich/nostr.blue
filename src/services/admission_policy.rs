#[cfg(target_arch = "wasm32")]
use nostr_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use nostr_sdk::FromBech32;
#[cfg(target_arch = "wasm32")]
use nostr_relay_pool::policy::{AdmitPolicy, AdmitStatus, PolicyError};
#[cfg(target_arch = "wasm32")]
use nostr::util::BoxedFuture;

/// Custom admission policy for nostr.blue
///
/// Filters events before they are stored in the database to:
/// - Block spam and malicious events
/// - Reduce database size
/// - Improve query performance
/// - Enhance user experience
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default)]
pub struct NostrBlueAdmissionPolicy;

#[cfg(target_arch = "wasm32")]
impl AdmitPolicy for NostrBlueAdmissionPolicy {
    fn admit_event<'a>(
        &'a self,
        _relay_url: &'a RelayUrl,
        _subscription_id: &'a SubscriptionId,
        event: &'a Event,
    ) -> BoxedFuture<'a, Result<AdmitStatus, PolicyError>> {
        Box::pin(async move {
            // 1. Block oversized events (prevent DoS attacks)
            // Typical text notes are <10KB, even long-form articles are <50KB
            if event.content.len() > 100_000 {
                log::warn!(
                    "Rejected oversized event {} from {} ({} bytes)",
                    event.id,
                    event.pubkey,
                    event.content.len()
                );
                return Ok(AdmitStatus::rejected("Event content too large (>100KB)"));
            }

            // 2. Validate event signature
            // This ensures the event hasn't been tampered with and was signed by the claimed author
            if let Err(e) = event.verify() {
                log::warn!(
                    "Rejected event {} with invalid signature: {}",
                    event.id,
                    e
                );
                return Ok(AdmitStatus::rejected("Invalid event signature"));
            }

            // 3. Check for expired events (NIP-40)
            // Events with an `expiration` tag should be rejected if past their expiration time
            if event.is_expired() {
                log::debug!(
                    "Rejected expired event {} from {}",
                    event.id,
                    event.pubkey
                );
                return Ok(AdmitStatus::rejected("Event has expired (NIP-40)"));
            }

            // 4. Check for protected events from other users (NIP-70)
            // Protected events (with `-` tag) should only be accepted from the current user
            if event.is_protected() {
                let current_pubkey = crate::stores::auth_store::get_pubkey();

                // Check if user is authenticated
                let Some(pk_str) = current_pubkey else {
                    log::debug!(
                        "Rejected protected event {} from {} (no authenticated user)",
                        event.id,
                        event.pubkey
                    );
                    return Ok(AdmitStatus::rejected(
                        "Protected event requires authenticated user (NIP-70)",
                    ));
                };

                // Parse stored pubkey (could be bech32 npub or hex format)
                let current_pk = match PublicKey::from_bech32(&pk_str)
                    .or_else(|_| PublicKey::from_hex(&pk_str))
                {
                    Ok(pk) => pk,
                    Err(e) => {
                        log::warn!(
                            "Failed to parse stored pubkey '{}': {} - rejecting protected event {}",
                            pk_str,
                            e,
                            event.id
                        );
                        return Ok(AdmitStatus::rejected("Invalid stored pubkey (NIP-70)"));
                    }
                };

                // Check if event is from the current user
                if current_pk != event.pubkey {
                    log::debug!(
                        "Rejected protected event {} from {} (not current user)",
                        event.id,
                        event.pubkey
                    );
                    return Ok(AdmitStatus::rejected(
                        "Protected event from other user (NIP-70)",
                    ));
                }
            }

            // 5. Future enhancements could include:
            // - Web of Trust filtering (check if author is in contact list or WoT graph)
            // - Content-based filtering (keywords, regex patterns)
            // - Rate limiting per pubkey
            // - Minimum proof-of-work requirements

            // Event passes all checks
            Ok(AdmitStatus::success())
        })
    }

    fn admit_connection<'a>(
        &'a self,
        _relay_url: &'a RelayUrl,
    ) -> BoxedFuture<'a, Result<AdmitStatus, PolicyError>> {
        // Allow all relay connections by default
        Box::pin(async move { Ok(AdmitStatus::success()) })
    }
}
