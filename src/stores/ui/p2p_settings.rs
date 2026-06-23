//! Mostro P2P user settings (NIP-78 kind 30078).
//!
//! Phase 7: persists Mostro-specific user preferences:
//! - Default fiat currency
//! - Default Lightning address
//! - Notification toggles (trade updates, chat, dispute, sound, vibration)
//! - Trade history expiration period
//!
//! Follows the canonical 3-step NIP-78 load pattern from `sidebar_store.rs`:
//! 1. localStorage cache (sync, instant UI)
//! 2. nostr-sdk local DB query
//! 3. Relay fetch with fallback to defaults
//!
//! Stored as a kind 30078 event with d-tag `nostr.blue/p2p/settings`,
//! encrypted via the Mostro identity key (same as trades/node_config —
//! see `private_app_data.rs`).

use dioxus::prelude::*;
use nostr_sdk::{Filter, Kind};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::platform::storage;
use crate::stores::{auth_store, nostr_client};
use crate::stores::ui::sidebar_store::Nip78LoadState;

const APP_DATA_KIND: u16 = 30078;
const P2P_SETTINGS_D_TAG: &str = "nostr.blue/p2p/settings";
const P2P_SETTINGS_STORAGE_KEY: &str = "nostr_blue_p2p_settings";

/// Schema version — bump when the struct changes.
const SETTINGS_VERSION: u32 = 1;

/// Default trade-history expiration in days (0 = never expire).
const DEFAULT_TRADE_HISTORY_DAYS: u32 = 30;

/// User-configurable Mostro P2P preferences.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MostroSettings {
    #[serde(default = "default_version")]
    pub version: u32,
    /// Preferred fiat currency for order creation/filtering (ISO 4217).
    #[serde(default)]
    pub default_fiat_code: Option<String>,
    /// Preferred Lightning Address (user@domain.com) for auto-fill on
    /// sell-take and buy-order invoice fields.
    #[serde(default)]
    pub default_ln_address: Option<String>,
    /// Whether to surface local notifications for trade status updates
    /// (PayInvoice, FiatSentOk, Released, etc.).
    #[serde(default = "default_true")]
    pub notify_trade_updates: bool,
    /// Whether to surface local notifications for P2P chat messages.
    #[serde(default = "default_true")]
    pub notify_chat_messages: bool,
    /// Whether to surface local notifications for dispute updates
    /// (solver assigned, admin settled/canceled, etc.).
    #[serde(default = "default_true")]
    pub notify_dispute_updates: bool,
    /// Play a sound with notifications.
    #[serde(default)]
    pub notify_sound: bool,
    /// Vibrate with notifications (mobile only).
    #[serde(default)]
    pub notify_vibration: bool,
    /// Trade history expiration in days. 0 = never expire. Controls the
    /// cleanup loop's `MAX_AGE_SECS`.
    #[serde(default = "default_trade_history_days")]
    pub trade_history_expiration_days: u32,
}

fn default_version() -> u32 {
    SETTINGS_VERSION
}

fn default_true() -> bool {
    true
}

fn default_trade_history_days() -> u32 {
    DEFAULT_TRADE_HISTORY_DAYS
}

impl Default for MostroSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            default_fiat_code: None,
            default_ln_address: None,
            notify_trade_updates: true,
            notify_chat_messages: true,
            notify_dispute_updates: true,
            notify_sound: false,
            notify_vibration: false,
            trade_history_expiration_days: DEFAULT_TRADE_HISTORY_DAYS,
        }
    }
}

/// Global reactive state. Read with `MOSTRO_SETTINGS()`.
pub static MOSTRO_SETTINGS: GlobalSignal<MostroSettings> = Signal::global(MostroSettings::default);

/// Load state for the 3-step NIP-78 pattern.
pub static MOSTRO_SETTINGS_STATE: GlobalSignal<Nip78LoadState> =
    Signal::global(Nip78LoadState::default);

/// Load from local cache synchronously. Call at app startup (alongside
/// the other NIP-78 first-loaders) for instant availability.
pub fn init_from_cache() {
    if let Ok(json) = storage::get::<String>(P2P_SETTINGS_STORAGE_KEY) {
        if let Ok(parsed) = serde_json::from_str::<MostroSettings>(&json) {
            *MOSTRO_SETTINGS.write() = parsed;
            *MOSTRO_SETTINGS_STATE.write() = Nip78LoadState::LoadedDefaults;
            return;
        }
    }
    *MOSTRO_SETTINGS_STATE.write() = Nip78LoadState::Pending;
}

