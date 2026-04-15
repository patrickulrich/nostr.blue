//! Wallet initialization
//!
//! Functions for initializing the wallet, checking/accepting terms,
//! and creating new wallets.
use super::events::{
    fetch_tokens, publish_signed_event, sign_event_builder_with_signer,
    start_pending_events_processor,
};
use super::history::fetch_history;
use super::internal::{init_multi_mint_wallet, inject_nip60_proofs_to_cdk};
use super::recovery::{recover_pending_operations, sync_state_with_all_mints};
use super::signals::{
    PENDING_NOSTR_EVENTS, TERMS_ACCEPTED, TERMS_D_TAG, WALLET_STATE, WALLET_STATUS,
};
use super::types::PendingEventType;
use super::types::{WalletState, WalletStatus};
use super::utils::normalize_mint_url;
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{Event, EventBuilder, Filter, Kind, PublicKey, SecretKey, Tag, Url};
use std::time::Duration;
struct WalletEvent {
    privkey: String,
    mints: Vec<Url>,
}
impl WalletEvent {
    fn new(privkey: String, mints: Vec<Url>) -> Self {
        Self { privkey, mints }
    }
}

async fn initialize_wallet_source(event: &Event, source: &str) -> Result<(), String> {
    initialize_wallet_from_event(event)
        .await
        .map_err(|error| format!("Failed to initialize {source} wallet snapshot: {error}"))
}

fn latest_pending_wallet_snapshot_event() -> Option<Event> {
    PENDING_NOSTR_EVENTS
        .read()
        .iter()
        .filter(|event| event.event_type == PendingEventType::WalletSnapshot)
        .filter_map(
            |event| match serde_json::from_str::<Event>(&event.builder_json) {
                Ok(parsed) => Some(parsed),
                Err(error) => {
                    log::warn!(
                        "Failed to deserialize queued wallet snapshot {}: {}",
                        event.id,
                        error
                    );
                    None
                }
            },
        )
        .max_by(|left, right| left.created_at.cmp(&right.created_at))
}

