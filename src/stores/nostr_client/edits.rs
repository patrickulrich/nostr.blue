use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use crate::stores::relay;
use crate::utils::custom_emoji::build_custom_emoji_tags;
use crate::utils::mention_extractor::{create_mention_tags, extract_mentioned_pubkeys};
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

pub const KIND_NOTE_EDIT: u16 = 1010;

pub struct EditPublishResult {
    pub event: Event,
    pub publish: PublishResult,
}

fn extract_relay_hint(original_event: &Event) -> Option<RelayUrl> {
    for tag in original_event.tags.iter() {
        if let Some(TagStandard::Event { relay_url: Some(url), .. }) = tag.as_standardized() {
            return Some(url.clone());
        }
    }
    relay::nip65::get_write_relays()
        .first()
        .and_then(|url| RelayUrl::parse(url).ok())
}

pub async fn publish_edit(
    original_event: &Event,
    content: String,
    summary: Option<String>,
    notify_pubkey: Option<PublicKey>,
) -> std::result::Result<EditPublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let original_event_id = original_event.id;
    log::info!(
        "Publishing edit for note {}",
        &original_event_id.to_hex()[..16.min(original_event_id.to_hex().len())]
    );
    let mentioned_pubkeys = extract_mentioned_pubkeys(&content);
    let mut mention_tags = create_mention_tags(&mentioned_pubkeys);
    let custom_emoji_tags = build_custom_emoji_tags(&content);
    mention_tags.extend(custom_emoji_tags);
    let mut seen_pubkeys = std::collections::HashSet::new();
    if let Some(ref notify) = notify_pubkey {
        seen_pubkeys.insert(notify.to_hex());
    }
    mention_tags.retain(|tag| {
        if tag.kind() == nostr::TagKind::p() {
            if let Some(pk) = tag.content() {
                return seen_pubkeys.insert(pk.to_string());
            }
        }
        true
    });
    let relay_hint = extract_relay_hint(original_event);
    let e_tag = Tag::from_standardized_without_cell(TagStandard::Event {
        event_id: original_event_id,
        relay_url: relay_hint,
        marker: None,
        public_key: None,
        uppercase: false,
    });
    let mut builder = nostr::EventBuilder::new(Kind::Custom(KIND_NOTE_EDIT), &content)
        .tag(e_tag)
        .tag(nostr::Tag::alt("Content Change Event"))
        .tags(mention_tags);
    if let Some(ref sum) = summary {
        builder = builder.tag(nostr::Tag::custom(nostr::TagKind::Summary, [sum.as_str()]));
    }
    if let Some(ref notify) = notify_pubkey {
        builder = builder.tag(nostr::Tag::public_key(*notify));
    }
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign edit: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event.clone(),
        crate::stores::publish_queue::types::QueueEventType::Edit,
        None,
        std::collections::HashMap::new(),
    ).await;
    let publish = PublishResult::queued(queue_id, event_id);
    log::info!("Edit queued: {}", publish.event_id);
    Ok(EditPublishResult { event, publish })
}
