use crate::utils::nips::chess::{
    CHESS_RELAYS, CHALLENGE_WINDOW_SECS, FETCH_TIMEOUT_SECS, GAME_EVENT_WINDOW_SECS,
};

pub fn chess_relay_urls() -> Vec<String> {
    CHESS_RELAYS.iter().map(|s| s.to_string()).collect()
}

#[allow(dead_code)]
pub fn fetch_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(FETCH_TIMEOUT_SECS)
}

#[allow(dead_code)]
pub fn challenge_window_secs() -> u64 {
    CHALLENGE_WINDOW_SECS
}

#[allow(dead_code)]
pub fn game_event_window_secs() -> u64 {
    GAME_EVENT_WINDOW_SECS
}
