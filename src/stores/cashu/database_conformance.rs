//! WalletDatabase conformance suite (cdk-common's public test harness).
//!
//! Runs the generic conformance functions from
//! `cdk_common::database::wallet::test` against **cdk-redb** — the backend
//! desktop wallets persist ecash to (`wallet.redb`). The harness helpers
//! use fixed test IDs, so every function gets a fresh database (matching
//! how cdk's own CI drives the suite). The custom wasm IndexedDB backend
//! (`crate::stores::indexeddb_database`) is browser-bound by design (its
//! native stub errors loudly on every method), so running the same harness
//! against it requires a wasm/browser test runner — tracked as separate
//! infrastructure, not smuggled in here.
//!
//! Runs under `cargo test --no-default-features --features desktop`
//! (the CI native test pass); `cashu_native` gates the cdk-redb dependency.
//! cdk-common's `test` feature (the harness module gate) comes from our
//! `[dev-dependencies]` — cdk-redb stopped leaking it as a regular dep in
//! 0.18. All 56 harness functions are driven directly (the upstream
//! `wallet_db_test!` macro list covers 51 of them).

async fn fresh_db(tag: &str) -> (cdk_redb::WalletRedbDatabase, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "nostrblue-conf-{tag}-{}.redb",
        uuid::Uuid::new_v4()
    ));
    let db = cdk_redb::WalletRedbDatabase::new(&path).expect("open redb wallet database");
    (db, path)
}

fn cleanup(path: std::path::PathBuf) {
    if let Err(e) = std::fs::remove_file(&path) {
        // Best-effort cleanup of the temp database.
        log::debug!("conformance cleanup: {e}");
    }
}

/// Run one harness function against a fresh database.
macro_rules! conformance {
    ($tag:literal, $fn_name:path) => {{
        let (db, path) = fresh_db($tag).await;
        $fn_name(db).await;
        cleanup(path);
    }};
}

#[tokio::test]
async fn redb_passes_wallet_database_conformance() {
    use cdk_common::database::wallet::test;

    // Mints
    conformance!("mint_add_get", test::add_and_get_mint);
    conformance!("mint_no_info", test::add_mint_without_info);
    conformance!("mint_remove", test::remove_mint);
    conformance!("mint_update_url", test::update_mint_url);
    // Keysets & keys
    conformance!("keysets", test::add_and_get_keysets);
    conformance!("keyset_in_txn", test::get_keyset_by_id_in_transaction);
    conformance!("keys_add_get", test::add_and_get_keys);
    conformance!("keys_in_txn", test::get_keys_in_transaction);
    conformance!("keys_remove", test::remove_keys);
    conformance!("keyset_counter", test::increment_keyset_counter);
    conformance!("keyset_counter_isolation", test::keyset_counter_isolation);
    // Derivation counters (cdk 0.18; IndexedDB schema v9)
    conformance!(
        "derivation_counter",
        test::increment_derivation_counter
    );
    conformance!(
        "derivation_counter_independent",
        test::derivation_counters_are_independent
    );
    conformance!(
        "derivation_counter_atomic",
        test::derivation_counter_is_atomic
    );
    // Mint quotes
    conformance!("mint_quote", test::add_and_get_mint_quote);
    conformance!("mint_quote_txn", test::get_mint_quote_in_transaction);
    conformance!("mint_quote_rm", test::remove_mint_quote);
    conformance!(
        "mint_quote_lock",
        test::add_mint_quote_optimistic_locking
    );
    // Melt quotes
    conformance!("melt_quote", test::add_and_get_melt_quote);
    conformance!("melt_quote_txn", test::get_melt_quote_in_transaction);
    conformance!("melt_quote_rm", test::remove_melt_quote);
    conformance!(
        "melt_quote_lock",
        test::add_melt_quote_optimistic_locking
    );
    // Proofs
    conformance!("proofs_add_get", test::add_and_get_proofs);
    conformance!("proofs_txn", test::get_proofs_in_transaction);
    conformance!("proofs_update", test::update_proofs);
    conformance!("proofs_state", test::update_proofs_state);
    conformance!("proofs_by_unit", test::filter_proofs_by_unit);
    conformance!("proofs_by_state", test::filter_proofs_by_state);
    conformance!("balance", test::get_balance);
    conformance!("balance_by_state", test::get_balance_by_state);
    conformance!("reserve", test::reserve_proofs);
    conformance!("release", test::release_proofs);
    conformance!("reserved_get", test::get_reserved_proofs);
    conformance!(
        "reserve_twice",
        test::reserve_proofs_already_reserved
    );
    // Transactions
    conformance!("txn_add_get", test::add_and_get_transaction);
    conformance!("txn_list", test::list_transactions);
    conformance!("txn_by_mint", test::filter_transactions_by_mint);
    conformance!("txn_rm", test::remove_transaction);
    // Generic KV store
    conformance!("kv_write_read", test::kvstore_write_and_read);
    conformance!("kv_list", test::kvstore_list);
    conformance!("kv_update", test::kvstore_update);
    conformance!("kv_remove", test::kvstore_remove);
    conformance!("kv_namespace", test::kvstore_namespace_isolation);
    // P2PK signing keys
    conformance!("p2pk_add_get", test::add_and_get_p2pk_key);
    conformance!("p2pk_list_empty", test::list_p2pk_keys_empty);
    conformance!(
        "p2pk_list_multi",
        test::list_p2pk_keys_multiple
    );
    conformance!("p2pk_latest_empty", test::latest_p2pk_empty);
    conformance!("p2pk_latest", test::latest_p2pk_with_keys);
    // Wallet sagas
    conformance!("saga_add_get", test::add_and_get_saga);
    conformance!("saga_lock", test::update_saga_optimistic_locking);
    conformance!("saga_delete", test::delete_saga);
    conformance!("sagas_incomplete", test::get_incomplete_sagas);
    // Transaction lifecycle & proof reservations (cdk 0.18)
    conformance!(
        "transaction_status_update",
        test::update_transaction_status
    );
    conformance!(
        "same_proofs_different_sagas",
        test::same_proofs_in_different_sagas
    );
    conformance!(
        "release_proofs_preserves_spent",
        test::release_proofs_preserves_spent
    );
    conformance!(
        "reserve_proofs_atomic",
        test::reserve_proofs_is_atomic
    );
}
