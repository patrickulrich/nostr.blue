use nostr_sdk::prelude::*;
use nostr_relay_pool::policy::{AdmitPolicy, AdmitStatus, PolicyError};
use nostr::util::BoxedFuture;

/// Custom admission policy for nostr.blue
///
/// Filters events before they are stored in the database to:
/// - Block spam and malicious events
/// - Reduce database size
/// - Improve query performance
/// - Enhance user experience
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

            // 3. Block known spam event kinds
            // Kind 9999 is commonly used for spam/testing
            match event.kind.as_u16() {
                9999 => {
                    log::info!(
                        "Rejected spam kind 9999 event {} from {}",
                        event.id,
                        event.pubkey
                    );
                    return Ok(AdmitStatus::rejected("Spam event kind blocked"));
                }
                _ => {}
            }

            // 4. Future enhancements could include:
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
