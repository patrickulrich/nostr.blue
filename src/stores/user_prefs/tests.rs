//! Wire-format + merge-helper tests for the unified preference blobs.
//!
//! Validates the same properties amethyst's `PinnedChatroomsSyncTest` locks
//! in, plus a whole-blob round-trip test (which amethyst is missing — this
//! is a gap we fill).

use nostr::Keys;
use nostr::nips::nip44::{self, Version};
use nostr::secp256k1::rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::stores::user_prefs::blob::UserPrefsBlob;
use crate::stores::user_prefs::mostro_blob::MostroPrefsBlob;
use crate::stores::user_prefs::{MAX_RECENT_TRADES, PREFS_D_TAG};

// ─── Whole-blob round-trip ──────────────────────────────────────────────

#[test]
fn user_prefs_blob_round_trip() {
    let blob = UserPrefsBlob {
        version: 1,
        settings: crate::stores::ui::settings_store::AppSettings::default(),
        sidebar: crate::stores::ui::sidebar_store::SidebarPreferencesData::default(),
        reactions: vec![],
        ai_credentials: crate::stores::ui::ai_provider_store::AiProviderState::default(),
        notifications_checked_at: 1700000000,
        cashu_terms_accepted: Some(1),
        p2p_terms_accepted: Some(3),
    };
    let json = serde_json::to_string(&blob).expect("serialize");
    let decoded: UserPrefsBlob = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(blob, decoded);
}

#[test]
fn mostro_prefs_blob_round_trip() {
    let blob = MostroPrefsBlob {
        version: 1,
        settings: crate::stores::ui::p2p_settings::MostroSettings::default(),
        node_config: None,
        recent_trades: vec![],
        archive_cursor: None,
    };
    let json = serde_json::to_string(&blob).expect("serialize");
    let decoded: MostroPrefsBlob = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(blob, decoded);
}

// ─── Forward-compat: missing fields default ─────────────────────────────

#[test]
fn empty_json_defaults_all_fields() {
    // A `{}` blob (written before any fields existed) should parse with
    // all defaults, matching amethyst's `blobWithoutPinnedRoomsFieldDefaultsToEmpty`.
    let decoded: UserPrefsBlob = serde_json::from_str("{}").expect("empty blob should parse");
    assert_eq!(decoded.version, 1);
    assert_eq!(decoded.notifications_checked_at, 0);
    assert!(decoded.cashu_terms_accepted.is_none());
    assert!(decoded.p2p_terms_accepted.is_none());
}

#[test]
fn blob_missing_version_defaults_to_v1() {
    // If the `version` field is absent, it defaults to 1.
    let json = r#"{"notifications_checked_at": 42}"#;
    let decoded: UserPrefsBlob = serde_json::from_str(json).expect("partial blob should parse");
    assert_eq!(decoded.version, 1);
    assert_eq!(decoded.notifications_checked_at, 42);
}

// ─── NIP-44 encrypt/decrypt round-trip ──────────────────────────────────

#[test]
fn nip44_encrypt_decrypt_round_trip() {
    let keys = Keys::generate();
    let sk = keys.secret_key();
    let pk = keys.public_key();
    let blob = UserPrefsBlob::default();
    let json = serde_json::to_string(&blob).expect("serialize");

    let encrypted =
        nip44::encrypt_with_rng(&mut OsRng, sk, &pk, json.as_bytes(), Version::V2)
            .expect("encrypt");

    // The ciphertext must NOT contain the plaintext.
    assert!(!encrypted.contains("nostr.blue"));
    assert!(!encrypted.contains("notifications_checked_at"));

    // Round-trip: decrypt should recover the blob.
    let plaintext = nip44::decrypt(sk, &pk, &encrypted).expect("decrypt");
    let recovered: UserPrefsBlob = serde_json::from_str(&plaintext).expect("parse");
    assert_eq!(blob, recovered);
}

#[test]
fn legacy_plaintext_still_parses() {
    // Events published before encryption was added are plaintext JSON.
    let blob = UserPrefsBlob::default();
    let plaintext = serde_json::to_string(&blob).expect("serialize");

    // Simulate decrypt-from-self-or-legacy on plaintext content.
    // The NIP-44 decrypt will fail (it's not base64 ciphertext), so the
    // legacy fallback should parse the plaintext JSON directly.
    let keys = Keys::generate();
    let result: Result<UserPrefsBlob, String> = match nip44::decrypt(
        keys.secret_key(),
        &keys.public_key(),
        &plaintext,
    ) {
        Ok(_) => Err("Should have failed NIP-44 on plaintext".to_string()),
        Err(_) => serde_json::from_str::<UserPrefsBlob>(&plaintext)
            .map_err(|e| format!("plaintext parse: {e}")),
    };
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), blob);
}

