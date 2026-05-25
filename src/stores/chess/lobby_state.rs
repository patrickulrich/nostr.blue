use dioxus::prelude::*;
use nostr_sdk::{EventId, PublicKey};

use super::types::{ChessChallenge, PublicGame, CompletedGame, ViewerRole};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChessLobbyState {
    pub challenges: Vec<ChessChallenge>,
    pub active_games: Vec<ActiveGame>,
    pub public_games: Vec<PublicGame>,
    pub spectating_games: Vec<ActiveGame>,
    pub completed_games: Vec<CompletedGame>,
    pub selected_game_id: Option<EventId>,
    pub error: Option<String>,
    pub is_loading: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveGame {
    pub game_id: EventId,
    pub start_event_id: EventId,
    pub white_pubkey: PublicKey,
    pub black_pubkey: Option<PublicKey>,
    pub viewer_role: ViewerRole,
    pub move_count: usize,
    pub last_move_san: Option<String>,
    pub is_my_turn: bool,
    pub last_move_at: u64,
}

pub static CHESS_LOBBY: GlobalSignal<ChessLobbyState> = Signal::global(ChessLobbyState::default);

#[allow(dead_code)]
impl ChessLobbyState {
    pub fn incoming_challenges(&self, my_pubkey: &PublicKey) -> Vec<&ChessChallenge> {
        self.challenges
            .iter()
            .filter(|c| c.is_directed_at(my_pubkey))
            .collect()
    }

    pub fn outgoing_challenges(&self, my_pubkey: &PublicKey) -> Vec<&ChessChallenge> {
        self.challenges
            .iter()
            .filter(|c| c.is_from(my_pubkey))
            .collect()
    }

    pub fn open_challenges(&self, my_pubkey: &PublicKey) -> Vec<&ChessChallenge> {
        self.challenges
            .iter()
            .filter(|c| c.is_open() && !c.is_from(my_pubkey) && !c.is_directed_at(my_pubkey))
            .collect()
    }

    pub fn badge_count(&self, my_pubkey: &PublicKey) -> usize {
        self.incoming_challenges(my_pubkey).len()
            + self
                .active_games
                .iter()
                .filter(|g| g.is_my_turn)
                .count()
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }
}
