use nostr::event::EventId;

#[allow(dead_code)]
pub fn ensure_publish_accepted(
    output: &nostr_sdk::client::Output<EventId>,
    action: &str,
) -> Result<(), String> {
    if output.success.is_empty() {
        Err(format!("{action}: no relays accepted the event"))
    } else {
        Ok(())
    }
}
