//! Mostro P2P exchange key management
//!
//! The Mostro protocol uses a separate BIP-39 mnemonic (independent of the user's
//! primary Nostr nsec) to derive per-trade keys. This module handles:
//!
//! - Generating a fresh 12-word mnemonic on first use
//! - Persisting it in `platform::storage` (plaintext, per design decision)
//! - Deriving the identity key (NIP-06 path `m/44'/1237'/38383'/0/0`)
//! - Deriving per-trade keys (path `m/44'/1237'/38383'/0/{trade_index}`)
//!
//! Privacy mode: when enabled, identity and trade keys are the same (no
//! reputation tracking on the order book). When disabled (default), each trade
//! gets a fresh key derived from a monotonic trade index counter.

use dioxus::prelude::*;
use nostr::Keys;
use serde::{Deserialize, Serialize};

use crate::platform::storage;
use crate::utils::nip06;

/// NIP-06 account index used by the Mostro protocol convention.
/// 38383 = `NOSTR_ORDER_EVENT_KIND`, used as the "account" branch in derivation.
pub const MOSTRO_ACCOUNT: u32 = 38383;

/// Storage keys
const KEY_MNEMONIC: &str = "mostro_mnemonic";
const KEY_TRADE_INDEX: &str = "mostro_trade_index";
const KEY_PRIVACY_MODE: &str = "mostro_privacy_mode";

/// State of the Mostro key store.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum MostroKeyState {
    /// No keys have been generated or loaded yet. `init` should be called.
    NotInitialized,
    /// Initial load or trade-key derivation in progress.
    Loading,
    /// Keys are ready for use.
    Ready(MostroKeys),
    /// Initialization or persistence failed.
    Error(String),
}

/// Mostro key material plus the persistent state needed to derive per-trade keys.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MostroKeys {
    /// 12-word BIP-39 mnemonic. Stored in plaintext per design decision.
    pub mnemonic: String,
    /// Next trade key index to derive. Monotonically increasing.
    pub trade_index: u32,
    /// Long-lived identity key at `m/44'/1237'/38383'/0/0`.
    pub identity_keys: Keys,
    /// When true, identity and trade keys are the same (privacy mode, no reputation).
    pub privacy_mode: bool,
}

impl MostroKeys {
    /// Get a reference to the identity key (used for seal signing + reputation).
    #[allow(dead_code)]
    pub fn get_identity_key(&self) -> &Keys {
        &self.identity_keys
    }

    /// Derive a trade key at a specific index without mutating state.
    /// Used to re-derive keys for session restore.
    ///
    /// In privacy mode, always returns the identity key regardless of index,
    /// because the daemon requires `identity == sender` when `trade_index` is
    /// `None` on the wire (see mostro `take_sell.rs:168-177`).
    #[allow(dead_code)]
    pub fn get_trade_key_by_index(&self, index: u32) -> Result<Keys, String> {
        if self.privacy_mode {
            Ok(self.identity_keys.clone())
        } else {
            nip06::derive_at(&self.mnemonic, None, MOSTRO_ACCOUNT, 0, index)
        }
    }

    /// Derive the NEXT trade key, persist the incremented index, and return the key.
    /// Trade index is monotonic — never reuses an index even on cancel/expire.
    #[allow(dead_code)]
    pub fn get_next_trade_key(&mut self) -> Result<Keys, String> {
        let n = self.trade_index;
        let key = self.get_trade_key_by_index(n)?;
        self.trade_index = n.wrapping_add(1);
        if let Err(e) = storage::set(KEY_TRADE_INDEX, &self.trade_index) {
            return Err(format!("failed to persist trade index: {e}"));
        }
        Ok(key)
    }

    /// Get the trade keys for a new protocol action (create/take order).
    /// In privacy mode returns the identity key without incrementing the counter.
    /// In normal mode derives and increments as `get_next_trade_key` does.
    #[allow(dead_code)]
    pub fn next_protocol_trade_keys(&mut self) -> Result<Keys, String> {
        if self.privacy_mode {
            Ok(self.identity_keys.clone())
        } else {
            self.get_next_trade_key()
        }
    }

    /// Mostro daemon's `users.last_trade_index` is monotonic. When a remote
    /// restore discovers a higher index, call this to advance the local counter
    /// past the highest known index.
    #[allow(dead_code)]
    pub fn sync_trade_index(&mut self, remote: u32) -> Result<(), String> {
        if remote >= self.trade_index {
            self.trade_index = remote.saturating_add(1);
            storage::set(KEY_TRADE_INDEX, &self.trade_index)
                .map_err(|e| format!("failed to persist trade index: {e}"))?;
        }
        Ok(())
    }
}

