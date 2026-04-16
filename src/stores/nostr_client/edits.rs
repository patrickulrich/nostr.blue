use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use crate::utils::custom_emoji::build_custom_emoji_tags;
use crate::utils::mention_extractor::{create_mention_tags, extract_mentioned_pubkeys};
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

pub const KIND_NOTE_EDIT: u16 = 1010;

pub struct EditPublishResult {
    pub event: Event,
    pub publish: PublishResult,
}

pub async fn publish_edit(
    original_event_id: String,
    content: String,
    summary: Option<String>,
) -> std::result::Result<EditPublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    log::info!("Publishing edit for note {}", &original_event_id[..16.min(original_event_id.len())]);
    let event_id = EventId::from_hex(&original_event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let mentioned_pubkeys = extract_mentioned_pubkeys(&content);
    let mut mention_tags = create_mention_tags(&mentioned_pubkeys);
    let custom_emoji_tags = build_custom_emoji_tags(&content);
    mention_tags.extend(custom_emoji_tags);
    let mut seen_pubkeys = std::collections::HashSet::new();
    mention_tags.retain(|tag| {
        if tag.kind() == nostr::TagKind::p() {
            if let Some(pk) = tag.content() {
                return seen_pubkeys.insert(pk.to_string());
            }
        }
        true
    });
    let mut builder = nostr::EventBuilder::new(Kind::Custom(KIND_NOTE_EDIT), &content)
        .tag(nostr::Tag::event(event_id))
        .tag(nostr::Tag::alt("Content Change Event"))
        .tags(mention_tags);
    if let Some(ref sum) = summary {
        builder = builder.tag(nostr::Tag::custom(nostr::TagKind::Summary, [sum.as_str()]));
    }
    let builder = crate::utils::nips::nip89::tag_event_builder(builder);
    let event = client
        .sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign edit: {}", e))?;
    let output = client
        .send_event(&event)
        .await
        .map_err(|e| format!("Failed to publish edit: {}", e))?;
    let result = PublishResult::from_output(output);
    log::info!(
        "Edit published: {} ({}/{} relays)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );
    Ok(EditPublishResult { event, publish: result })
}
