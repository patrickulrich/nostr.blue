//! Load functions for the unified preference blobs.
//!
//! Phase 1 (dual-read): the unified blob is fetched first. If found on
//! relays, it is decrypted and applied to the existing per-store
//! GlobalSignals. If not found (the common case during Phase 1, before
//! Phase 2 starts writing the unified blob), the legacy per-store loaders
//! handle everything as before.
//!
//! The load function is added to `run_post_login_init`'s `futures::join!`
//! alongside the existing legacy loaders. Both run in parallel; when both
//! find data (after Phase 2 ships), they write consistent values because
//! both read from the same user's relays.

use std::time::Duration;

use nostr::Event;

use crate::stores::relay::wait_for_user_relays;
use crate::stores::relay::USER_RELAYS_APPLIED;
use crate::stores::ui::sidebar_store::Nip78LoadState;
use crate::stores::user_prefs::apply::{check_and_lock, mark_applied, mark_failed, BlobSource};
use crate::stores::user_prefs::blob::UserPrefsBlob;
use crate::stores::user_prefs::mostro_blob::MostroPrefsBlob;
use super::{
    encrypt, fetch, LAST_PUBLISHED_EVENT_ID, LAST_PUBLISHED_MOSTRO_EVENT_ID, MOSTRO_PREFS_D_TAG,
    MOSTRO_PREFS_EVENT_ID, MOSTRO_PREFS_LOAD_STATE, PREFS_CACHE_PREFIX, MOSTRO_PREFS_CACHE_PREFIX,
    USER_PREFS_EVENT_ID, USER_PREFS_STATE, PREFS_D_TAG,
};

use dioxus::prelude::*;

/// Load the unified user preferences blob from relays.
///
/// Flow:
/// 1. Read localStorage cache (instant UI).
/// 2. Wait for user relays (`wait_for_user_relays` gate).
/// 3. Fetch `nostr.blue/prefs` via quorum-EOSE.
/// 4. Decrypt + apply to existing per-store GlobalSignals.
///
/// Returns `Ok(Some(blob))` if the unified blob was found and applied,
/// `Ok(None)` if not found (legacy loaders handle this case).
pub async fn load_user_prefs() -> Result<Option<UserPrefsBlob>, String> {
    if !crate::stores::auth_store::is_authenticated() {
        return Ok(None);
    }
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return Ok(None),
    };
    let pubkey = match crate::stores::nostr_client::get_cached_pubkey() {
        Ok(pk) => pk,
        Err(_) => return Ok(None),
    };

    // Step 1: localStorage cache for instant UI.
    let cache_key = format!("{PREFS_CACHE_PREFIX}{pubkey}");
    if let Ok(cached_json) = crate::platform::storage::get::<String>(&cache_key) {
        if let Ok(blob) = serde_json::from_str::<UserPrefsBlob>(&cached_json) {
            log::debug!("load_user_prefs: applying cached blob");
            apply_blob_to_signals(&blob, BlobSource::Cache);
        }
    }

    // Step 2: relay readiness gate.
    *USER_PREFS_STATE.write() = Nip78LoadState::Loading;
    wait_for_user_relays(
        Duration::from_secs(5),
        "user_prefs::load_user_prefs",
    )
    .await;

    // Step 3: fetch via quorum-EOSE.
    let filter = nostr::Filter::new()
        .author(pubkey)
        .kind(nostr::Kind::from(30078))
        .identifier(PREFS_D_TAG)
        .limit(1);

    let event =
        match fetch::fetch_newest_with_quorum(&client, filter, Duration::from_secs(10)).await {
            Ok(Some(e)) => e,
            Ok(None) => {
                // Unified blob not found on relays — legacy loaders handle this.
                *USER_PREFS_STATE.write() = if !*USER_RELAYS_APPLIED.peek() {
                    Nip78LoadState::Failed("User relays not applied, retry needed".into())
                } else {
                    Nip78LoadState::LoadedDefaults
                };
                return Ok(None);
            }
            Err(e) => {
                mark_failed(e, &USER_PREFS_STATE);
                return Ok(None);
            }
        };

    // Step 4: decrypt + apply.
    match decrypt_and_apply(event, BlobSource::Relay).await {
        Ok(blob) => {
            // Persist to localStorage cache.
            if let Ok(json) = serde_json::to_string(&blob) {
                let _ = crate::platform::storage::set(&cache_key, &json);
            }
            Ok(Some(blob))
        }
        Err(e) => {
            mark_failed(e, &USER_PREFS_STATE);
            Ok(None)
        }
    }
}

