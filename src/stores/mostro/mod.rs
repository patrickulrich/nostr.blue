//! Mostro P2P exchange integration.
//!
//! See `keys.rs` for key management. See `client.rs` for GiftWrap transport.
//! See `flow.rs` for the protocol state-machine message builders.
//! See `node_config.rs` for daemon selection. See `trade_store.rs` for
//! local trade persistence. See `source_tag.rs` for parsing the kind
//! 38383 `source` tag. See `nip78.rs` for terms-of-service NIP-78
//! publication + check. See `restore.rs` for session restore pipeline.

use nostr_sdk::SubscriptionId;
use nostr::PublicKey;
use dioxus::prelude::*;

pub static PENDING_CREATE_SUB: GlobalSignal<Option<(SubscriptionId, PublicKey)>> =
    Signal::global(|| None);

pub static TRADE_UNREAD: GlobalSignal<usize> = Signal::global(|| 0);

/// Phase 2.3 (M4/M8): toast queue populated by the background trade
/// monitor when a Mostro action arrives while no trade-detail page is
/// mounted (or while the page is mounted but the user is not actively
/// viewing it).
///
/// Each entry is `(title, body)`. A root-level component
/// (see `mostro_toast_drainer.rs`) drains this queue and renders the
/// toasts via `consume_toast`. Without this queue, daemon refusals
/// (`Action::CantDo`) and bond slash notifications (`Action::BondSlashed`)
/// arriving in the background were silently dropped.
pub static MOSTRO_BACKGROUND_TOASTS: GlobalSignal<Vec<(String, String)>> =
    Signal::global(Vec::new);

/// Phase 2.3: enqueue a toast to be drained by the root-level drainer.
pub fn enqueue_background_toast(title: String, body: String) {
    MOSTRO_BACKGROUND_TOASTS.write().push((title, body));
}

/// Phase 2.3: drain all pending background toasts. Called by the root
/// component on every render; returns the toasts to display.
pub fn drain_background_toasts() -> Vec<(String, String)> {
    let mut queue = MOSTRO_BACKGROUND_TOASTS.write();
    let drained = queue.clone();
    queue.clear();
    drained
}

pub mod admin_keys;
pub mod chat_read_state;
pub mod client;
pub mod cleanup;
pub mod communities;
pub mod creation_ledger;
pub mod dedup;
pub mod deeplink;
pub mod discovery;
pub mod dispute_store;
pub mod encrypted_attachment;
pub mod flow;
pub mod helpers;
pub mod i18n;
pub mod keys;
pub mod nip78;
pub mod node_config;
pub mod notification_store;
pub mod notifications;
pub mod ratings;
pub mod reconciliation;
pub mod restore;
pub mod source_tag;
pub mod take;
pub mod toast;
pub mod trade_store;
pub mod waiter;

#[allow(unused_imports)]
pub use toast::{emit_toasts, MostroToast, MostroToastKind};

#[allow(unused_imports)]
pub use dedup::{is_seen, mark_seen};
#[allow(unused_imports)]
pub use helpers::{cant_do_message, parse_node_pubkey};
#[allow(unused_imports)]
pub use admin_keys::{clear as clear_admin_keys, init_from_cache as init_admin_keys_from_cache, load_from_nsec, pubkey_hex as admin_pubkey_hex, try_get as try_get_admin_keys, AdminKeys, ADMIN_KEYS};
#[allow(unused_imports)]
pub use dispute_store::{clear_all as clear_disputes, filter_for_daemon as disputes_for_daemon, find_by_id as find_dispute, parse_dispute_event, upsert as upsert_dispute, Dispute, DisputeInitiator, DisputeStatus, DISPUTE_EVENT_KIND, DISPUTES};
#[allow(unused_imports)]
pub use waiter::{prune_waiter, prune_waiters_for_order, register_waiter, try_satisfy_waiter};

#[allow(unused_imports)]
pub use communities::{default_node_config, find_by_pubkey, MostroCommunity, COMMUNITIES};

#[allow(unused_imports)]
pub use discovery::{discover_daemons, switch_to_daemon, DiscoveredDaemon};

