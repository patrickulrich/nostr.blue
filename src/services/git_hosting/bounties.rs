//! Bounties Service
//!
//! Handles fetching and publishing Kind 9806 bounty events for git issues.
#![allow(dead_code)]
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::borrow::Cow;
use std::time::Duration;
use crate::stores::code_store::{cache_bounty_events, get_cached_bounties};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nip34::{Bounty, BountyStatus};
/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Fetch bounties for an issue by event ID hex
pub async fn fetch_bounties_for_issue(
    issue_event_id: &str,
) -> Result<Vec<Bounty>, String> {
    if let Some(bounties) = get_cached_bounties(issue_event_id) {
        return Ok(bounties);
    }
    let event_id = EventId::from_hex(issue_event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(Bounty::KIND))
        .event(event_id);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch bounties: {}", e))?;
    cache_bounty_events(&events);
    Ok(get_cached_bounties(issue_event_id).unwrap_or_default())
}
/// Publish a bounty on an issue
pub async fn publish_bounty(
    issue_event_id: EventId,
    amount_sats: u64,
    repository: Option<&Coordinate>,
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    if amount_sats == 0 {
        return Err("Bounty amount must be greater than 0".into());
    }
    let mut builder = EventBuilder::new(Kind::Custom(Bounty::KIND), "")
        .tag(Tag::event(issue_event_id))
        .tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("amount")),
            [amount_sats.to_string()],
        ))
        .tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("status")),
            ["pending"],
        ));
    if let Some(coord) = repository {
        builder = builder.tag(Tag::coordinate(coord.clone(), None));
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_bounty_events(&events);
    }
    Ok(event_id)
}
/// Update bounty status (publish a new Kind 9806 with updated status tag)
pub async fn update_bounty_status(
    bounty_event_id: EventId,
    issue_event_id: EventId,
    new_status: &str,
    amount_sats: u64,
    repository: Option<&Coordinate>,
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let mut builder = EventBuilder::new(Kind::Custom(Bounty::KIND), "")
        .tag(Tag::event(issue_event_id))
        .tag(Tag::event(bounty_event_id))
        .tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("amount")),
            [amount_sats.to_string()],
        ))
        .tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("status")),
            [new_status],
        ));
    if let Some(coord) = repository {
        builder = builder.tag(Tag::coordinate(coord.clone(), None));
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_bounty_events(&events);
    }
    Ok(event_id)
}
/// Claim a bounty (sets status to "claimed" and tags the claimer)
pub async fn claim_bounty(
    bounty_event_id: EventId,
    issue_event_id: EventId,
    amount_sats: u64,
    repository: Option<&Coordinate>,
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let signer = client.signer().await.map_err(|e| format!("Failed to get signer: {}", e))?;
    let claimer_pubkey = signer.get_public_key().await.map_err(|e| format!("Failed to get public key: {}", e))?;
    let mut builder = EventBuilder::new(Kind::Custom(Bounty::KIND), "")
        .tag(Tag::event(issue_event_id))
        .tag(Tag::event(bounty_event_id))
        .tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("amount")),
            [amount_sats.to_string()],
        ))
        .tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("status")),
            ["claimed"],
        ))
        .tag(Tag::public_key(claimer_pubkey));
    if let Some(coord) = repository {
        builder = builder.tag(Tag::coordinate(coord.clone(), None));
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_bounty_events(&events);
    }
    Ok(event_id)
}
/// Release a bounty by paying via NWC and marking as "paid"
///
/// Fetches the claimer's Lightning address from their profile,
/// generates an invoice, and pays via NWC.
pub async fn release_bounty(
    bounty_event_id: EventId,
    issue_event_id: EventId,
    claimer_pubkey: &str,
    amount_sats: u64,
    repository: Option<&Coordinate>,
) -> Result<EventId, String> {
    use crate::stores::nwc_store;
    use crate::stores::profiles::PROFILE_CACHE;

    // Check NWC is connected
    if !nwc_store::is_connected() {
        return Err("NWC wallet not connected. Connect a wallet in Settings first.".to_string());
    }

    // Re-fetch the bounty event to validate current state before payment
    let bounty_filter = Filter::new().id(bounty_event_id);
    let bounty_events = fetch_events_aggregated(bounty_filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to verify bounty state: {e}"))?;
    let bounty_event = bounty_events.first()
        .ok_or_else(|| "Bounty event not found".to_string())?;
    let current = Bounty::from_event(bounty_event)
        .ok_or_else(|| "Failed to parse bounty event".to_string())?;

    // Validate bounty is in "claimed" state
    if current.status != BountyStatus::Claimed {
        return Err(format!(
            "Bounty not in claimed state (current: {})",
            current.status.label()
        ));
    }

    // Validate claimer pubkey matches
    if current.claimer.as_deref() != Some(claimer_pubkey) {
        return Err("Claimer pubkey mismatch".to_string());
    }

    // Validate amount matches
    if current.amount_sats != amount_sats {
        return Err(format!(
            "Amount mismatch: expected {} sats, bounty has {} sats",
            amount_sats, current.amount_sats
        ));
    }

    // Look up claimer's lightning address
    let lud16 = {
        let cache = PROFILE_CACHE.read();
        cache.peek(claimer_pubkey).and_then(|p| p.lud16.clone())
    };
    let lud16 = lud16.ok_or_else(|| {
        "Could not find claimer's Lightning address. Their profile may not be loaded yet, or they may not have a lud16 set.".to_string()
    })?;

    // Generate invoice from LNURL
    let invoice = crate::services::payments::lnurl::get_invoice_from_lud16(&lud16, amount_sats, Some("Bounty payment"))
        .await
        .map_err(|e| format!("Failed to get invoice: {}", e))?;

    // Pay via NWC first — if payment fails, no status event is published
    nwc_store::pay_invoice(invoice)
        .await
        .map_err(|e| format!("Payment failed: {}", e))?;

    // Payment succeeded — now mark as paid
    let new_event_id = update_bounty_status(bounty_event_id, issue_event_id, "paid", amount_sats, repository).await?;

    Ok(new_event_id)
}

/// Fetch bounties for an issue by event ID string (convenience wrapper)
pub async fn fetch_bounties_for_issue_by_id(
    issue_ref: &str,
) -> Result<Vec<Bounty>, String> {
    use crate::utils::nip34::decode_event_id;
    let event_id = decode_event_id(issue_ref)
        .map_err(|e| format!("Invalid event reference: {}", e))?;
    fetch_bounties_for_issue(&event_id.to_hex()).await
}