/// Fetch from relays and update state. Safe to call multiple times.
/// On `Failed` state, retries (the guard only blocks `Loading` and
/// already-`Loaded` states, matching the sidebar_store pattern).
pub async fn load_settings() -> Result<(), String> {
    {
        let state = MOSTRO_SETTINGS_STATE.read().clone();
        if state.is_loading() {
            return Ok(());
        }
        // Allow retry on Failed. Block only on definitively-loaded states.
        if matches!(state, Nip78LoadState::Loaded | Nip78LoadState::LoadedDefaults) {
            return Ok(());
        }
        *MOSTRO_SETTINGS_STATE.write() = Nip78LoadState::Loading;
    }

    let pubkey_str = match auth_store::get_pubkey() {
        Some(pk) => pk,
        None => {
            *MOSTRO_SETTINGS_STATE.write() = Nip78LoadState::LoadedDefaults;
            return Ok(());
        }
    };
    let pubkey = match nostr::PublicKey::from_hex(&pubkey_str) {
        Ok(pk) => pk,
        Err(_) => {
            *MOSTRO_SETTINGS_STATE.write() = Nip78LoadState::LoadedDefaults;
            return Ok(());
        }
    };

    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => {
            *MOSTRO_SETTINGS_STATE.write() = Nip78LoadState::Failed("Client not ready".into());
            return Ok(());
        }
    };

    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(P2P_SETTINGS_D_TAG)
        .limit(1);
    // Gate: ensure the user's NIP-65 outbox relays are in the pool before
    // fetching, so we query the right relays (not the bootstrap set).
    crate::stores::relay::wait_for_user_relays(
        std::time::Duration::from_secs(5),
        "p2p_settings::load_settings",
    )
    .await;
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let parsed = events
                .iter()
                .find_map(|e| evaluate_settings_event(e, &pubkey));
            if let Some(settings) = parsed {
                *MOSTRO_SETTINGS.write() = settings.clone();
                write_cache(&settings);
                *MOSTRO_SETTINGS_STATE.write() = Nip78LoadState::Loaded;
            } else {
                // No settings event found. Distinguish "user relays not
                // applied" (Failed → retry) from "genuinely no settings"
                // (LoadedDefaults). Prevents premature default-baking.
                *MOSTRO_SETTINGS_STATE.write() =
                    if !*crate::stores::relay::USER_RELAYS_APPLIED.peek() {
                        Nip78LoadState::Failed(
                            "User relays not applied, retry needed".into(),
                        )
                    } else {
                        Nip78LoadState::LoadedDefaults
                    };
            }
            Ok(())
        }
        Err(e) => {
            log::warn!("Failed to fetch P2P settings: {e}");
            *MOSTRO_SETTINGS_STATE.write() = Nip78LoadState::Failed(e.to_string());
            Ok(())
        }
    }
}

/// Verify a NIP-78 event is a valid settings record owned by the user.
/// Phase 1.2: content may be NIP-44-encrypted (new) or plaintext (legacy).
fn evaluate_settings_event(
    event: &nostr_sdk::Event,
    user_pubkey: &nostr::PublicKey,
) -> Option<MostroSettings> {
    if event.pubkey != *user_pubkey {
        return None;
    }
    if event.verify().is_err() {
        return None;
    }
    let parsed: MostroSettings =
        crate::stores::private_app_data::decrypt_from_self_or_legacy(&event.content).ok()?;
    if parsed.version > SETTINGS_VERSION {
        return None;
    }
    Some(parsed)
}

/// Publish current settings to the user's write relays as an encrypted
/// kind 30078 event.
pub async fn publish() -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    let settings = MOSTRO_SETTINGS.read().clone();
    write_cache(&settings);

    let builder =
        match crate::stores::private_app_data::build_encrypted_event_builder(
            P2P_SETTINGS_D_TAG,
            &settings,
        ) {
            Ok(b) => b,
            Err(e) => {
                log::debug!("Skipping encrypted P2P settings publish: {e}");
                return Ok(());
            }
        };
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign P2P settings: {e}"))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("p2p_settings".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(())
}

/// Update a single field and publish + cache.
#[allow(dead_code)]
pub async fn update_and_publish<F: FnOnce(&mut MostroSettings)>(f: F) -> Result<(), String> {
    {
        let mut settings = MOSTRO_SETTINGS.write();
        f(&mut settings);
    }
    publish().await
}

fn write_cache(settings: &MostroSettings) {
    if let Ok(json) = serde_json::to_string(settings) {
        let _ = storage::set(P2P_SETTINGS_STORAGE_KEY, &json);
    }
}

/// Convenience: get the default fiat code, falling back to "USD".
pub fn default_fiat_or_usd() -> String {
    MOSTRO_SETTINGS
        .read()
        .default_fiat_code
        .clone()
        .unwrap_or_else(|| "USD".to_string())
}

/// Convenience: get the default LN address if set.
pub fn default_ln_address() -> Option<String> {
    MOSTRO_SETTINGS.read().default_ln_address.clone()
}