#[test]
fn wrong_key_cannot_decrypt() {
    let keys_a = Keys::generate();
    let keys_b = Keys::generate();
    let json = r#"{"version":1}"#;
    let encrypted = nip44::encrypt_with_rng(
        &mut OsRng,
        keys_a.secret_key(),
        &keys_a.public_key(),
        json.as_bytes(),
        Version::V2,
    )
    .expect("encrypt");

    // Decrypt with keys_b should fail.
    let result = nip44::decrypt(keys_b.secret_key(), &keys_b.public_key(), &encrypted);
    assert!(result.is_err(), "decryption with wrong key must fail");
}

// ─── Trade bounding + merge ─────────────────────────────────────────────

/// Create a Trade with the given order_id and updated_at, using serde
/// default for all other fields (Trade has `#[serde(default)]` on all fields).
fn test_trade(order_id: &str, updated_at: i64) -> crate::stores::mostro::trade_store::Trade {
    let d_tag = format!("d-{order_id}");
    let json = format!(
        r#"{{"order_id":"{order_id}","d_tag":"{d_tag}","maker_pubkey":"abc","role":"maker","kind":"sell","fiat_amount":"100","fiat_code":"USD","premium":0.0,"payment_methods":["SEPA"],"status":"pending","created_at":0,"updated_at":{updated_at}}}"#
    );
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("test_trade serde: {e}"))
}

#[test]
fn bound_trades_keeps_newest_50() {
    let mut blob = MostroPrefsBlob::default();
    for i in 0..60i64 {
        blob.recent_trades.push(test_trade(&format!("order-{i}"), i));
    }
    let spillover = blob.bound_trades();
    assert_eq!(blob.recent_trades.len(), MAX_RECENT_TRADES);
    assert_eq!(spillover.len(), 10);
    // The newest (i=59) should be at index 0 (descending sort).
    assert_eq!(blob.recent_trades[0].order_id, "order-59");
    // The oldest (i=0) should be in the spillover.
    assert_eq!(spillover.last().unwrap().order_id, "order-0");
}

#[test]
fn merge_trades_unions_by_order_id() {
    use crate::stores::mostro::trade_store::Trade;
    let local: Vec<Trade> = vec![
        test_trade("A", 100),
        test_trade("B", 200),
    ];
    let remote: Vec<Trade> = vec![
        test_trade("B", 300),
        test_trade("C", 50),
    ];
    let merged = MostroPrefsBlob::merge_trades(&local, &remote);
    // Union: A, B, C
    assert_eq!(merged.len(), 3);
    // B should have the remote (newer) updated_at.
    let b = merged.iter().find(|t| t.order_id == "B").unwrap();
    assert_eq!(b.updated_at, 300);
}

// ─── Merge helpers ──────────────────────────────────────────────────────

#[test]
fn notifications_checked_at_takes_max() {
    let local = UserPrefsBlob {
        notifications_checked_at: 100,
        ..Default::default()
    };
    let remote = UserPrefsBlob {
        notifications_checked_at: 200,
        ..Default::default()
    };
    let merged = UserPrefsBlob::merge(&local, &remote);
    assert_eq!(merged.notifications_checked_at, 200);

    // Reverse order: local is newer.
    let merged = UserPrefsBlob::merge(&remote, &local);
    assert_eq!(merged.notifications_checked_at, 200);
}

// ─── d-tag constants ────────────────────────────────────────────────────

#[test]
fn d_tag_constants_are_stable() {
    // These values MUST NOT change — they are the wire-format identifier
    // that relays use to group replaceable events.
    assert_eq!(PREFS_D_TAG, "nostr.blue/prefs");
    assert_eq!(
        crate::stores::user_prefs::MOSTRO_PREFS_D_TAG,
        "nostr.blue/p2p"
    );
    assert_eq!(
        crate::stores::user_prefs::TRADES_ARCHIVE_D_TAG,
        "nostr.blue/p2p/trades-archive"
    );
    assert_eq!(MAX_RECENT_TRADES, 50);
}
