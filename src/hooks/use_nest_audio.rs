use crate::services::nests_audio::ConnectionState;

pub async fn join_room(
    publisher_id: &str,
    auth_url: &str,
    relay_url: &str,
    namespace: &str,
    my_pubkey: &str,
) -> Result<(), String> {
    let jwt =
        crate::services::nests_audio::authenticate_with_nest(auth_url, namespace, false).await?;
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
    Ok(())
}

pub async fn join_room_with_retry(
    publisher_id: &str,
    auth_url: &str,
    relay_url: &str,
    namespace: &str,
    my_pubkey: &str,
    max_retries: u32,
) -> Result<(), String> {
    let mut last_error = String::new();
    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay_ms: u32 = (1000u64 * 2u64.pow(attempt)).min(30_000) as u32;
            crate::platform::timer::sleep_ms(delay_ms).await;
        }
        match join_room(publisher_id, auth_url, relay_url, namespace, my_pubkey).await {
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
    crate::services::nests_audio::js_subscribe_audio(publisher_id, pubkey).await
}

pub async fn unsubscribe_from_participant(publisher_id: &str, pubkey: &str) -> Result<(), String> {
    crate::services::nests_audio::js_unsubscribe_audio(publisher_id, pubkey).await
}

#[allow(dead_code)]
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
    crate::services::nests_audio::js_disconnect(publisher_id).await
}

#[allow(dead_code)]
pub async fn get_connection_state(publisher_id: &str) -> ConnectionState {
    crate::services::nests_audio::js_get_connection_state(publisher_id).await
}

#[allow(dead_code)]
pub async fn get_participant_tracks(publisher_id: &str) -> Vec<String> {
    crate::services::nests_audio::js_get_participant_tracks(publisher_id).await
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
