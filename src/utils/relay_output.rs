use nostr_sdk::EventId;

#[allow(dead_code)]
pub fn ensure_publish_accepted(
    output: &nostr_relay_pool::Output<EventId>,
    action: &str,
) -> Result<(), String> {
    if output.success.is_empty() {
        Err(format!("{action}: no relays accepted the event"))
    } else {
        Ok(())
    }
}
