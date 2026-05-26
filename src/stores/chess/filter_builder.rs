use nostr_sdk::{Filter, Kind, PublicKey, EventId, Timestamp, SingleLetterTag, Alphabet};

use crate::utils::nips::chess::{
    KIND_JESTER, JESTER_START_POSITION_HASH, CHALLENGE_WINDOW_SECS, GAME_EVENT_WINDOW_SECS,
};

pub fn start_event_filter() -> Filter {
    let since = Timestamp::now() - CHALLENGE_WINDOW_SECS;
    Filter::new()
        .kind(Kind::Custom(KIND_JESTER))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::E), JESTER_START_POSITION_HASH)
        .since(since)
        .limit(100)
}

pub fn personal_filters(pubkey: &PublicKey) -> Vec<Filter> {
    let since = Timestamp::now() - CHALLENGE_WINDOW_SECS;
    let p_tag_filter = Filter::new()
        .kind(Kind::Custom(KIND_JESTER))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::P), pubkey.to_hex())
        .since(since)
        .limit(100);
    let private_ref = crate::utils::nips::chess::jester_private_start_ref(&pubkey.to_hex());
    let private_e_tag_filter = Filter::new()
        .kind(Kind::Custom(KIND_JESTER))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::E), private_ref)
        .since(since)
        .limit(100);
    vec![p_tag_filter, private_e_tag_filter]
}

pub fn game_events_filter(game_id: &EventId) -> Vec<Filter> {
    vec![
        Filter::new()
            .id(*game_id)
            .kind(Kind::Custom(KIND_JESTER))
            .limit(1),
        Filter::new()
            .kind(Kind::Custom(KIND_JESTER))
            .custom_tag(SingleLetterTag::lowercase(Alphabet::E), game_id.to_hex())
            .limit(500),
    ]
}

pub fn game_moves_subscription_filter(game_id: &EventId) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_JESTER))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::E), game_id.to_hex())
        .limit(0)
        .since(Timestamp::now())
}

pub fn active_game_filters(game_ids: &[EventId]) -> Vec<Filter> {
    if game_ids.is_empty() {
        return vec![];
    }
    let ids: Vec<EventId> = game_ids.to_vec();
    let id_strs: Vec<String> = ids.iter().map(|id| id.to_hex()).collect();
    vec![
        Filter::new()
            .ids(ids)
            .kind(Kind::Custom(KIND_JESTER))
            .limit(500),
        Filter::new()
            .kind(Kind::Custom(KIND_JESTER))
            .custom_tags(SingleLetterTag::lowercase(Alphabet::E), id_strs)
            .limit(500),
    ]
}

#[allow(dead_code)]
pub fn recent_games_filter() -> Filter {
    let since = Timestamp::now() - GAME_EVENT_WINDOW_SECS;
    Filter::new()
        .kind(Kind::Custom(KIND_JESTER))
        .since(since)
        .limit(100)
}

pub fn pgn_games_filter() -> Filter {
    Filter::new()
        .kind(Kind::Custom(64))
        .limit(50)
}