/// Convenience: get the trade-history expiration in days.
pub fn trade_history_expiration_days() -> u32 {
    MOSTRO_SETTINGS.read().trade_history_expiration_days
}

/// Phase 7.4: determine whether a Mostro action should produce a local
/// notification based on the user's per-category toggles. Used by Phase 9's
/// notification mapper to gate notifications before they reach the platform
/// notification API.
///
/// Categories:
/// - `Trade`: PayInvoice, PayBondInvoice, AddInvoice, FiatSentOk, Released,
///   Canceled, HoldInvoicePaymentSettled, PurchaseCompleted, PaymentFailed,
///   BuyerTookOrder, InvoiceUpdated, HoldInvoicePaymentAccepted,
///   HoldInvoicePaymentCanceled, WaitingSellerToPay, WaitingBuyerInvoice,
///   BondSlashed, BondInvoiceAccepted, BondPayoutCompleted
/// - `Dispute`: DisputeInitiatedByYou/Peer, AdminTookDispute, AdminCanceled,
///   AdminSettled, CooperativeCancel*
/// - `Chat`: SendDm, TradePubkey
/// - `Rate`: Rate, RateReceived
#[allow(dead_code)]
pub fn should_notify(action: &mostro_core::prelude::Action) -> bool {
    use mostro_core::prelude::Action as A;
    let settings = MOSTRO_SETTINGS.read();

    let is_trade = matches!(
        action,
        A::PayInvoice
            | A::PayBondInvoice
            | A::AddInvoice
            | A::FiatSentOk
            | A::Released
            | A::Canceled
            | A::HoldInvoicePaymentSettled
            | A::PurchaseCompleted
            | A::PaymentFailed
            | A::BuyerTookOrder
            | A::InvoiceUpdated
            | A::HoldInvoicePaymentAccepted
            | A::HoldInvoicePaymentCanceled
            | A::WaitingSellerToPay
            | A::WaitingBuyerInvoice
            | A::BondSlashed
            | A::BondInvoiceAccepted
            | A::BondPayoutCompleted
    );
    if is_trade {
        return settings.notify_trade_updates;
    }

    let is_dispute = matches!(
        action,
        A::DisputeInitiatedByYou
            | A::DisputeInitiatedByPeer
            | A::AdminTookDispute
            | A::AdminCanceled
            | A::AdminSettled
            | A::CooperativeCancelInitiatedByYou
            | A::CooperativeCancelInitiatedByPeer
            | A::CooperativeCancelAccepted
    );
    if is_dispute {
        return settings.notify_dispute_updates;
    }

    let is_chat = matches!(action, A::SendDm | A::TradePubkey);
    if is_chat {
        return settings.notify_chat_messages;
    }

    // Rate and other informational actions: always notify (they're rare).
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let s = MostroSettings::default();
        assert_eq!(s.version, SETTINGS_VERSION);
        assert!(s.notify_trade_updates);
        assert!(s.notify_chat_messages);
        assert!(!s.notify_sound);
        assert_eq!(s.trade_history_expiration_days, DEFAULT_TRADE_HISTORY_DAYS);
    }

    #[test]
    fn test_serde_roundtrip() {
        let s = MostroSettings {
            version: SETTINGS_VERSION,
            default_fiat_code: Some("EUR".to_string()),
            default_ln_address: Some("me@walletofsatoshi.com".to_string()),
            notify_trade_updates: false,
            notify_chat_messages: true,
            notify_dispute_updates: true,
            notify_sound: true,
            notify_vibration: false,
            trade_history_expiration_days: 90,
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: MostroSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, s);
    }

    #[test]
    fn test_serde_backward_compat_missing_fields() {
        // Simulates loading from an older version that didn't have all fields.
        let legacy_json = r#"{"version":1,"default_fiat_code":"USD"}"#;
        let parsed: MostroSettings = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(parsed.default_fiat_code.as_deref(), Some("USD"));
        assert!(parsed.notify_trade_updates, "missing bool field defaults via serde");
        assert_eq!(parsed.trade_history_expiration_days, DEFAULT_TRADE_HISTORY_DAYS);
    }

    #[test]
    fn test_default_fiat_fallback() {
        // Can't easily read the GlobalSignal from tests (needs Dioxus
        // runtime). Verify the logic directly: None → "USD".
        let s = MostroSettings::default();
        let fallback = s.default_fiat_code.clone().unwrap_or_else(|| "USD".to_string());
        assert_eq!(fallback, "USD");

        let s2 = MostroSettings {
            default_fiat_code: Some("EUR".to_string()),
            ..MostroSettings::default()
        };
        let fallback2 = s2.default_fiat_code.clone().unwrap_or_else(|| "USD".to_string());
        assert_eq!(fallback2, "EUR");
    }
}