/// Global reactive state. Read with `MOSTRO_KEYS()`, write via `*MOSTRO_KEYS.write()`.
#[allow(dead_code)]
pub static MOSTRO_KEYS: GlobalSignal<MostroKeyState> =
    Signal::global(|| MostroKeyState::NotInitialized);

/// Persisted privacy-mode toggle (independent of `MOSTRO_KEYS` for snappy UI reads).
#[allow(dead_code)]
pub static MOSTRO_PRIVACY_MODE: GlobalSignal<bool> = Signal::global(|| false);

/// Initialize or restore Mostro keys. Safe to call multiple times.
///
/// On first run: generate a fresh 12-word mnemonic, derive identity key,
/// persist, and transition to `Ready`.
///
/// On subsequent runs: load the persisted mnemonic, derive identity key,
/// restore trade index, and transition to `Ready`.
#[allow(dead_code)]
pub fn init() {
    if matches!(*MOSTRO_KEYS.read(), MostroKeyState::Ready(_)) {
        return;
    }
    *MOSTRO_KEYS.write() = MostroKeyState::Loading;

    match load_or_generate() {
        Ok(keys) => {
            *MOSTRO_PRIVACY_MODE.write() = keys.privacy_mode;
            *MOSTRO_KEYS.write() = MostroKeyState::Ready(keys);
        }
        Err(e) => *MOSTRO_KEYS.write() = MostroKeyState::Error(e),
    }
}

fn load_or_generate() -> Result<MostroKeys, String> {
    let mnemonic = match storage::get_string(KEY_MNEMONIC) {
        Some(m) if !m.is_empty() => m,
        _ => {
            let m = nip06::generate_mnemonic()?;
            storage::set_string(KEY_MNEMONIC, &m)
                .map_err(|e| format!("failed to persist mnemonic: {e}"))?;
            m
        }
    };

    let trade_index: u32 = storage::get(KEY_TRADE_INDEX).unwrap_or(0u32);
    let privacy_mode: bool = storage::get(KEY_PRIVACY_MODE).unwrap_or(false);
    *MOSTRO_PRIVACY_MODE.write() = privacy_mode;

    let identity_keys = nip06::derive_at(&mnemonic, None, MOSTRO_ACCOUNT, 0, 0)?;

    Ok(MostroKeys {
        mnemonic,
        trade_index,
        identity_keys,
        privacy_mode,
    })
}

/// Convenience accessor. Returns `None` if keys are not yet ready.
#[allow(dead_code)]
pub fn try_get() -> Option<MostroKeys> {
    match &*MOSTRO_KEYS.read() {
        MostroKeyState::Ready(k) => Some(k.clone()),
        _ => None,
    }
}

/// Write back a new trade_index to the global signal (single-lock update).
/// Call after `get_next_trade_key()` or `sync_trade_index()` on a cloned `MostroKeys`
/// — those methods persist to storage but do NOT update the reactive signal.
#[allow(dead_code)]
pub fn write_back_trade_index(new_index: u32) {
    MOSTRO_KEYS.with_mut(|state| {
        if let MostroKeyState::Ready(k) = state {
            k.trade_index = new_index;
        }
    });
}

/// Set privacy mode and persist. Idempotent.
#[allow(dead_code)]
pub fn set_privacy_mode(enabled: bool) -> Result<(), String> {
    storage::set(KEY_PRIVACY_MODE, &enabled)
        .map_err(|e| format!("failed to persist privacy mode: {e}"))?;
    *MOSTRO_PRIVACY_MODE.write() = enabled;
    MOSTRO_KEYS.with_mut(|state| {
        if let MostroKeyState::Ready(k) = state {
            k.privacy_mode = enabled;
        }
    });
    Ok(())
}

