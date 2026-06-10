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

pub mod client;
pub mod cleanup;
pub mod communities;
pub mod discovery;
pub mod encrypted_attachment;
pub mod flow;
pub mod helpers;
pub mod keys;
pub mod nip78;
pub mod node_config;
pub mod reconciliation;
pub mod restore;
pub mod source_tag;
pub mod take;
pub mod trade_store;

#[allow(unused_imports)]
pub use helpers::{cant_do_message, parse_node_pubkey};

#[allow(unused_imports)]
pub use communities::{default_node_config, find_by_pubkey, MostroCommunity, COMMUNITIES};

#[allow(unused_imports)]
pub use discovery::{discover_daemons, switch_to_daemon, DiscoveredDaemon};

#[allow(unused_imports)]
pub use client::{
    active_trade_backfill_filter, active_trade_filter, apply_mostro_action,
    build_trade_key_map, check_relay_health,
    ensure_node_relays_connected, node_info_filter, order_live_filter, send_mostro_message,
    unwrap_mostro_response,
};
#[allow(unused_imports)]
pub use flow::{
    accept_cancel, add_bond_invoice, add_invoice, cancel, dispute, fiat_sent, last_trade_index,
    new_order, rate_user, release, request_orders, restore_session, send_dm, take_buy, take_sell,
    validate_invoice, validate_invoice_with_amount,
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
pub use node_config::{
    clear_config as clear_node_config, init_from_cache as init_node_config_from_cache,
    refresh_from_relays as refresh_node_config, save_config as save_node_config,
    sync_relays_from_nip65, try_get as try_get_node_config, update_pow_from_event,
    MostroNodeConfig, MostroNodeInfo, MOSTRO_NODE_CONFIG, MOSTRO_NODE_INFO, NODE_CONFIG_D_TAG,
    NODE_CONFIG_VERSION,
};
#[allow(unused_imports)]
pub use restore::{
    handle_restore_event, handle_orders_event, init_from_cache as init_restore_from_cache,
    request_restore, RestoreStage, RestoreState, RESTORE_STATE,
};
#[allow(unused_imports)]
pub use source_tag::{parse_source_tag, ParsedSourceTag};
#[allow(unused_imports)]
pub use take::{take_order, TakeRequest, TakeResult};
#[allow(unused_imports)]
pub use reconciliation::{reconcile_order_event, build_reconciliation_filter};
#[allow(unused_imports)]
pub use cleanup::run_cleanup_loop;
#[allow(unused_imports)]
pub use trade_store::{
    active_trades, apply_status, find_by_order_id, init_from_cache as init_trades_from_cache,
    publish as publish_trades, refresh_from_relays as refresh_trades, remove as remove_trade,
    upsert as upsert_trade, Trade, TradeRole, TradeStatus, TRADES, TRADES_D_TAG, TRADES_VERSION,
};

pub fn reset_all() {
    keys::reset();
    node_config::reset();
    trade_store::clear_all();
    nip78::reset();
    restore::reset();
    *PENDING_CREATE_SUB.write() = None;
    crate::stores::social::p2p_store::clear_caches();
}

pub async fn reset_all_with_publish() -> Result<(), String> {
    node_config::clear_config().await?;
    keys::reset();
    trade_store::clear_all();
    nip78::reset();
    restore::reset();
    *PENDING_CREATE_SUB.write() = None;
    crate::stores::social::p2p_store::clear_caches();
    Ok(())
}
