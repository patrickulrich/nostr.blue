use nostr::util::BoxedFuture;
use nostr_relay_pool::policy::{AdmitPolicy, AdmitStatus, PolicyError};
use nostr_sdk::prelude::*;
use nostr_sdk::FromBech32;
/// Custom admission policy for nostr.blue
///
/// Filters events before they are stored in the database to:
/// - Block spam and malicious events
/// - Reduce database size
/// - Improve query performance
/// - Enhance user experience
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct NostrBlueAdmissionPolicy;
impl AdmitPolicy for NostrBlueAdmissionPolicy {
    fn admit_event<'a>(
        &'a self,
        _relay_url: &'a RelayUrl,
        _subscription_id: &'a SubscriptionId,
        event: &'a Event,
    ) -> BoxedFuture<'a, Result<AdmitStatus, PolicyError>> {
        Box::pin(async move {
            if event.content.len() > 100_000 {
                log::warn!(
                    "Rejected oversized event {} from {} ({} bytes)",
                    event.id,
                    event.pubkey,
                    event.content.len()
                );
                return Ok(AdmitStatus::rejected("Event content too large (>100KB)"));
            }
            if let Err(e) = event.verify() {
                log::warn!("Rejected event {} with invalid signature: {}", event.id, e);
                return Ok(AdmitStatus::rejected("Invalid event signature"));
            }
            if event.is_expired() {
                log::debug!("Rejected expired event {} from {}", event.id, event.pubkey);
                return Ok(AdmitStatus::rejected("Event has expired (NIP-40)"));
            }
            if event.is_protected() {
                let current_pubkey = crate::stores::auth_store::get_pubkey();
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
                let current_pk =
                    match PublicKey::from_bech32(&pk_str).or_else(|_| PublicKey::from_hex(&pk_str))
                    {
                        Ok(pk) => pk,
                        Err(e) => {
                            log::warn!(
                            "Failed to parse stored pubkey '{}': {} - rejecting protected event {}",
                            pk_str, e, event.id
                        );
                            return Ok(AdmitStatus::rejected("Invalid stored pubkey (NIP-70)"));
                        }
                    };
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
            Ok(AdmitStatus::success())
        })
    }
    fn admit_connection<'a>(
        &'a self,
        _relay_url: &'a RelayUrl,
    ) -> BoxedFuture<'a, Result<AdmitStatus, PolicyError>> {
        Box::pin(async move { Ok(AdmitStatus::success()) })
    }
}