#[allow(unused_imports)]
pub use client::{
    active_trade_backfill_filter, active_trade_filter, apply_mostro_action,
    backfill_active_trades, build_trade_key_map, check_relay_health,
    start_background_trade_monitor, ensure_node_relays_connected, node_info_filter,
    order_live_filter, resolve_effective_pow, send_mostro_message, unwrap_mostro_response,
};
#[allow(unused_imports)]
pub use flow::{
    accept_cancel, add_bond_invoice, add_invoice, admin_add_solver, cancel, dispute, fiat_sent,
    last_trade_index, new_order, rate_user, release, request_orders, restore_session, send_dm,
    take_buy, take_sell, validate_invoice, validate_invoice_with_amount, SolverPermission,
};
#[allow(unused_imports)]
pub use keys::{
    export_mnemonic, import_mnemonic, init, reset, set_privacy_mode, try_get,
    write_back_trade_index, MostroKeyState, MostroKeys, MostroKeysSnapshot, MOSTRO_KEYS,
    MOSTRO_PRIVACY_MODE,
};
#[allow(unused_imports)]
pub use nip78::{
    accept_p2p_terms, check_p2p_terms_accepted, reset as reset_terms, P2P_TERMS_ACCEPTED,
    P2P_TERMS_D_TAG, P2P_TERMS_VERSION, P2P_TERMS_VERSION_ACCEPTED,
};
#[allow(unused_imports)]
pub use notification_store::{
    clear_all as clear_notifications, init_from_cache as init_notifications_from_cache,
    mark_all_read as mark_all_notifications_read, mark_read as mark_notification_read,
    push as push_notification, refresh_from_relays as refresh_notifications,
    reset as reset_notifications, unread_count as unread_notification_count, MostroNotification,
    NOTIFICATIONS, NOTIFICATIONS_D_TAG,
};
#[allow(unused_imports)]
pub use node_config::{
    clear_config as clear_node_config, init_from_cache as init_node_config_from_cache,
    refresh_from_relays as refresh_node_config, save_config as save_node_config,
    sync_relays_from_nip65, try_get as try_get_node_config, update_pow_from_event,
    update_relays_from_nip65_event, validate_against_node_limits,
    MostroNodeConfig, MostroNodeInfo, MOSTRO_NODE_CONFIG, MOSTRO_NODE_INFO, NODE_CONFIG_D_TAG,
    NODE_CONFIG_VERSION,
};
#[allow(unused_imports)]
pub use restore::{
    handle_restore_event, handle_orders_event, init_from_cache as init_restore_from_cache,
    is_restore_in_progress, merge_small_orders, recover_order_by_id, request_last_trade_index,
    request_restore, RestoreStage, RestoreState, RESTORE_STATE,
};
#[allow(unused_imports)]
pub use source_tag::{parse_source_tag, ParsedSourceTag};
#[allow(unused_imports)]
pub use take::{check_source_tag_daemon_switch, take_order, TakeRequest, TakeResult};
#[allow(unused_imports)]
pub use reconciliation::reconcile_order_event;
#[allow(unused_imports)]
pub use cleanup::{run_all_cleanup_loops, run_cleanup_loop};
#[allow(unused_imports)]
pub use ratings::{
    clear_ratings, format_stars, get_rating, parse_rating_event,
    rating_filter, rating_filter_batch, upsert_rating,
    RATINGS, RATING_EVENT_KIND,
};
#[allow(unused_imports)]
pub use trade_store::{
    active_trades, active_trades_for_daemon, all_trades_for_daemon, apply_status,
    find_by_order_id, init_from_cache as init_trades_from_cache,
    insert_range_child_placeholder,
    is_status_transition_allowed,
    publish as publish_trades, refresh_from_relays as refresh_trades, remove as remove_trade,
    upsert as upsert_trade, Trade, TradeRole, TradeStatus, TRADES, TRADES_D_TAG, TRADES_VERSION,
};

pub fn reset_all() {
    keys::reset();
    node_config::reset();
    trade_store::clear_all();
    nip78::reset();
    notification_store::reset();
    chat_read_state::reset_all();
    restore::reset();
    *PENDING_CREATE_SUB.write() = None;
    *TRADE_UNREAD.write() = 0;
    crate::stores::social::p2p_store::clear_caches();
}

pub async fn reset_all_with_publish() -> Result<(), String> {
    node_config::clear_config().await?;
    keys::reset();
    trade_store::clear_all();
    nip78::reset();
    notification_store::reset();
    chat_read_state::reset_all();
    restore::reset();
    *PENDING_CREATE_SUB.write() = None;
    *TRADE_UNREAD.write() = 0;
    crate::stores::social::p2p_store::clear_caches();
    Ok(())
}

#[allow(dead_code)]
pub fn increment_trade_unread() {
    *TRADE_UNREAD.write() += 1;
}

#[allow(dead_code)]
pub fn clear_trade_unread() {
    *TRADE_UNREAD.write() = 0;
}
