use nostr_sdk::{EventId, PublicKey};

#[derive(Debug, Clone, PartialEq)]
pub struct ChessChallenge {
    pub event_id: EventId,
    pub game_id: EventId,
    pub challenger_pubkey: PublicKey,
    pub opponent_pubkey: Option<PublicKey>,
    pub challenger_color: rschess::Color,
    pub created_at: u64,
}

impl ChessChallenge {
    pub fn is_open(&self) -> bool {
        self.opponent_pubkey.is_none()
    }

    pub fn is_directed_at(&self, pubkey: &PublicKey) -> bool {
        self.opponent_pubkey.as_ref() == Some(pubkey)
    }

    pub fn is_from(&self, pubkey: &PublicKey) -> bool {
        &self.challenger_pubkey == pubkey
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicGame {
    pub game_id: EventId,
    pub white_pubkey: PublicKey,
    pub black_pubkey: Option<PublicKey>,
    pub move_count: usize,
    pub last_move_at: u64,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletedGame {
    pub game_id: EventId,
    pub white_pubkey: PublicKey,
    pub black_pubkey: PublicKey,
    pub result: String,
    pub termination: Option<String>,
    pub move_count: usize,
    pub completed_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewerRole {
    WhitePlayer,
    BlackPlayer,
    Spectator,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ChessGameStatus {
    WaitingForOpponent,
    Active,
    Completed(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChessColor {
    White,
    Black,
}

impl From<rschess::Color> for ChessColor {
    fn from(c: rschess::Color) -> Self {
        match c {
            rschess::Color::White => ChessColor::White,
            rschess::Color::Black => ChessColor::Black,
        }
    }
}

impl From<ChessColor> for rschess::Color {
    fn from(c: ChessColor) -> Self {
        match c {
            ChessColor::White => rschess::Color::White,
            ChessColor::Black => rschess::Color::Black,
        }
    }
}
