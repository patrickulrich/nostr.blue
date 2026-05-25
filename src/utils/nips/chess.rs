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