/// Decrypt the event and apply the blob to existing per-store GlobalSignals.
///
/// Uses `check_and_lock` for event-id dedup + phantom-decrypt prevention,
/// then writes each sub-field to the corresponding signal with side effects
/// (theme apply, Blossom servers, etc.).
async fn decrypt_and_apply(event: Event, source: BlobSource) -> Result<UserPrefsBlob, String> {
    let event_id = event.id;
    let guard = check_and_lock(
        event_id,
        source.clone(),
        &USER_PREFS_EVENT_ID,
        &LAST_PUBLISHED_EVENT_ID,
    )
    .await
    .ok_or_else(|| "Skipped (dedup or phantom-self)".to_string())?;

    // Decrypt: try NIP-44 via signer, fall back to plaintext (legacy).
    let blob: UserPrefsBlob =
        match encrypt::decrypt_from_self_signer(&event.content, event_id).await {
            Ok(b) => b,
            Err(e) => {
                return Err(e);
            }
        };

    // Apply each sub-field to existing GlobalSignals.
    apply_blob_to_signals(&blob, source);

    mark_applied(guard, &USER_PREFS_EVENT_ID, &USER_PREFS_STATE);
    Ok(blob)
}

/// Apply the unified blob to existing per-store GlobalSignals.
///
/// This is the "wiring" that connects the unified blob to the UI. Each
/// sub-field writes to the corresponding signal + handles side effects
/// (theme apply, cache write, etc.).
pub fn apply_blob_to_signals(blob: &UserPrefsBlob, source: BlobSource) {
    // Settings + side effects.
    let settings = blob.settings.clone();
    let theme = match settings.theme.as_str() {
        "light" => crate::stores::theme_store::Theme::Light,
        "dark" => crate::stores::theme_store::Theme::Dark,
        _ => crate::stores::theme_store::Theme::System,
    };
    crate::stores::theme_store::set_theme_internal(theme);
    if !settings.blossom_servers.is_empty() {
        use crate::stores::blossom_store::BlossomServersStoreStoreExt;
        *crate::stores::blossom_store::BLOSSOM_SERVERS
            .read()
            .data()
            .write() = settings.blossom_servers.clone();
    }
    let _ = crate::platform::storage::set("nostr_blue_settings", &settings);
    *crate::stores::settings_store::SETTINGS.write() = settings;

    // Sidebar.
    let sidebar = blob.sidebar.clone();
    let sidebar_migrated = sidebar.migrate_to_v2();
    *crate::stores::sidebar_store::SIDEBAR_ITEMS.write() = sidebar_migrated.items_order;
    *crate::stores::sidebar_store::SIDEBAR_SLOT_COUNT.write() = sidebar_migrated.items_per_page;

    // Reactions.
    if !blob.reactions.is_empty() {
        *crate::stores::reactions_store::PREFERRED_REACTIONS.write() = blob.reactions.clone();
    }

    // Notifications checked_at — take max.
    {
        let local = *crate::stores::ui::notifications::NOTIFICATIONS_CHECKED_AT.read();
        let remote = blob.notifications_checked_at as i64;
        if remote > local {
            *crate::stores::ui::notifications::NOTIFICATIONS_CHECKED_AT.write() = remote;
        }
    }

    // Terms.
    if let Some(_version) = blob.cashu_terms_accepted {
        #[cfg(feature = "cashu")]
        {
            *crate::stores::cashu::TERMS_ACCEPTED.write() = Some(_version >= 1);
        }
    }
    if let Some(version) = blob.p2p_terms_accepted {
        *crate::stores::mostro::nip78::P2P_TERMS_ACCEPTED.write() = Some(version >= 1);
        *crate::stores::mostro::nip78::P2P_TERMS_VERSION_ACCEPTED.write() = Some(version);
    }

    // Mostro mnemonic — restore from the (main-signer-encrypted) backup when
    // localStorage lacks it or differs. This is the cross-device /
    // cleared-storage recovery path: the main blob rehydrates the Mostro
    // identity, after which the Mostro blob + daemon restore can proceed
    // with the correct identity. `restore_mnemonic` preserves the existing
    // trade index (the daemon's LastTradeIndex syncs it during restore).
    if let Some(ref remote_mnemonic) = blob.mostro_mnemonic {
        let local_mnemonic = crate::stores::mostro::keys::stored_mnemonic();
        if local_mnemonic.as_deref() != Some(remote_mnemonic.as_str()) {
            if let Err(e) = crate::stores::mostro::keys::restore_mnemonic(remote_mnemonic) {
                log::warn!("apply_blob_to_signals: failed to restore Mostro mnemonic: {e}");
            }
        }
    }

    // AI credentials — handled by the legacy ai_provider_store loader
    // during Phase 1 dual-read. The unified blob's ai_credentials will be
    // applied directly in Phase 4 once the legacy loader is removed.

    log::debug!(
        "apply_blob_to_signals: applied from {:?} (settings.theme={}, {} reactions, checked_at={})",
        source,
        blob.settings.theme,
        blob.reactions.len(),
        blob.notifications_checked_at
    );
}

// ─── Mostro blob load ───────────────────────────────────────────────────

