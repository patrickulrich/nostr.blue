//! Workouts (kind 1301, NIP-101e)
//!
//! Publishes workout records in the canonical RUNSTR wire format,
//! interoperable with the POWR / NIP-101e strength dialect on read.
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use crate::utils::nips::nip101e::{self, WorkoutDraft};
use dioxus::prelude::ReadableExt;

/// Publish a workout record (kind 1301) with relay feedback.
/// NIP-101e (draft), interoperable with the RUNSTR dialect.
pub async fn publish_workout_tracked(draft: WorkoutDraft) -> Result<PublishResult, String> {
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    if draft.duration_seconds == 0 {
        return Err("Workout duration must be greater than zero".to_string());
    }
    log::info!(
        "Publishing workout: {} ({})",
        draft.title.as_deref().unwrap_or(draft.exercise.code()),
        draft.exercise.code()
    );
    let workout_id = uuid::Uuid::new_v4().to_string();
    let builder = nip101e::build_workout_event(&draft, workout_id);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign workout: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Workout,
        // None = the user's WRITE-flagged relay pool, matching shop/sidebar.
        None,
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Workout enqueued: {} (queue: {})", event_id, queue_id);
    Ok(PublishResult::queued(queue_id, event_id))
}

/// Publish a workout; returns the event id. See [publish_workout_tracked].
pub async fn publish_workout(draft: WorkoutDraft) -> Result<String, String> {
    publish_workout_tracked(draft)
        .await
        .map(|result| result.event_id)
}
