//! Wallet initialization
//!
//! Functions for initializing the wallet, checking/accepting terms,
//! and creating new wallets.

use std::time::Duration;

use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{Event, EventBuilder, Filter, Kind, PublicKey, SecretKey, Tag, Url};

use super::events::{fetch_tokens, start_pending_events_processor};
use super::history::fetch_history;
use super::internal::init_multi_mint_wallet;
use super::recovery::{recover_pending_operations, sync_state_with_all_mints};
use super::signals::{TERMS_ACCEPTED, TERMS_D_TAG, WALLET_STATE, WALLET_STATUS};
use super::types::{WalletState, WalletStatus};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};

// NIP-60 Wallet event structure
struct WalletEvent {
    privkey: String,
    mints: Vec<Url>,
}

impl WalletEvent {
    fn new(privkey: String, mints: Vec<Url>) -> Self {
        Self { privkey, mints }
    }
}

// =============================================================================
// Terms Acceptance (NIP-78)
// =============================================================================

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

    // Build filter for terms agreement event
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(30078))
        .identifier(TERMS_D_TAG)
        .limit(1);

    // Ensure relays are ready
    nostr_client::ensure_relays_ready(&client).await;

    // Fetch terms agreement event
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
            *TERMS_ACCEPTED.write() = Some(false);
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

    // Ensure relays are ready before publishing
    nostr_client::ensure_relays_ready(&client).await;

    // Create content with timestamp and version
    let now = (js_sys::Date::now() / 1000.0) as u64;
    let content = serde_json::json!({
        "accepted_at": now,
        "version": 1
    })
    .to_string();

    // Build NIP-78 event (kind 30078 with d-tag)
    let builder = EventBuilder::new(Kind::from(30078), content).tag(Tag::identifier(TERMS_D_TAG));

    // Publish to relays
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish terms acceptance: {}", e))?;

    log::info!("Terms acceptance published successfully");

    // Update signal
    *TERMS_ACCEPTED.write() = Some(true);

    Ok(())
}

// =============================================================================
// Wallet Initialization
// =============================================================================

/// Initialize wallet by fetching from relays
pub async fn init_wallet() -> Result<(), String> {
    // Guard against concurrent initialization
    {
        let status = WALLET_STATUS.read();
        if matches!(*status, WalletStatus::Loading | WalletStatus::Ready) {
            log::debug!("Wallet init skipped - already {:?}", *status);
            return Ok(());
        }
    }

    *WALLET_STATUS.write() = WalletStatus::Loading;

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading Cashu wallet for {}", pubkey_str);

    // Fetch wallet event (kind 17375)
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(17375))
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(wallet_event) = events.into_iter().next() {
                // Decrypt and parse wallet event
                match decrypt_wallet_event(&wallet_event).await {
                    Ok(wallet_data) => {
                        log::info!("Wallet loaded with {} mints", wallet_data.mints.len());

                        *WALLET_STATE.write() = Some(WalletState {
                            privkey: Some(wallet_data.privkey.clone()),
                            mints: wallet_data.mints.iter().map(|u| u.to_string()).collect(),
                            initialized: true,
                        });

                        // Initialize MultiMintWallet with all mints
                        if let Err(e) = init_multi_mint_wallet(&wallet_data.mints).await {
                            log::error!("Failed to initialize MultiMintWallet: {}", e);
                            // Continue without MultiMintWallet - fallback to legacy mode
                        }

                        // Fetch tokens and history
                        if let Err(e) = fetch_tokens().await {
                            log::error!("Failed to fetch tokens: {}", e);
                        }

                        if let Err(e) = fetch_history().await {
                            log::error!("Failed to fetch history: {}", e);
                        }

                        // Sync MultiMintWallet state to Dioxus signals
                        if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
                            log::warn!("Failed to sync MultiMintWallet state: {}", e);
                        }

                        // Load pending events queue from IndexedDB
                        if let Err(e) = load_pending_events().await {
                            log::warn!("Failed to load pending events: {}", e);
                        }

                        // Start background processor for pending events
                        start_pending_events_processor();

                        // Set status to Recovering while background sync runs
                        *WALLET_STATUS.write() = WalletStatus::Recovering;

                        // Run recovery in background to not block UI
                        spawn(async move {
                            // Small delay to let UI render
                            #[cfg(target_arch = "wasm32")]
                            gloo_timers::future::TimeoutFuture::new(500).await;

                            // Phase 1: Sync proof states with all mints (NUT-07)
                            // Detects proofs spent elsewhere, proofs pending at mint
                            log::info!("Starting wallet recovery - syncing with mints...");
                            if let Err(e) = sync_state_with_all_mints().await {
                                log::warn!("Mint sync during recovery failed: {}", e);
                            }

                            // Phase 2: Recover pending operations (quotes, transactions)
                            // Completes paid-but-not-minted quotes, recovers change, etc.
                            if let Err(e) = recover_pending_operations().await {
                                log::warn!("Pending operation recovery failed: {}", e);
                            }

                            // Phase 3: Check for paid mint quotes using CDK
                            // This uses CDK's built-in check_all_mint_quotes()
                            if let Some(multi_wallet) = cashu_cdk_bridge::MULTI_WALLET.read().as_ref() {
                                match multi_wallet.check_all_mint_quotes(None).await {
                                    Ok(amount) => {
                                        if u64::from(amount) > 0 {
                                            log::info!("Recovered {} sats from paid mint quotes", u64::from(amount));
                                            // Sync state to update UI
                                            let _ = cashu_cdk_bridge::sync_wallet_state().await;
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("Mint quote recovery failed: {}", e);
                                    }
                                }
                            }

                            log::info!("Wallet recovery complete");
                            *WALLET_STATUS.write() = WalletStatus::Ready;
                        });

                        Ok(())
                    }
                    Err(e) => {
                        let error = format!("Failed to decrypt wallet: {}", e);
                        log::error!("{}", error);
                        *WALLET_STATUS.write() = WalletStatus::Error(error.clone());
                        Err(error)
                    }
                }
            } else {
                log::info!("No wallet found");
                *WALLET_STATE.write() = Some(WalletState {
                    privkey: None, // No wallet exists yet
                    mints: Vec::new(),
                    initialized: false,
                });
                *WALLET_STATUS.write() = WalletStatus::Ready;
                Ok(())
            }
        }
        Err(e) => {
            let error = format!("Failed to fetch wallet: {}", e);
            log::error!("{}", error);
            *WALLET_STATUS.write() = WalletStatus::Error(error.clone());
            Err(error)
        }
    }
}

