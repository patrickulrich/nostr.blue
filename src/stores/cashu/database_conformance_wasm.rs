//! Browser conformance suite for the wasm IndexedDB `WalletDatabase`.
//!
//! Runs the same cdk-common harness functions as the redb suite
//! (`database_conformance.rs`) against `IndexedDbDatabase` inside a real
//! browser (`cargo test --target wasm32-unknown-unknown`; the runner is
//! registered in `.cargo/config.toml`, CI drives it in a headless chromium).
//! This is the gate that proves the IndexedDB saga/transaction/reservation
//! implementation before recovery flows depend on it — the implementation is
//! browser-bound, so compilation alone never exercised it before.
//!
//! Every case opens a uniquely-named database (harness fixtures use fixed
//! IDs) and deletes it afterwards, so cases cannot leak state even though
//! IndexedDB persists between test runs.

#![cfg(all(test, target_arch = "wasm32", feature = "web", not(feature = "native")))]

use cdk_common::database::wallet::test;
use indexed_db_futures::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

use crate::stores::indexeddb_database::IndexedDbDatabase;

async fn fresh_db(tag: &str) -> (IndexedDbDatabase, String) {
    let name = format!("cashu-conf-{}-{}", tag, uuid::Uuid::new_v4());
    let db = IndexedDbDatabase::new_with_name(&name)
        .await
        .expect("open fresh IndexedDB wallet database");
    (db, name)
}

/// Best-effort `deleteDatabase` after closing the connection — a failure
/// here never fails a case (unique names make leftovers inert).
async fn cleanup(db: IndexedDbDatabase, name: &str) {
    db.close();
    if let Ok(req) = IdbDatabase::delete_by_name(name) {
        let _ = req.await;
    }
}

/// Run one harness function against a fresh database.
macro_rules! conformance {
    ($tag:literal, $fn_name:path) => {{
        let (db, name) = fresh_db($tag).await;
        $fn_name(db.clone()).await;
        cleanup(db, &name).await;
    }};
}

#[wasm_bindgen_test]
async fn indexeddb_conformance_mints_and_keysets() {
    conformance!("mint_add_get", test::add_and_get_mint);
    conformance!("mint_no_info", test::add_mint_without_info);
    conformance!("mint_remove", test::remove_mint);
    conformance!("mint_update_url", test::update_mint_url);
    conformance!("keysets", test::add_and_get_keysets);
    conformance!("keyset_in_txn", test::get_keyset_by_id_in_transaction);
    conformance!("keys_add_get", test::add_and_get_keys);
    conformance!("keys_in_txn", test::get_keys_in_transaction);
    conformance!("keys_remove", test::remove_keys);
    conformance!("keyset_counter", test::increment_keyset_counter);
    conformance!("keyset_counter_isolation", test::keyset_counter_isolation);
}

#[wasm_bindgen_test]
async fn indexeddb_conformance_derivation_counters() {
    // cdk 0.18 additions backing IndexedDB schema v9
    conformance!("derivation_counter", test::increment_derivation_counter);
    conformance!(
        "derivation_counter_independent",
        test::derivation_counters_are_independent
    );
    conformance!(
        "derivation_counter_atomic",
        test::derivation_counter_is_atomic
    );
}

#[wasm_bindgen_test]
async fn indexeddb_conformance_quotes() {
    conformance!("mint_quote", test::add_and_get_mint_quote);
    conformance!("mint_quote_txn", test::get_mint_quote_in_transaction);
    conformance!("mint_quote_rm", test::remove_mint_quote);
    conformance!("mint_quote_lock", test::add_mint_quote_optimistic_locking);
    conformance!("melt_quote", test::add_and_get_melt_quote);
    conformance!("melt_quote_txn", test::get_melt_quote_in_transaction);
    conformance!("melt_quote_rm", test::remove_melt_quote);
    conformance!("melt_quote_lock", test::add_melt_quote_optimistic_locking);
}

#[wasm_bindgen_test]
async fn indexeddb_conformance_proofs_and_balance() {
    conformance!("proofs_add_get", test::add_and_get_proofs);
    conformance!("proofs_txn", test::get_proofs_in_transaction);
    conformance!("proofs_update", test::update_proofs);
    conformance!("proofs_state", test::update_proofs_state);
    conformance!("proofs_by_unit", test::filter_proofs_by_unit);
    conformance!("proofs_by_state", test::filter_proofs_by_state);
    conformance!("balance", test::get_balance);
    conformance!("balance_by_state", test::get_balance_by_state);
}

#[wasm_bindgen_test]
async fn indexeddb_conformance_reservations() {
    conformance!("reserve", test::reserve_proofs);
    conformance!("release", test::release_proofs);
    conformance!("reserved_get", test::get_reserved_proofs);
    conformance!("reserve_twice", test::reserve_proofs_already_reserved);
    conformance!(
        "release_proofs_preserves_spent",
        test::release_proofs_preserves_spent
    );
    conformance!("reserve_proofs_atomic", test::reserve_proofs_is_atomic);
}

#[wasm_bindgen_test]
async fn indexeddb_conformance_transactions_and_kv() {
    conformance!("txn_add_get", test::add_and_get_transaction);
    conformance!("txn_list", test::list_transactions);
    conformance!("txn_by_mint", test::filter_transactions_by_mint);
    conformance!("txn_rm", test::remove_transaction);
    conformance!("transaction_status_update", test::update_transaction_status);
    conformance!(
        "same_proofs_different_sagas",
        test::same_proofs_in_different_sagas
    );
    conformance!("kv_write_read", test::kvstore_write_and_read);
    conformance!("kv_list", test::kvstore_list);
    conformance!("kv_update", test::kvstore_update);
    conformance!("kv_remove", test::kvstore_remove);
    conformance!("kv_namespace", test::kvstore_namespace_isolation);
}

#[wasm_bindgen_test]
async fn indexeddb_conformance_p2pk_keys() {
    conformance!("p2pk_add_get", test::add_and_get_p2pk_key);
    conformance!("p2pk_list_empty", test::list_p2pk_keys_empty);
    conformance!("p2pk_list_multi", test::list_p2pk_keys_multiple);
    conformance!("p2pk_latest_empty", test::latest_p2pk_empty);
    // `latest_p2pk_with_keys` (native-only) sleeps for 1s via
    // `tokio::time::sleep` to order `created_time` stamps — tokio's clock
    // always panics on wasm32-unknown-unknown (std Instant is unimplemented
    // there, and no feature swaps it). Upstream keeps the p2pk tests out of
    // their default macro list too; the store logic stays covered by the
    // add/get/list/latest-empty cases plus the full native redb run.
}

#[wasm_bindgen_test]
async fn indexeddb_conformance_sagas() {
    conformance!("saga_add_get", test::add_and_get_saga);
    conformance!("saga_lock", test::update_saga_optimistic_locking);
    conformance!("saga_delete", test::delete_saga);
    conformance!("sagas_incomplete", test::get_incomplete_sagas);
}
