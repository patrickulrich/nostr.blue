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

    pub fn mark_challenge_accepted(&mut self, game_id: &EventId, my_pubkey: &PublicKey) {
        let idx = match self.challenges.iter().position(|c| &c.game_id == game_id) {
            Some(i) => i,
            None => return,
        };
        let challenge = self.challenges.remove(idx);
        let (white_pk, black_pk, viewer_role) = match challenge.challenger_color {
            rschess::Color::White => (
                challenge.challenger_pubkey,
                Some(*my_pubkey),
                ViewerRole::BlackPlayer,
            ),
            rschess::Color::Black => (
                *my_pubkey,
                Some(challenge.challenger_pubkey),
                ViewerRole::WhitePlayer,
            ),
        };
        let start_event_id = challenge.event_id;
        if !self.active_games.iter().any(|g| g.game_id == *game_id) {
            self.active_games.push(ActiveGame {
                game_id: *game_id,
                start_event_id,
                white_pubkey: white_pk,
                black_pubkey: black_pk,
                viewer_role,
                move_count: 0,
                last_move_san: None,
                is_my_turn: challenge.challenger_color == rschess::Color::White,
                last_move_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });
        }
    }
}