/// Create a new wallet with generated P2PK key
pub async fn create_wallet(mints: Vec<String>) -> Result<(), String> {
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Generate new private key for P2PK ecash (separate from Nostr key)
    let wallet_secret = SecretKey::generate();
    let wallet_privkey = wallet_secret.to_secret_hex();

    log::info!("Creating new wallet with {} mints", mints.len());

    // Parse mint URLs
    let mint_urls: Vec<Url> = mints.iter().filter_map(|m| Url::parse(m).ok()).collect();

    // Build wallet data following rust-nostr's internal format
    let wallet_event = WalletEvent::new(wallet_privkey.clone(), mint_urls);

    let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", &wallet_event.privkey]];
    for mint in wallet_event.mints.iter() {
        content_array.push(vec!["mint", mint.as_str()]);
    }

    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;

    // Encrypt content using signer
    let encrypted_content = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt wallet data: {}", e))?;

    // Build event using rust-nostr kind constant
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWallet, encrypted_content);

    // Publish wallet event
    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Wallet created successfully");

            // Update local state
            *WALLET_STATE.write() = Some(WalletState {
                privkey: Some(wallet_privkey),
                mints: mints.clone(),
                initialized: true,
            });

            *WALLET_STATUS.write() = WalletStatus::Ready;
            Ok(())
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

// =============================================================================
// Internal Helpers
// =============================================================================

/// Decrypt wallet event (kind 17375)
///
/// Parses the NIP-60 wallet event format: `[["privkey", "hex"], ["mint", "url"], ...]`
async fn decrypt_wallet_event(event: &Event) -> Result<WalletEvent, String> {
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    // Decrypt the content using signer's NIP-44 method
    let decrypted = signer
        .nip44_decrypt(&event.pubkey, &event.content)
        .await
        .map_err(|e| format!("Failed to decrypt wallet event: {}", e))?;

    // Parse the decrypted JSON array
    let pairs: Vec<Vec<String>> =
        serde_json::from_str(&decrypted).map_err(|e| format!("Failed to parse wallet JSON: {}", e))?;

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
            _ => {} // Ignore unknown keys for forward compatibility
        }
    }

    // Validate required fields
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

    // Get the shared localstore
    let localstore = SHARED_LOCALSTORE.read()
        .as_ref()
        .ok_or("Localstore not initialized")?
        .clone();

    // Load all pending events from IndexedDB
    let pending_events = localstore
        .get_all_pending_events()
        .await
        .map_err(|e| format!("Failed to load pending events: {}", e))?;

    if pending_events.is_empty() {
        log::debug!("No pending events found in IndexedDB");
        return Ok(());
    }

    log::info!("Loaded {} pending events from IndexedDB", pending_events.len());

    // Populate the in-memory signal
    let mut events = PENDING_NOSTR_EVENTS.write();
    for event in pending_events {
        // Avoid duplicates (in case signal already has some events)
        if !events.iter().any(|e| e.id == event.id) {
            events.push(event);
        }
    }

    log::info!("Pending events loaded and ready for retry processing");
    Ok(())
}
