pub async fn publish_admin_command(
    room_coordinate: &str,
    target_pubkey: &str,
    action: &str,
) -> Result<(), String> {
    crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let tags = crate::utils::nips::nip53::build_admin_command_tags(
        room_coordinate,
        target_pubkey,
        action,
    );
    // EGG-07: the verb lives in the `["action", …]` tag; `content` is empty.
    // (Amethyst's parser reads the tag first and falls back to content, but
    // other clients may not — keep the wire shape canonical.)
    let builder =
        nostr::event::EventBuilder::new(nostr::event::Kind::Custom(4312), "").tags(tags);
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
