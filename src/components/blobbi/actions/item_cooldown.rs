use dioxus::prelude::*;
use std::collections::HashMap;

const DEFAULT_COOLDOWN_SECS: u64 = 30;

#[allow(dead_code)]
pub fn use_item_cooldowns() -> Signal<HashMap<String, u64>> {
    use_signal(HashMap::new)
}

#[allow(dead_code)]
pub fn is_on_cooldown(cooldowns: &HashMap<String, u64>, item_id: &str) -> bool {
    if let Some(&until) = cooldowns.get(item_id) {
        let now = nostr_sdk::Timestamp::now().as_secs();
        now < until
    } else {
        false
    }
}

#[allow(dead_code)]
pub fn apply_cooldown(cooldowns: &mut HashMap<String, u64>, item_id: &str) {
    let now = nostr_sdk::Timestamp::now().as_secs();
    cooldowns.insert(item_id.to_string(), now + DEFAULT_COOLDOWN_SECS);
}

#[allow(dead_code)]
pub fn cooldown_remaining(cooldowns: &HashMap<String, u64>, item_id: &str) -> u64 {
    if let Some(&until) = cooldowns.get(item_id) {
        let now = nostr_sdk::Timestamp::now().as_secs();
        until.saturating_sub(now)
    } else {
        0
    }
}