async fn initialize_wallet_from_event(wallet_event: &Event) -> Result<(), String> {
    let wallet_data = decrypt_wallet_event(wallet_event).await?;
    log::info!("Wallet loaded with {} mints", wallet_data.mints.len());
    *WALLET_STATE.write() = Some(WalletState {
        privkey: Some(wallet_data.privkey.clone()),
        mints: wallet_data
            .mints
            .iter()
            .map(|u| normalize_mint_url(u.as_ref()))
            .collect(),
        initialized: true,
    });
    if let Err(e) = init_multi_mint_wallet(&wallet_data.mints).await {
        log::error!("Failed to initialize MultiMintWallet: {}", e);
    }
    if let Err(e) = fetch_tokens().await {
        log::error!("Failed to fetch tokens: {}", e);
    }
    if let Err(e) = inject_nip60_proofs_to_cdk().await {
        log::warn!("Failed to inject NIP-60 proofs to CDK: {}", e);
    }
    if let Err(e) = fetch_history().await {
        log::error!("Failed to fetch history: {}", e);
    }
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state: {}", e);
    }
    super::signals::load_pending_secrets().await;
    super::signals::load_in_flight_melt_requests().await;
    start_pending_events_processor();
    *WALLET_STATUS.write() = WalletStatus::Recovering;
    spawn(async move {
        crate::platform::timer::sleep_ms(500).await;
        super::signals::cleanup_expired_pending_secrets().await;
        match super::recovery::sync_orphaned_cdk_proofs_to_nostr().await {
            Ok(result) => {
                if result.proofs_recovered > 0 {
                    log::info!(
                        "Orphan sync: recovered {} proofs ({} sats) from CDK to NIP-60",
                        result.proofs_recovered,
                        result.sats_recovered
                    );
                }
                if !result.errors.is_empty() {
                    for err in result.errors {
                        log::warn!("Orphan sync error: {}", err);
                    }
                }
            }
            Err(e) => {
                log::warn!("Orphan sync failed: {}", e);
            }
        }
        log::info!("Starting wallet recovery - syncing with mints...");
        if let Err(e) = sync_state_with_all_mints().await {
            log::warn!("Mint sync during recovery failed: {}", e);
        }
        if let Err(e) = recover_pending_operations().await {
            log::warn!("Pending operation recovery failed: {}", e);
        }
        match super::recovery::recover_all_pending_melt_quotes().await {
            Ok(result) => {
                if result.quotes_paid > 0 || result.change_recovered > 0 {
                    log::info!(
                        "In-flight melt recovery: {} paid, {} sats recovered",
                        result.quotes_paid,
                        result.change_recovered
                    );
                }
                if !result.errors.is_empty() {
                    for err in result.errors {
                        log::warn!("In-flight melt recovery error: {}", err);
                    }
                }
            }
            Err(e) => {
                log::warn!("In-flight melt quote recovery failed: {}", e);
            }
        }
        let result = super::proof_recovery::run_full_recovery().await;
        if result.recovered_count > 0 || result.spent_count > 0 {
            log::info!(
                "Proof recovery: {} recovered ({} sats), {} spent ({} sats)",
                result.recovered_count,
                result.recovered_value,
                result.spent_count,
                result.spent_value
            );
        }
        if !result.errors.is_empty() {
            for err in &result.errors {
                log::warn!("Proof recovery error: {}", err);
            }
        }
        if let Some(multi_wallet) = cashu_cdk_bridge::MULTI_WALLET.read().as_ref() {
            match multi_wallet.check_all_mint_quotes(None).await {
                Ok(amount) => {
                    if u64::from(amount) > 0 {
                        log::info!("Recovered {} sats from paid mint quotes", u64::from(amount));
                        let _ = cashu_cdk_bridge::sync_wallet_state().await;
                    }
                }
                Err(e) => {
                    log::warn!("Mint quote recovery failed: {}", e);
                }
            }
        }
        super::proof_recovery::recalculate_balance();
        log::debug!("Final balance recalculation complete");
        log::info!("Wallet recovery complete");
        *WALLET_STATUS.write() = WalletStatus::Ready;
    });
    Ok(())
}
/// Check if user has accepted Cashu wallet terms (NIP-78)
/// Returns true if the terms agreement event exists, false otherwise
pub async fn check_terms_accepted() -> Result<bool, String> {
    log::info!("Checking Cashu wallet terms acceptance (NIP-78)...");
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(30078))
        .identifier(TERMS_D_TAG)
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            let accepted = !events.is_empty();
            log::info!(
                "Terms acceptance check: {}",
                if accepted { "accepted" } else { "not accepted" }
            );
            *TERMS_ACCEPTED.write() = Some(accepted);
            Ok(accepted)
        }
        Err(e) => {
            log::warn!("Failed to check terms acceptance: {}", e);
            Err(format!("Failed to check terms: {}", e))
        }
    }
}
/// Accept Cashu wallet terms by publishing a NIP-78 event
pub async fn accept_terms() -> Result<(), String> {
    log::info!("Accepting Cashu wallet terms (NIP-78)...");
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    nostr_client::ensure_relays_ready(&client).await;
    let now = crate::platform::timestamp::now_secs();
    let content = serde_json::json!({ "accepted_at" : now, "version" : 1 }).to_string();
    let builder = EventBuilder::new(Kind::from(30078), content).tag(Tag::identifier(TERMS_D_TAG));
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish terms acceptance: {}", e))?;
    if output.success.is_empty() {
        return Err("Failed to publish terms acceptance: no relay accepted".to_string());
    }
    log::info!("Terms acceptance published successfully");
    *TERMS_ACCEPTED.write() = Some(true);
    Ok(())
}
/// Initialize wallet by fetching from relays
pub async fn init_wallet() -> Result<(), String> {
    {
        let mut status = WALLET_STATUS.write();
        if matches!(
            *status,
            WalletStatus::Loading | WalletStatus::Ready | WalletStatus::Recovering
        ) {
            log::debug!("Wallet init skipped - already {:?}", *status);
            return Ok(());
        }
        *status = WalletStatus::Loading;
    }
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    log::info!("Loading Cashu wallet for {}", pubkey_str);
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(17375))
        .limit(1);
    if let Err(e) = load_pending_events().await {
        log::warn!("Failed to load pending events: {}", e);
    }
    let pending_wallet_event = latest_pending_wallet_snapshot_event();
    nostr_client::ensure_relays_ready(&client).await;
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let relay_wallet_event = events.into_iter().next();
            match (pending_wallet_event.as_ref(), relay_wallet_event.as_ref()) {
                (Some(pending), Some(relay)) => {
                    let (primary, primary_label, fallback, fallback_label) =
                        if pending.created_at >= relay.created_at {
                            (pending, "queued", relay, "relay")
                        } else {
                            (relay, "relay", pending, "queued")
                        };
                    match initialize_wallet_source(primary, primary_label).await {
                        Ok(()) => Ok(()),
                        Err(primary_error) => {
                            log::warn!("{}", primary_error);
                            if let Err(fallback_error) =
                                initialize_wallet_source(fallback, fallback_label).await
                            {
                                log::error!("{}", fallback_error);
                                *WALLET_STATUS.write() =
                                    WalletStatus::Error(fallback_error.clone());
                                Err(fallback_error)
                            } else {
                                Ok(())
                            }
                        }
                    }
                }
                (Some(pending), None) => {
                    if let Err(error) = initialize_wallet_source(pending, "queued").await {
                        log::error!("{}", error);
                        *WALLET_STATUS.write() = WalletStatus::Error(error.clone());
                        Err(error)
                    } else {
                        Ok(())
                    }
                }
                (None, Some(relay)) => {
                    if let Err(error) = initialize_wallet_source(relay, "relay").await {
                        log::error!("{}", error);
                        *WALLET_STATUS.write() = WalletStatus::Error(error.clone());
                        Err(error)
                    } else {
                        Ok(())
                    }
                }
                (None, None) => {
                    log::info!("No wallet found");
                    *WALLET_STATE.write() = Some(WalletState {
                        privkey: None,
                        mints: Vec::new(),
                        initialized: false,
                    });
                    *WALLET_STATUS.write() = WalletStatus::Ready;
                    Ok(())
                }
            }
        }
        Err(e) => {
            if let Some(wallet_event) = pending_wallet_event {
                log::warn!(
                    "Failed to fetch wallet from relays, using queued wallet snapshot: {}",
                    e
                );
                if let Err(error) = initialize_wallet_source(&wallet_event, "queued").await {
                    log::error!("{}", error);
                    *WALLET_STATUS.write() = WalletStatus::Error(error.clone());
                    Err(error)
                } else {
                    Ok(())
                }
            } else {
                let error = format!("Failed to fetch wallet: {}", e);
                log::error!("{}", error);
                *WALLET_STATUS.write() = WalletStatus::Error(error.clone());
                Err(error)
            }
        }
    }
}
/// Create a new wallet with generated P2PK key
pub async fn create_wallet(mints: Vec<String>) -> Result<(), String> {
    if is_wallet_initialized() {
        return Err("Wallet already exists. Cannot overwrite existing wallet.".to_string());
    }
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let signer_type = crate::stores::signer::get_signer().ok_or("No signer available")?;
    let signer = signer_type.as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let wallet_secret = SecretKey::generate();
    let wallet_privkey = wallet_secret.to_secret_hex();
    log::info!("Creating new wallet with {} mints", mints.len());
    let mut mint_urls = Vec::new();
    let mut invalid_mints = Vec::new();
    for m in &mints {
        match Url::parse(m) {
            Ok(url) => mint_urls.push(url),
            Err(e) => invalid_mints.push(format!("{}: {}", m, e)),
        }
    }
    if !invalid_mints.is_empty() {
        return Err(format!("Invalid mint URLs: {}", invalid_mints.join(", ")));
    }
    let wallet_event = WalletEvent::new(wallet_privkey.clone(), mint_urls);
    let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", &wallet_event.privkey]];
    for mint in wallet_event.mints.iter() {
        content_array.push(vec!["mint", mint.as_str()]);
    }
    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;
    let encrypted_content = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt wallet data: {}", e))?;
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWallet, encrypted_content);
    let event = sign_event_builder_with_signer(builder, signer_type).await?;
    match publish_signed_event(&client, &event).await {
        Ok(output) if !output.success.is_empty() => {
            log::info!("Wallet created successfully");
            *WALLET_STATE.write() = Some(WalletState {
                privkey: Some(wallet_privkey),
                mints: mints.clone(),
                initialized: true,
            });
            *WALLET_STATUS.write() = WalletStatus::Ready;
            Ok(())
        }
        Ok(_) => {
            let error = "Failed to create wallet: no relay accepted the event".to_string();
            log::error!("{}", error);
            Err(error)
        }
        Err(e) => {
            let error = format!("Failed to create wallet: {}", e);
            log::error!("{}", error);
            Err(error)
        }
    }
}
/// Check if wallet is initialized
pub fn is_wallet_initialized() -> bool {
    WALLET_STATE
        .read()
        .as_ref()
        .map(|w| w.initialized)
        .unwrap_or(false)
}
/// Decrypt wallet event (kind 17375)
///
/// Parses the NIP-60 wallet event format: `[["privkey", "hex"], ["mint", "url"], ...]`
async fn decrypt_wallet_event(event: &Event) -> Result<WalletEvent, String> {
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let decrypted = super::internal::nip44_decrypt_cached(
        signer,
        event.id,
        event.pubkey,
        event.content.clone(),
    )
    .await
    .map_err(|e| format!("Failed to decrypt wallet event: {}", e))?;
    let pairs: Vec<Vec<String>> = serde_json::from_str(&decrypted)
        .map_err(|e| format!("Failed to parse wallet JSON: {}", e))?;
    let mut privkey = String::new();
    let mut mints = Vec::new();
    let mut found_multiple_privkeys = false;
    for pair in pairs {
        if pair.len() != 2 {
            log::warn!(
                "Skipping malformed wallet event entry with {} elements",
                pair.len()
            );
            continue;
        }
        match pair[0].as_str() {
            "privkey" => {
                if !privkey.is_empty() {
                    found_multiple_privkeys = true;
                } else {
                    privkey = pair[1].clone();
                }
            }
            "mint" => match Url::parse(&pair[1]) {
                Ok(mint_url) => mints.push(mint_url),
                Err(e) => {
                    log::warn!("Skipping invalid mint URL '{}': {}", pair[1], e);
                }
            },
            _ => {}
        }
    }
    if found_multiple_privkeys {
        return Err("Wallet event contains multiple privkeys (invalid per NIP-60)".to_string());
    }
    if privkey.is_empty() {
        return Err("Missing required field: privkey".to_string());
    }
    if mints.is_empty() {
        return Err("Missing required field: mint (at least one mint URL required)".to_string());
    }
    Ok(WalletEvent::new(privkey, mints))
}
/// Load pending events from IndexedDB on startup
async fn load_pending_events() -> Result<(), String> {
    use super::signals::{PENDING_NOSTR_EVENTS, SHARED_LOCALSTORE};
    log::info!("Loading pending events from IndexedDB...");
    let localstore = SHARED_LOCALSTORE
        .read()
        .as_ref()
        .ok_or("Localstore not initialized")?
        .clone();
    let pending_events = localstore
        .get_all_pending_events()
        .await
        .map_err(|e| format!("Failed to load pending events: {}", e))?;
    if pending_events.is_empty() {
        log::debug!("No pending events found in IndexedDB");
        return Ok(());
    }
    log::info!(
        "Loaded {} pending events from IndexedDB",
        pending_events.len()
    );
    let mut events = PENDING_NOSTR_EVENTS.write();
    for event in pending_events {
        if !events.iter().any(|e| e.id == event.id) {
            events.push(event);
        }
    }
    log::info!("Pending events loaded and ready for retry processing");
    Ok(())
}
