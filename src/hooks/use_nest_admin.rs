pub async fn publish_admin_command(
    room_coordinate: &str,
    target_pubkey: &str,
    action: &str,
) -> Result<(), String> {
    let _pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let tags = crate::utils::nips::nip53::build_admin_command_tags(
        room_coordinate,
        target_pubkey,
        action,
    );
    let builder =
        nostr_sdk::EventBuilder::new(nostr_sdk::Kind::Custom(4312), action).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("nest-admin".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(())
}
