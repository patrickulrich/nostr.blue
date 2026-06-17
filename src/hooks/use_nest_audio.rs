use crate::services::nests_audio::reconnect::{self, NestConfig};
use crate::services::nests_audio::ConnectionState;

/// Join a nest room's audio session.
///
/// `publish` controls whether the JWT is minted with publish scope (speaker)
/// or subscribe-only scope (listener). Speakers call this with `publish=true`
/// after the host flips their role on the 30312; listeners call it with
/// `publish=false`. The JWT's `put` claim is populated only when `publish=true`
/// (verified at `moq-auth/src/index.ts:188-197`).
pub async fn join_room(
    publisher_id: &str,
    auth_url: &str,
    relay_url: &str,
    namespace: &str,
    my_pubkey: &str,
    publish: bool,
) -> Result<(), String> {
    let jwt = crate::services::nests_audio::authenticate_with_nest(auth_url, namespace, publish)
        .await?;
    crate::services::nests_audio::js_init(publisher_id).await?;
    crate::services::nests_audio::js_connect(
        publisher_id,
        auth_url,
        relay_url,
        namespace,
        &jwt,
        my_pubkey,
    )
    .await?;
    // Track for reconnect support (Phase 2.1).
    reconnect::track_join(
        publisher_id,
        NestConfig {
            auth_url: auth_url.to_string(),
            relay_url: relay_url.to_string(),
            namespace: namespace.to_string(),
            my_pubkey: my_pubkey.to_string(),
        },
        publish,
    );
    Ok(())
}

pub async fn join_room_with_retry(
    publisher_id: &str,
    auth_url: &str,
    relay_url: &str,
    namespace: &str,
    my_pubkey: &str,
    publish: bool,
    max_retries: u32,
) -> Result<(), String> {
    let mut last_error = String::new();
    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay_ms: u32 = (1000u64 * 2u64.pow(attempt)).min(30_000) as u32;
            crate::platform::timer::sleep_ms(delay_ms).await;
        }
        match join_room(publisher_id, auth_url, relay_url, namespace, my_pubkey, publish).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_error = e;
                let _ = crate::services::nests_audio::js_disconnect(publisher_id).await;
            }
        }
    }
    Err(last_error)
}

pub async fn subscribe_to_participant(publisher_id: &str, pubkey: &str) -> Result<(), String> {
    crate::services::nests_audio::js_subscribe_audio(publisher_id, pubkey).await?;
    reconnect::track_subscribe(publisher_id, pubkey);
    Ok(())
}

pub async fn unsubscribe_from_participant(publisher_id: &str, pubkey: &str) -> Result<(), String> {
    crate::services::nests_audio::js_unsubscribe_audio(publisher_id, pubkey).await?;
    reconnect::track_unsubscribe(publisher_id, pubkey);
    Ok(())
}

/// Start publishing local microphone audio. Requires that `join_room` was
/// called with `publish=true` (otherwise the relay will reject writes per the
/// JWT's `put` claim — verified at `moq-auth/src/index.ts:188-197`).
pub async fn start_publishing(publisher_id: &str) -> Result<(), String> {
    crate::services::nests_audio::js_publish_audio(publisher_id).await
}

pub async fn mute(publisher_id: &str) -> Result<(), String> {
    crate::services::nests_audio::js_mute(publisher_id).await
}

pub async fn unmute(publisher_id: &str) -> Result<(), String> {
    crate::services::nests_audio::js_unmute(publisher_id).await
}

pub async fn leave_room(publisher_id: &str) -> Result<(), String> {
    let result = crate::services::nests_audio::js_disconnect(publisher_id).await;
    reconnect::clear(publisher_id);
    result
}

#[allow(dead_code)]
pub async fn get_connection_state(publisher_id: &str) -> ConnectionState {
    crate::services::nests_audio::js_get_connection_state(publisher_id).await
}

#[allow(dead_code)]
pub async fn get_participant_tracks(publisher_id: &str) -> Vec<String> {
    crate::services::nests_audio::js_get_participant_tracks(publisher_id).await
}

/// Read the local mic peak level (0.0–1.0). Used by Phase 1.5's energy-gated
/// speaking ring (100ms poll). On desktop, reads from the encoding thread's
/// shared AtomicU32; on web, calls `moq-nest.js`'s `getMicLevel`.
pub async fn get_mic_level(publisher_id: &str) -> f32 {
    crate::services::nests_audio::js_get_mic_level(publisher_id).await
}

/// Phase 4.1: Poll the MoQ ANNOUNCE stream for real-time participant
/// discovery. Returns pubkeys of speakers currently announced on the relay.
/// Called every 3s from `nest_viewer.rs` to reconcile with Nostr presence.
pub async fn poll_announced_participants(publisher_id: &str) -> Vec<String> {
    crate::services::nests_audio::js_poll_announced_participants(publisher_id).await
}

/// Phase 3.2: Set per-speaker volume (0.0–1.0). Applies a GainNode value to
/// one speaker without affecting the rest ("local hush").
#[allow(dead_code)]
pub async fn set_local_hush(
    publisher_id: &str,
    participant_pubkey: &str,
    volume: f32,
) -> Result<(), String> {
    crate::services::nests_audio::js_set_volume(publisher_id, participant_pubkey, volume).await
}

/// Phase 3.7: Batch-poll all subscribed participant audio levels for remote
/// speaking detection. Returns `{pubkey: peak_level}` map. Called every 100ms
/// from the level poll task in `nest_viewer.rs`.
pub async fn get_all_participant_levels(
    publisher_id: &str,
) -> std::collections::HashMap<String, f32> {
    crate::services::nests_audio::js_get_all_participant_levels(publisher_id).await
}

pub async fn publish_presence(
    room_coordinate: &str,
    muted: bool,
    publishing: bool,
    hand_raised: bool,
    onstage: bool,
) -> Result<(), String> {
    let tags = crate::utils::nips::nip53::build_room_presence_tags(
        room_coordinate, hand_raised, muted, publishing, onstage,
    );
    let builder = nostr_sdk::EventBuilder::new(nostr_sdk::Kind::Custom(10312), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("nest-presence".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(())
}
