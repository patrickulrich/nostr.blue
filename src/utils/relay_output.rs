use nostr_sdk::EventId;

pub fn ensure_publish_accepted(
    output: &nostr_relay_pool::Output<EventId>,
    action: &str,
) -> Result<(), String> {
    if output.success.is_empty() {
        let details: Vec<String> = output
            .failed
            .iter()
            .map(|(relay, reason)| format!("{}: {}", relay, reason))
            .collect();
        Err(format!(
            "{action}: no relays accepted the event; failures: {}",
            details.join(", ")
        ))
    } else {
        Ok(())
    }
}
