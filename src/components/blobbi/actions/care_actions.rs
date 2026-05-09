use crate::components::blobbi::actions::action_types::BlobbiActionType;
use crate::components::blobbi::core::builders::{build_interaction_event, publish_blobbi_state};
use crate::components::blobbi::core::decay::apply_decay;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::utils::nip_bb::constants::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

pub fn apply_action_to_blobbi(blobbi: &BlobbiCompanion, action: BlobbiActionType) -> BlobbiCompanion {
    let now = nostr_sdk::Timestamp::now().as_secs();
    let mut updated = apply_decay(blobbi, now);
    let is_egg = updated.is_egg();

    let happiness_before = updated.stats.happiness;

    for (stat, delta) in action.stat_changes() {
        let current = updated.stat_value(stat);
        let new_val = (current + delta).round().clamp(STAT_MIN, STAT_MAX);
        updated.set_stat_value(stat, new_val);
    }

    if is_egg {
        updated.stats.energy = 100.0;
        updated.stats.hunger = 100.0;
    }

    updated.last_interaction = Some(now);
    updated.last_decay_at = Some(now);

    let happiness_changed = updated.stats.happiness != happiness_before;
    let grant_xp = match action {
        BlobbiActionType::Sing | BlobbiActionType::PlayMusic => happiness_changed,
        _ => true,
    };
    if grant_xp {
        updated.experience = updated.experience.saturating_add(action.xp_value());
    }
    updated.source = Some("user".to_string());

    match action {
        BlobbiActionType::Feed => updated.last_meal = Some(now),
        BlobbiActionType::Clean => updated.last_clean = Some(now),
        BlobbiActionType::Warm => updated.last_warm = Some(now),
        BlobbiActionType::Sing => updated.last_sing = Some(now),
        BlobbiActionType::Talk => updated.last_talk = Some(now),
        BlobbiActionType::Medicine => updated.last_medicine = Some(now),
        BlobbiActionType::Check => updated.last_check = Some(now),
        _ => {}
    }

    if updated.stats.average() > 60.0 {
        updated.personality.mood = "happy".to_string();
    } else if updated.stats.average() > 30.0 {
        updated.personality.mood = "neutral".to_string();
    } else {
        updated.personality.mood = "sad".to_string();
    }

    updated
}

#[allow(dead_code)]
pub fn apply_item_action(blobbi: &BlobbiCompanion, stat_changes: &[(&str, f64)], xp: u64) -> BlobbiCompanion {
    let now = nostr_sdk::Timestamp::now().as_secs();
    let mut updated = apply_decay(blobbi, now);

    for (stat, delta) in stat_changes {
        let current = updated.stat_value(stat);
        let new_val = (current + delta).round().clamp(STAT_MIN, STAT_MAX);
        updated.set_stat_value(stat, new_val);
    }

    updated.last_interaction = Some(now);
    updated.last_decay_at = Some(now);
    updated.experience = updated.experience.saturating_add(xp);
    updated.source = Some("user".to_string());

    if updated.stats.average() > 60.0 {
        updated.personality.mood = "happy".to_string();
    } else if updated.stats.average() > 30.0 {
        updated.personality.mood = "neutral".to_string();
    } else {
        updated.personality.mood = "sad".to_string();
    }

    updated
}

pub async fn execute_blobbi_action(blobbi: &BlobbiCompanion, action: BlobbiActionType) -> Result<BlobbiCompanion, String> {
    let mut blobbi = blobbi.clone();
    crate::components::blobbi::core::migration::ensure_canonical_before_action(&mut blobbi);

    let mut updated = apply_action_to_blobbi(&blobbi, action);

    super::hatch_tasks::update_task_progress(&mut updated, action.as_str());

    crate::components::blobbi::core::streak::record_care_action(&mut updated, action.as_str());

    super::mission_tracker::track_mission_progress(action);

    crate::components::blobbi::visual::status_reaction::trigger_action_emotion(action);

    publish_blobbi_state(&updated).await?;

    let stat_changes: Vec<String> = action
        .stat_changes()
        .iter()
        .map(|(stat, delta)| {
            if *delta >= 0.0 {
                format!("{}:+{:.0}", stat, delta)
            } else {
                format!("{}:{:.0}", stat, delta)
            }
        })
        .collect();

    let interaction_event = build_interaction_event(
        &blobbi.d,
        action.as_str(),
        action.category(),
        &stat_changes,
        None,
        action.xp_value(),
    );

    let event = crate::stores::publish_queue::signing::sign_event_builder(interaction_event)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;

    let xp_earned = updated.experience.saturating_sub(blobbi.experience);

    let toast = consume_toast();
    if xp_earned > 0 {
        toast.success(
            format!("{} performed!", action.label()),
            ToastOptions::new()
                .description(format!("+{} XP", xp_earned))
                .duration(Duration::from_secs(2)),
        );
    } else {
        toast.success(
            format!("{} performed!", action.label()),
            ToastOptions::new()
                .duration(Duration::from_secs(2)),
        );
    }

    Ok(updated)
}

#[allow(dead_code)]
pub async fn execute_item_action(
    blobbi: &BlobbiCompanion,
    item_id: &str,
    stat_changes: &[(&str, f64)],
    xp: u64,
) -> Result<BlobbiCompanion, String> {
    let mut updated = apply_item_action(blobbi, stat_changes, xp);

    super::hatch_tasks::update_task_progress(&mut updated, "use_item");

    publish_blobbi_state(&updated).await?;

    let stat_change_strs: Vec<String> = stat_changes
        .iter()
        .map(|(stat, delta)| {
            if *delta >= 0.0 {
                format!("{}:+{:.0}", stat, delta)
            } else {
                format!("{}:{:.0}", stat, delta)
            }
        })
        .collect();

    let interaction_event = build_interaction_event(
        &blobbi.d,
        "use_item",
        "inventory",
        &stat_change_strs,
        Some(item_id),
        xp,
    );

    let event = crate::stores::publish_queue::signing::sign_event_builder(interaction_event)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;

    Ok(updated)
}
