pub const KIND_JESTER: u16 = 30;
#[allow(dead_code)]
pub const KIND_CHESS_PGN: u16 = 64;

pub const JESTER_START_POSITION_HASH: &str =
    "b1791d7fc9ae3d38966568c257ffb3a02cbf8394cdb4805bc70f64fc3c0b6879";

#[allow(dead_code)]
pub const FEN_START_POSITION: &str =
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

pub const CHESS_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.primal.net",
    "wss://offchain.pub",
];

#[allow(dead_code)]
pub const FETCH_TIMEOUT_SECS: u64 = 10;
pub const CHALLENGE_WINDOW_SECS: u64 = 86_400;
#[allow(dead_code)]
pub const GAME_EVENT_WINDOW_SECS: u64 = 604_800;

pub const JESTER_ID_PREFIX: &str = "jester";

pub fn jester_private_start_ref(opponent_hex: &str) -> String {
    use sha2::{Sha256, Digest};
    let hash1 = Sha256::digest(opponent_hex.as_bytes());
    let hash1_hex = format!("{:x}", hash1);
    let combined = format!("{}{}", hash1_hex, JESTER_START_POSITION_HASH);
    let hash2 = Sha256::digest(combined.as_bytes());
    format!("{:x}", hash2)
}

#[allow(dead_code)]
pub fn game_id_to_jester_id(event_id_hex: &str) -> Option<String> {
    use bech32::{Bech32m, Hrp};
    let bytes = hex::decode(event_id_hex).ok()?;
    let hrp = Hrp::parse(JESTER_ID_PREFIX).ok()?;
    bech32::encode::<Bech32m>(hrp, &bytes).ok()
}

pub fn jester_id_to_game_id(jester_id: &str) -> Option<String> {
    let (hrp, data) = bech32::decode(jester_id).ok()?;
    if hrp.as_str() != JESTER_ID_PREFIX {
        return None;
    }
    Some(hex::encode(data))
}

pub fn parse_game_id(input: &str) -> Option<String> {
    if input.starts_with("jester1") {
        jester_id_to_game_id(input)
    } else {
        Some(input.to_string())
    }
}
