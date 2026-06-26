//! Mostro admin/solver key management.
//!
//! Admin keys are used by solvers to take disputes, settle/cancel trades,
//! and chat with both parties. They are distinct from MostroKeys:
//!
//! - Admin keys sign BOTH layers of NIP-59 gift wraps (identity == trade key).
//! - Admin keys don't rotate per-trade.
//! - Admin messages use `Message::new_dispute(...)` with `trade_index: None`.
//!
//! Keys are stored via `platform::storage` (plaintext localStorage on web,
//! native equivalent on desktop/mobile). This matches the pattern used for
//! the Mostro trading mnemonic. For production solvers, consider using a
//! dedicated hardware-backed secret store.

use dioxus::prelude::*;
use nostr::Keys;

const ADMIN_NSEC_KEY: &str = "mostro_admin_nsec";

/// Admin/solver key material.
#[derive(Clone, Debug)]
pub struct AdminKeys {
    pub keys: Keys,
}

/// Global reactive state. `None` when no admin keys are loaded.
pub static ADMIN_KEYS: GlobalSignal<Option<AdminKeys>> = Signal::global(|| None);

#[allow(dead_code)]
pub fn try_get() -> Option<AdminKeys> {
    ADMIN_KEYS.read().clone()
}

/// Load admin keys from a plaintext nsec string. Validates the nsec format
/// and persists to `platform::storage`.
#[allow(dead_code)]
pub fn load_from_nsec(nsec: &str) -> Result<(), String> {
    let trimmed = nsec.trim();
    let keys = Keys::parse(trimmed).map_err(|e| format!("Invalid nsec: {e}"))?;
    crate::platform::storage::set_string(ADMIN_NSEC_KEY, trimmed)
        .map_err(|e| format!("Failed to persist admin keys: {e}"))?;
    *ADMIN_KEYS.write() = Some(AdminKeys { keys });
    Ok(())
}

/// Clear admin keys from storage and state.
#[allow(dead_code)]
pub fn clear() {
    let _ = crate::platform::storage::delete(ADMIN_NSEC_KEY);
    *ADMIN_KEYS.write() = None;
}

/// Load from local cache at app startup.
#[allow(dead_code)]
pub fn init_from_cache() {
    if let Some(nsec) = crate::platform::storage::get_string(ADMIN_NSEC_KEY) {
        if !nsec.is_empty() {
            if let Err(e) = load_from_nsec(&nsec) {
                log::warn!("Failed to load cached admin keys: {e}");
            }
        }
    }
}

/// The admin's public key as hex (convenience accessor).
#[allow(dead_code)]
pub fn pubkey_hex() -> Option<String> {
    ADMIN_KEYS.read().as_ref().map(|ak| ak.keys.public_key().to_hex())
}
