//! D5: per-(order, channel) chat unread-count tracking.
//!
//! Stores `last_read_at` timestamps in `platform::storage` so the user
//! can dismiss the chat panel and pick up where they left off across
//! app restarts. Used by the sidebar (Mostro entry badge) and the
//! trade list (per-row badge).
//!
//! Channels:
//! - "peer" — P2P trade chat between counterparties.
//! - "dispute" — dispute chat with the assigned solver.
//!
//! Mirrors Mobile's `chat_read_status_service.dart` /
//! `dispute_read_status_service.dart` patterns.

use crate::platform::storage;

const KEY_PREFIX: &str = "mostro/chat_read_state/";

#[allow(dead_code)]
const PEER: &str = "peer";
#[allow(dead_code)]
const DISPUTE: &str = "dispute";

fn storage_key(order_id: &str, channel: &str) -> String {
    format!("{KEY_PREFIX}{channel}/{order_id}")
}

/// Get the unix-secs timestamp of the last time the user opened the chat
/// panel for this `(order_id, channel)`. `None` means "never opened".
#[allow(dead_code)]
pub fn get_last_read_at(order_id: &str, channel: &str) -> Option<i64> {
    storage::get::<i64>(&storage_key(order_id, channel)).ok()
}

/// Mark the chat panel as read up to "now". Called when the chat panel
/// gains focus or receives a message while focused.
#[allow(dead_code)]
pub fn mark_read(order_id: &str, channel: &str) {
    let now = crate::platform::timestamp::now_secs() as i64;
    let _ = storage::set(&storage_key(order_id, channel), &now);
}

/// Count unread messages: messages in `messages` whose `timestamp` is
/// strictly greater than `last_read_at` (or all messages if never read).
/// Excludes messages sent by the local user (`is_me == true`).
#[allow(dead_code)]
pub fn unread_count(
    order_id: &str,
    channel: &str,
    messages: &[crate::components::mostro::trade_chat::ChatMsg],
) -> usize {
    let last_read_at = get_last_read_at(order_id, channel).unwrap_or(0);
    messages
        .iter()
        .filter(|m| !m.is_me && m.timestamp > last_read_at)
        .count()
}

/// Total unread count across all known trade + dispute chats.
///
/// Iterates `mostro_trades_v1` cache to enumerate orders, then reads
/// each order's chat messages from storage. Cheap enough for sidebar
/// rendering (typically <50 trades, <100 messages each).
#[allow(dead_code)]
pub fn total_unread_count() -> usize {
    let trades = crate::stores::mostro::trade_store::all_trades_for_daemon();
    let mut total = 0usize;
    for trade in &trades {
        let msgs = crate::components::mostro::trade_chat::load_chat_messages(&trade.order_id);
        total += unread_count(&trade.order_id, PEER, &msgs);
        // Dispute chat messages are stored per dispute_id, but the
        // trade record carries it. Skip if no dispute.
        if let Some(ref did) = trade.dispute_id {
            let dmsgs = load_dispute_chat_messages(did);
            total += unread_count(did, DISPUTE, &dmsgs);
        }
    }
    total
}

/// Load dispute chat messages by dispute id. Dispute chat storage uses
/// the same `mostro/dispute-chat/{id}` pattern as trade chat.
fn load_dispute_chat_messages(dispute_id: &str) -> Vec<crate::components::mostro::trade_chat::ChatMsg> {
    let key = format!("mostro/dispute-chat/{dispute_id}");
    crate::platform::storage::get::<String>(&key)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// Wipe all read state. Used on logout.
#[allow(dead_code)]
pub fn reset_all() {
    // platform::storage doesn't have a "delete by prefix" API, so we
    // iterate the trades list and delete each known key. Best-effort.
    let trades = crate::stores::mostro::trade_store::all_trades_for_daemon();
    for trade in &trades {
        let _ = storage::delete(&storage_key(&trade.order_id, PEER));
        if let Some(ref did) = trade.dispute_id {
            let _ = storage::delete(&storage_key(did, DISPUTE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::mostro::trade_chat::ChatMsg;

    fn msg(timestamp: i64, is_me: bool) -> ChatMsg {
        ChatMsg {
            content: "test".to_string(),
            sender_hex: "deadbeef".to_string(),
            is_me,
            timestamp,
            attachments: vec![],
        }
    }

    /// Pure count helper (doesn't touch storage). Tests the filtering
    /// logic without needing platform::storage.
    fn count_unread_since(
        last_read_at: i64,
        messages: &[ChatMsg],
    ) -> usize {
        messages.iter().filter(|m| !m.is_me && m.timestamp > last_read_at).count()
    }

    #[test]
    fn count_unread_excludes_is_me_messages() {
        let msgs = vec![
            msg(100, false),
            msg(200, true), // sent by me — don't count
            msg(300, false),
        ];
        assert_eq!(count_unread_since(0, &msgs), 2);
    }

    #[test]
    fn count_unread_respects_last_read_at() {
        let msgs = vec![
            msg(100, false), // before last_read — read
            msg(200, false), // after — unread
            msg(300, false), // after — unread
        ];
        assert_eq!(count_unread_since(150, &msgs), 2);
    }

    #[test]
    fn count_unread_handles_never_read() {
        let msgs = vec![
            msg(100, false),
            msg(200, false),
        ];
        // last_read_at = 0 means "never read" — all incoming messages unread.
        assert_eq!(count_unread_since(0, &msgs), 2);
    }

    #[test]
    fn count_unread_empty_messages() {
        let msgs: Vec<ChatMsg> = vec![];
        assert_eq!(count_unread_since(0, &msgs), 0);
    }

    #[test]
    fn storage_key_format() {
        assert_eq!(
            storage_key("order-123", PEER),
            "mostro/chat_read_state/peer/order-123"
        );
        assert_eq!(
            storage_key("dispute-456", DISPUTE),
            "mostro/chat_read_state/dispute/dispute-456"
        );
    }
}
