use dioxus::prelude::*;
use std::collections::HashMap;

const SUCCESS_COOLDOWN_MS: u64 = 400;
const FAILURE_COOLDOWN_MS: u64 = 2000;

pub fn use_item_cooldowns() -> Signal<HashMap<String, u64>> {
    use_signal(HashMap::new)
}

pub fn is_on_cooldown(cooldowns: &HashMap<String, u64>, item_id: &str) -> bool {
    if let Some(&until) = cooldowns.get(item_id) {
        let now = crate::platform::timestamp::now_millis();
        now < until
    } else {
        false
    }
}

pub fn apply_cooldown_success(cooldowns: &mut HashMap<String, u64>, item_id: &str) {
    let now = crate::platform::timestamp::now_millis();
    cooldowns.insert(item_id.to_string(), now + SUCCESS_COOLDOWN_MS);
}

pub fn apply_cooldown_failure(cooldowns: &mut HashMap<String, u64>, item_id: &str) {
    let now = crate::platform::timestamp::now_millis();
    cooldowns.insert(item_id.to_string(), now + FAILURE_COOLDOWN_MS);
}

#[allow(dead_code)]
pub fn cooldown_remaining(cooldowns: &HashMap<String, u64>, item_id: &str) -> u64 {
    if let Some(&until) = cooldowns.get(item_id) {
        let now = crate::platform::timestamp::now_millis();
        until.saturating_sub(now)
    } else {
        0
    }
}
