use nostr_sdk::{Event, EventId, PublicKey};
use rschess::Color;

use super::game_state::GameState;
use super::jester::{JesterContent, JESTER_CONTENT_KIND_START, JESTER_CONTENT_KIND_MOVE};
use super::types::ViewerRole;
use crate::utils::nips::chess::JESTER_START_POSITION_HASH;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReconstructedGameState {
    pub game_id: EventId,
    pub start_event_id: EventId,
    pub white_pubkey: PublicKey,
    pub black_pubkey: Option<PublicKey>,
    pub viewer_role: ViewerRole,
    pub game_state: GameState,
    pub move_history: Vec<String>,
    pub result: Option<String>,
    pub termination: Option<String>,
    pub is_game_over: bool,
}

pub fn find_best_move_event(events: &[Event]) -> Option<&Event> {
    let mut best: Option<&Event> = None;
    let mut best_len: usize = 0;
    let mut best_created_at: u64 = u64::MAX;

    for event in events {
        let content = match JesterContent::parse(&event.content) {
            Some(c) => c,
            None => continue,
        };
        if content.kind != JESTER_CONTENT_KIND_MOVE {
            continue;
        }
        if content.history.len() > best_len
            || (content.history.len() == best_len && event.created_at.as_secs() < best_created_at)
        {
            best_len = content.history.len();
            best_created_at = event.created_at.as_secs();
            best = Some(event);
        }
    }
    best
}

pub fn find_start_event(events: &[Event]) -> Option<&Event> {
    events.iter().find(|event| {
        JesterContent::parse(&event.content)
            .map(|c| c.kind == JESTER_CONTENT_KIND_START && c.history.is_empty())
            .unwrap_or(false)
    })
}

pub fn get_ptag_opponent(event: &Event) -> Option<PublicKey> {
    event
        .tags
        .iter()
        .find(|t| t.kind() == nostr_sdk::TagKind::p())
        .and_then(|t| t.content())
        .and_then(|pk| PublicKey::from_hex(pk).ok())
        .filter(|pk| *pk != event.pubkey)
}

pub fn reconstruct(
    events: &[Event],
    viewer_pubkey: &PublicKey,
) -> Result<ReconstructedGameState, String> {
    let start_event = find_start_event(events).ok_or("No start event found")?;
    let best_move_event = find_best_move_event(events);

    let start_content =
        JesterContent::parse(&start_event.content).ok_or("Invalid start content")?;

    let challenger_pubkey = start_event.pubkey;
    let challenger_color = match start_content.player_color.as_deref() {
        Some("black") => Color::Black,
        _ => Color::White,
    };

    let opponent_from_move = best_move_event
        .map(|e| e.pubkey)
        .filter(|p| *p != challenger_pubkey);
    let opponent_from_ptag = get_ptag_opponent(start_event);
    let opponent_pubkey = opponent_from_move.or(opponent_from_ptag);

    let (white_pubkey, black_pubkey) = if challenger_color == Color::White {
        (challenger_pubkey, opponent_pubkey)
    } else {
        let white = opponent_pubkey.unwrap_or(challenger_pubkey);
        (white, Some(challenger_pubkey))
    };

    let viewer_role = if viewer_pubkey == &white_pubkey {
        ViewerRole::WhitePlayer
    } else if black_pubkey.as_ref() == Some(viewer_pubkey) {
        ViewerRole::BlackPlayer
    } else {
        ViewerRole::Spectator
    };

    let mut game_state = GameState::new_game();
    let mut move_history: Vec<String> = vec![];
    let mut result: Option<String> = None;
    let mut termination: Option<String> = None;

    if let Some(move_ev) = best_move_event {
        let content = JesterContent::parse(&move_ev.content).unwrap();
        let mut desynced = false;
        for san in &content.history {
            if game_state.make_move_san(san).is_err() {
                log::warn!("SAN replay desync at move '{}', attempting FEN recovery", san);
                desynced = true;
                break;
            }
            move_history.push(san.clone());
        }
        if desynced {
            if let Some(ref fen) = content.fen {
                match GameState::from_fen(Some(fen)) {
                    Ok(recovered) => {
                        log::info!("Recovered from FEN: played {}/{} moves", move_history.len(), content.history.len());
                        game_state = recovered;
                        move_history = content.history.clone();
                    }
                    Err(e) => {
                        log::warn!("FEN recovery failed ({}), partial state with {} moves", e, move_history.len());
                    }
                }
            }
        }
        result = content.result;
        termination = content.termination;
    }

    let is_game_over = game_state.is_game_over() || result.is_some();

    Ok(ReconstructedGameState {
        game_id: start_event.id,
        start_event_id: start_event.id,
        white_pubkey,
        black_pubkey,
        viewer_role,
        game_state,
        move_history,
        result,
        termination,
        is_game_over,
    })
}

#[allow(dead_code)]
pub fn is_jester_start_event(event: &Event) -> bool {
    if event.kind.as_u16() != 30 {
        return false;
    }
    let content = match JesterContent::parse(&event.content) {
        Some(c) => c,
        None => return false,
    };
    content.kind == JESTER_CONTENT_KIND_START
        && content.history.is_empty()
        && event
            .tags
            .iter()
            .any(|t| t.kind() == nostr_sdk::TagKind::e()
                && t.content() == Some(JESTER_START_POSITION_HASH))
}

#[allow(dead_code)]
pub fn is_jester_move_event(event: &Event) -> bool {
    if event.kind.as_u16() != 30 {
        return false;
    }
    let content = match JesterContent::parse(&event.content) {
        Some(c) => c,
        None => return false,
    };
    content.kind == JESTER_CONTENT_KIND_MOVE
}

#[allow(dead_code)]
pub fn get_start_event_id(event: &Event) -> Option<EventId> {
    event
        .tags
        .iter()
        .find(|t| t.kind() == nostr_sdk::TagKind::e())
        .and_then(|t| t.content())
        .and_then(|id| EventId::from_hex(id).ok())
}

#[allow(dead_code)]
pub fn get_opponent_pubkey(event: &Event) -> Option<PublicKey> {
    event
        .tags
        .iter()
        .find(|t| t.kind() == nostr_sdk::TagKind::p())
        .and_then(|t| t.content())
        .and_then(|pk| PublicKey::from_hex(pk).ok())
}