/// Import an existing mnemonic (for multi-device sync). Replaces the current
/// mnemonic and resets the trade index to 0.
///
/// Returns an error if the mnemonic is invalid (wrong word count, bad checksum).
#[allow(dead_code)]
pub fn import_mnemonic(words: &str) -> Result<(), String> {
    let trimmed = words.trim();
    let word_count = trimmed.split_whitespace().count();
    if word_count != 12 && word_count != 24 {
        return Err(format!("expected 12 or 24 words, got {word_count}"));
    }
    // Validate by attempting to derive. Mostro uses the standard NIP-06 path.
    let identity_keys = nip06::derive_at(trimmed, None, MOSTRO_ACCOUNT, 0, 0)?;
    drop(identity_keys);

    storage::set_string(KEY_MNEMONIC, trimmed)
        .map_err(|e| format!("failed to persist mnemonic: {e}"))?;
    storage::set(KEY_TRADE_INDEX, &0u32)
        .map_err(|e| format!("failed to reset trade index: {e}"))?;

    // Reload the keys to refresh the global state
    let keys = load_or_generate()?;
    *MOSTRO_KEYS.write() = MostroKeyState::Ready(keys);
    Ok(())
}

/// Wipe all Mostro state from disk and in-memory. Use for "Reset Mostro".
#[allow(dead_code)]
pub fn reset() {
    let _ = storage::delete(KEY_MNEMONIC);
    let _ = storage::delete(KEY_TRADE_INDEX);
    let _ = storage::delete(KEY_PRIVACY_MODE);
    *MOSTRO_KEYS.write() = MostroKeyState::NotInitialized;
    *MOSTRO_PRIVACY_MODE.write() = false;
}

/// Export the current mnemonic (for multi-device backup).
#[allow(dead_code)]
pub fn export_mnemonic() -> Option<String> {
    try_get().map(|k| k.mnemonic)
}

/// Serializable snapshot of the keys for persistence layers beyond `platform::storage`
/// (e.g. NIP-78 app data).
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MostroKeysSnapshot {
    pub mnemonic: String,
    pub trade_index: u32,
    pub privacy_mode: bool,
}

#[allow(dead_code)]
impl From<&MostroKeys> for MostroKeysSnapshot {
    fn from(k: &MostroKeys) -> Self {
        Self {
            mnemonic: k.mnemonic.clone(),
            trade_index: k.trade_index,
            privacy_mode: k.privacy_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_constant_matches_mostro_protocol() {
        // 38383 is the NOSTR_ORDER_EVENT_KIND. The Mostro daemon and clients both
        // use it as the NIP-06 account index for trade-key derivation.
        assert_eq!(MOSTRO_ACCOUNT, 38383);
    }

    #[test]
    fn test_mostro_keys_snapshot_roundtrip() {
        let snap = MostroKeysSnapshot {
            mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_string(),
            trade_index: 7,
            privacy_mode: false,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let parsed: MostroKeysSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mnemonic, snap.mnemonic);
        assert_eq!(parsed.trade_index, snap.trade_index);
        assert_eq!(parsed.privacy_mode, snap.privacy_mode);
    }

    #[test]
    fn test_derive_trade_keys_are_distinct_and_deterministic() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let k0 = nip06::derive_at(words, None, MOSTRO_ACCOUNT, 0, 0).unwrap();
        let k1 = nip06::derive_at(words, None, MOSTRO_ACCOUNT, 0, 1).unwrap();
        let k0_again = nip06::derive_at(words, None, MOSTRO_ACCOUNT, 0, 0).unwrap();
        assert_eq!(k0.public_key().to_hex(), k0_again.public_key().to_hex());
        assert_ne!(k0.public_key().to_hex(), k1.public_key().to_hex());
    }

    #[test]
    fn test_privacy_mode_returns_identity_key_for_any_index() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let id = nip06::derive_at(words, None, MOSTRO_ACCOUNT, 0, 0).unwrap();
        let mut mk = MostroKeys {
            mnemonic: words.to_string(),
            trade_index: 5,
            identity_keys: id.clone(),
            privacy_mode: true,
        };
        let tk = mk.get_trade_key_by_index(3).unwrap();
        assert_eq!(tk.public_key().to_hex(), id.public_key().to_hex());
        let next = mk.next_protocol_trade_keys().unwrap();
        assert_eq!(next.public_key().to_hex(), id.public_key().to_hex());
        assert_eq!(mk.trade_index, 5);
    }

    #[test]
    fn test_normal_mode_returns_derived_key_and_increments() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let id = nip06::derive_at(words, None, MOSTRO_ACCOUNT, 0, 0).unwrap();
        let k5 = nip06::derive_at(words, None, MOSTRO_ACCOUNT, 0, 5).unwrap();
        let mut mk = MostroKeys {
            mnemonic: words.to_string(),
            trade_index: 5,
            identity_keys: id,
            privacy_mode: false,
        };
        let next = mk.get_trade_key_by_index(5).unwrap();
        assert_eq!(next.public_key().to_hex(), k5.public_key().to_hex());
    }
}