/// Load the unified Mostro preferences blob from relays.
///
/// Encrypted with the Mostro identity key (sync NIP-44 via
/// `private_app_data`), not the main signer.
pub async fn load_mostro_prefs() -> Result<Option<MostroPrefsBlob>, String> {
    if !crate::stores::auth_store::is_authenticated() {
        return Ok(None);
    }
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return Ok(None),
    };
    let pubkey = match crate::stores::nostr_client::get_cached_pubkey() {
        Ok(pk) => pk,
        Err(_) => return Ok(None),
    };

    // Cache.
    let cache_key = format!("{MOSTRO_PREFS_CACHE_PREFIX}{pubkey}");
    if let Ok(cached_json) = crate::platform::storage::get::<String>(&cache_key) {
        if let Ok(blob) = serde_json::from_str::<MostroPrefsBlob>(&cached_json) {
            log::debug!("load_mostro_prefs: applying cached blob");
            apply_mostro_blob_to_signals(&blob);
        }
    }

    // Gate.
    *MOSTRO_PREFS_LOAD_STATE.write() = Nip78LoadState::Loading;
    wait_for_user_relays(
        Duration::from_secs(5),
        "user_prefs::load_mostro_prefs",
    )
    .await;

    // Fetch.
    let filter = nostr::Filter::new()
        .author(pubkey)
        .kind(nostr::Kind::from(30078))
        .identifier(MOSTRO_PREFS_D_TAG)
        .limit(1);

    let event =
        match fetch::fetch_newest_with_quorum(&client, filter, Duration::from_secs(10)).await {
            Ok(Some(e)) => e,
            Ok(None) => {
                *MOSTRO_PREFS_LOAD_STATE.write() = if !*USER_RELAYS_APPLIED.peek() {
                    Nip78LoadState::Failed("User relays not applied, retry needed".into())
                } else {
                    Nip78LoadState::LoadedDefaults
                };
                return Ok(None);
            }
            Err(e) => {
                mark_failed(e, &MOSTRO_PREFS_LOAD_STATE);
                return Ok(None);
            }
        };

    // Decrypt + apply.
    let event_id = event.id;
    let guard = check_and_lock(
        event_id,
        BlobSource::Relay,
        &MOSTRO_PREFS_EVENT_ID,
        &LAST_PUBLISHED_MOSTRO_EVENT_ID,
    )
    .await
    .ok_or_else(|| "Skipped (dedup or phantom-self)".to_string())?;

    let blob: MostroPrefsBlob = encrypt::decrypt_from_self_mostro(&event.content)
        .map_err(|e| format!("Mostro blob decrypt: {e}"))?;

    apply_mostro_blob_to_signals(&blob);

    // Cache.
    if let Ok(json) = serde_json::to_string(&blob) {
        let _ = crate::platform::storage::set(&cache_key, &json);
    }

    mark_applied(guard, &MOSTRO_PREFS_EVENT_ID, &MOSTRO_PREFS_LOAD_STATE);
    Ok(Some(blob))
}

/// Apply the Mostro blob to existing per-store GlobalSignals.
pub fn apply_mostro_blob_to_signals(blob: &MostroPrefsBlob) {
    // Settings.
    let settings = blob.settings.clone();
    let _ = crate::platform::storage::set_string(
        "nostr_blue_p2p_settings",
        &serde_json::to_string(&settings).unwrap_or_default(),
    );
    *crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write() = settings;

    // Node config.
    if let Some(ref cfg) = blob.node_config {
        *crate::stores::mostro::node_config::MOSTRO_NODE_CONFIG.write() = Some(cfg.clone());
    }

    // Trades — merge with any existing local trades.
    if !blob.recent_trades.is_empty() {
        let local = crate::stores::mostro::trade_store::TRADES.read().clone();
        let merged = MostroPrefsBlob::merge_trades(&local, &blob.recent_trades);
        *crate::stores::mostro::trade_store::TRADES.write() = merged;
    }

    // Creation ledger — merge remote entries into the local signal (union by
    // (trade_index, role) / (order_id, role), keeping the newer). This
    // restores the durable "orders I own" handle across devices.
    if !blob.creation_ledger.is_empty() {
        let mut local =
            crate::stores::mostro::creation_ledger::CREATION_LEDGER.read().clone();
        for entry in &blob.creation_ledger {
            let role = entry.role;
            let idx = entry.trade_index;
            if let Some(existing) = local.iter_mut().find(|e| {
                (idx.is_some() && e.trade_index == idx && e.role == role)
                    || (e.order_id == entry.order_id && e.role == role)
            }) {
                // Keep the confirmed/UUID'd version if either side has it.
                if entry.confirmed && !existing.confirmed {
                    *existing = entry.clone();
                }
            } else {
                local.push(entry.clone());
            }
        }
        // Re-sort newest-first + bound (mirror append() invariants).
        local.sort_by_key(|e| std::cmp::Reverse(e.created_at));
        local.truncate(200);
        *crate::stores::mostro::creation_ledger::CREATION_LEDGER.write() = local;
    }

    log::debug!(
        "apply_mostro_blob_to_signals: {} trades, node_config={}, ledger={}",
        blob.recent_trades.len(),
        blob.node_config.is_some(),
        blob.creation_ledger.len()
    );
}
