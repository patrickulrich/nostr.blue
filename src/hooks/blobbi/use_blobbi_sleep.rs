use crate::components::blobbi::core::builders::{build_interaction_event, publish_blobbi_state};
use crate::components::blobbi::core::types::{BlobbiCompanion, BlobbiState};
use crate::stores::blobbi_store;
use crate::stores::nostr_client;
use crate::utils::nip_bb::constants::*;

pub async fn put_to_sleep(blobbi: &BlobbiCompanion) -> Result<BlobbiCompanion, String> {
    if blobbi.is_sleeping() {
        return Err("Already sleeping".to_string());
    }

    let now = nostr_sdk::Timestamp::now().as_secs();
    let mut updated = blobbi.clone();
    updated.is_sleeping = true;
    updated.state = BlobbiState::Sleeping;
    updated.sleep_started_at = Some(now);
    updated.last_sleep_update = Some(now);
    updated.last_interaction = Some(now);
    updated.source = Some("user".to_string());

    publish_blobbi_state(&updated).await?;

    let interaction = build_interaction_event(
        &blobbi.d,
        "rest",
        "care",
        &["energy:+0".to_string()],
        None,
        4,
    );
    let _client = nostr_client::get_client().ok_or("Client not initialized")?;
    let event = crate::stores::publish_queue::signing::sign_event_builder(interaction)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;

    blobbi_store::update_blobbi_in_collection(&updated);
    Ok(updated)
}

pub async fn wake_up(blobbi: &BlobbiCompanion) -> Result<BlobbiCompanion, String> {
    if !blobbi.is_sleeping() {
        return Err("Not sleeping".to_string());
    }

    let now = nostr_sdk::Timestamp::now().as_secs();
    let happiness_delta = if blobbi.stats.energy >= 50.0 { 5.0 } else { -5.0 };

    let mut updated = blobbi.clone();
    updated.is_sleeping = false;
    updated.state = BlobbiState::Active;
    updated.sleep_started_at = None;
    updated.last_sleep_update = None;
    updated.last_interaction = Some(now);
    updated.stats.happiness = (updated.stats.happiness + happiness_delta).round().clamp(STAT_MIN, STAT_MAX);
    updated.experience = updated.experience.saturating_add(2);
    updated.care_streak = updated.care_streak.saturating_add(1);
    updated.source = Some("user".to_string());

    publish_blobbi_state(&updated).await?;

    let delta_str = if happiness_delta >= 0.0 {
        format!("happiness:+{:.0}", happiness_delta)
    } else {
        format!("happiness:{:.0}", happiness_delta)
    };
    let interaction = build_interaction_event(
        &blobbi.d,
        "wake",
        "recovery",
        &[delta_str],
        None,
        2,
    );
    let _client = nostr_client::get_client().ok_or("Client not initialized")?;
    let event = crate::stores::publish_queue::signing::sign_event_builder(interaction)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;

    blobbi_store::update_blobbi_in_collection(&updated);
    Ok(updated)
}
