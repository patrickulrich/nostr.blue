use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, SecretKey};
use nostr_sdk::nips::nip60::{WalletEvent, CashuProof, TransactionDirection};
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;
use serde::Deserialize;

#[cfg(target_arch = "wasm32")]
use gloo_storage::{LocalStorage, Storage};

/// Custom deserialization structure for token events (more lenient than rust-nostr)
#[derive(Debug, Clone, Deserialize)]
struct TokenEventData {
    pub mint: String,
    pub proofs: Vec<ProofData>,
    #[serde(default)]
    pub del: Vec<String>,
}

/// Custom deserialization structure for proofs (allows missing fields)
#[derive(Debug, Clone, Deserialize)]
struct ProofData {
    #[serde(default)]
    pub id: String,
    pub amount: u64,
    pub secret: String,
    #[serde(default)]
    pub c: String,
}

/// Wallet state containing configuration
#[derive(Clone, Debug, PartialEq)]
pub struct WalletState {
    pub privkey: String,
    pub mints: Vec<String>,
    pub initialized: bool,
}

/// Token data with event metadata
#[derive(Clone, Debug, PartialEq)]
pub struct TokenData {
    pub event_id: String,
    pub mint: String,
    pub unit: String,
    pub proofs: Vec<CashuProof>,
    pub created_at: u64,
}

/// Transaction history item with event metadata
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryItem {
    pub event_id: String,
    pub direction: TransactionDirection,
    pub amount: u64,
    pub unit: String,
    pub created_at: u64,
    pub created_tokens: Vec<String>,
    pub destroyed_tokens: Vec<String>,
    pub redeemed_events: Vec<String>,
}

/// Wallet loading status
#[derive(Clone, Debug, PartialEq)]
pub enum WalletStatus {
    Uninitialized,
    Loading,
    Ready,
    Error(String),
}

/// Global signal for wallet state
pub static WALLET_STATE: GlobalSignal<Option<WalletState>> = Signal::global(|| None);

/// Global signal for tokens
pub static WALLET_TOKENS: GlobalSignal<Vec<TokenData>> = Signal::global(Vec::new);

/// Global signal for transaction history
pub static WALLET_HISTORY: GlobalSignal<Vec<HistoryItem>> = Signal::global(Vec::new);

/// Global signal for total balance
pub static WALLET_BALANCE: GlobalSignal<u64> = Signal::global(|| 0);

/// Global signal for wallet status
pub static WALLET_STATUS: GlobalSignal<WalletStatus> = Signal::global(|| WalletStatus::Uninitialized);

const STORAGE_KEY_WALLET_PRIVKEY: &str = "cashu_wallet_privkey";

/// Initialize wallet by fetching from relays
pub async fn init_wallet() -> Result<(), String> {
    *WALLET_STATUS.write() = WalletStatus::Loading;

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading Cashu wallet for {}", pubkey_str);

    // Fetch wallet event (kind 17375)
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(17375))
        .limit(1);

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(wallet_event) = events.into_iter().next() {
                // Decrypt and parse wallet event
                match decrypt_wallet_event(&wallet_event).await {
                    Ok(wallet_data) => {
                        log::info!("Wallet loaded with {} mints", wallet_data.mints.len());

                        // Store wallet privkey securely in localStorage
                        #[cfg(target_arch = "wasm32")]
                        {
                            if let Err(e) = LocalStorage::set(STORAGE_KEY_WALLET_PRIVKEY, &wallet_data.privkey) {
                                log::error!("Failed to store wallet privkey: {:?}", e);
                            }
                        }

                        *WALLET_STATE.write() = Some(WalletState {
                            privkey: wallet_data.privkey.clone(),
                            mints: wallet_data.mints.iter().map(|u| u.to_string()).collect(),
                            initialized: true,
                        });

                        // Fetch tokens and history
                        if let Err(e) = fetch_tokens().await {
                            log::error!("Failed to fetch tokens: {}", e);
                        }

                        if let Err(e) = fetch_history().await {
                            log::error!("Failed to fetch history: {}", e);
                        }

                        *WALLET_STATUS.write() = WalletStatus::Ready;
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
                    privkey: String::new(),
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

/// Decrypt wallet event (kind 17375)
async fn decrypt_wallet_event(event: &Event) -> Result<WalletEvent, String> {
    use nostr_sdk::signer::NostrSigner;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    // Decrypt the content using signer's NIP-44 method
    let decrypted = signer.nip44_decrypt(&event.pubkey, &event.content).await
        .map_err(|e| format!("Failed to decrypt wallet event: {}", e))?;

    // Parse the decrypted JSON array: [["privkey", "hex"], ["mint", "url"], ...]
    let pairs: Vec<Vec<String>> = serde_json::from_str(&decrypted)
        .map_err(|e| format!("Failed to parse wallet event JSON: {}", e))?;

    let mut privkey = String::new();
    let mut mints = Vec::new();

    for pair in pairs {
        if pair.len() != 2 {
            continue;
        }
        match pair[0].as_str() {
            "privkey" => privkey = pair[1].clone(),
            "mint" => {
                let mint_url = nostr_sdk::Url::parse(&pair[1])
                    .map_err(|e| format!("Invalid mint URL: {}", e))?;
                mints.push(mint_url);
            }
            _ => {} // Ignore unknown keys
        }
    }

    if privkey.is_empty() {
        return Err("No privkey found in wallet event".to_string());
    }

    Ok(WalletEvent::new(privkey, mints))
}

/// Fetch all token events (kind 7375)
pub async fn fetch_tokens() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Fetching token events");

    // Fetch kind-5 deletion events that target kind-7375 token events
    let deletion_filter = Filter::new()
        .author(pubkey.clone())
        .kind(Kind::from(5));

    let mut deleted_event_ids = std::collections::HashSet::new();

    if let Ok(deletion_events) = client.fetch_events(deletion_filter, Duration::from_secs(10)).await {
        for del_event in deletion_events {
            // Check e tags that reference kind-7375 events
            for tag in del_event.tags.iter() {
                let tag_vec = tag.clone().to_vec();
                if tag_vec.len() >= 2 && tag_vec[0] == "e" {
                    deleted_event_ids.insert(tag_vec[1].clone());
                }
            }
        }
        if !deleted_event_ids.is_empty() {
            log::info!("Found {} deleted token events via kind-5", deleted_event_ids.len());
        }
    }

    // Fetch all token events
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(7375));

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            use nostr_sdk::signer::NostrSigner;
            use std::collections::HashSet;

            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();

            // Convert Events to Vec for multiple iterations
            let events: Vec<_> = events.into_iter().collect();

            // First pass: collect all deleted proof secrets from del fields
            let mut deleted_secrets = HashSet::new();

            for event in &events {
                // Skip events that are deleted by kind-5
                if deleted_event_ids.contains(&event.id.to_hex()) {
                    continue;
                }

                // Decrypt and parse to get del field
                if let Ok(decrypted) = signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    if let Ok(token_event) = serde_json::from_str::<TokenEventData>(&decrypted) {
                        // Add all deleted proof identifiers to the set
                        for del_id in &token_event.del {
                            deleted_secrets.insert(del_id.clone());
                        }
                    }
                }
            }

            if !deleted_secrets.is_empty() {
                log::info!("Found {} deleted proof identifiers via del field", deleted_secrets.len());
            }

            // Second pass: collect tokens and filter out deleted proofs
            let mut tokens = Vec::new();
            let mut total_balance = 0u64;

            for event in &events {
                // Skip events deleted by kind-5
                if deleted_event_ids.contains(&event.id.to_hex()) {
                    continue;
                }

                // Decrypt token event using signer
                match signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    Ok(decrypted) => {
                        // Parse JSON: { mint: string, proofs: [...], del?: [...] }
                        match serde_json::from_str::<TokenEventData>(&decrypted) {
                            Ok(token_event) => {
                                // Convert ProofData to CashuProof, filtering out deleted proofs
                                let proofs: Vec<CashuProof> = token_event.proofs.iter()
                                    .filter(|p| {
                                        // Filter out proofs that are in the deletion set
                                        // Check by id, secret, or the combined identifier
                                        let proof_id = if p.id.is_empty() {
                                            format!("{}_{}", p.secret, p.amount)
                                        } else {
                                            p.id.clone()
                                        };
                                        !deleted_secrets.contains(&p.secret)
                                            && !deleted_secrets.contains(&p.id)
                                            && !deleted_secrets.contains(&proof_id)
                                    })
                                    .map(|p| CashuProof {
                                        id: if p.id.is_empty() {
                                            // Generate a placeholder ID if missing
                                            format!("{}_{}", p.secret, p.amount)
                                        } else {
                                            p.id.clone()
                                        },
                                        amount: p.amount,
                                        secret: p.secret.clone(),
                                        c: p.c.clone(),
                                    })
                                    .collect();

                                // Only include tokens with remaining proofs
                                if !proofs.is_empty() {
                                    // Calculate balance from non-deleted proofs
                                    let token_balance: u64 = proofs.iter()
                                        .map(|p| p.amount)
                                        .sum();

                                    total_balance += token_balance;

                                    tokens.push(TokenData {
                                        event_id: event.id.to_hex(),
                                        mint: token_event.mint.clone(),
                                        unit: "sat".to_string(), // TODO: Parse unit from event
                                        proofs,
                                        created_at: event.created_at.as_secs(),
                                    });
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to parse token event {}: {}", event.id, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to decrypt token event {}: {}", event.id, e);
                    }
                }
            }

            log::info!("Loaded {} token events with total balance: {} sats", tokens.len(), total_balance);
            *WALLET_TOKENS.write() = tokens;
            *WALLET_BALANCE.write() = total_balance;
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to fetch token events: {}", e);
            Err(format!("Failed to fetch token events: {}", e))
        }
    }
}

/// Fetch transaction history (kind 7376)
pub async fn fetch_history() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Fetching transaction history");

    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(7376));

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            use nostr_sdk::signer::NostrSigner;

            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();
            let mut history = Vec::new();

            for event in events {
                // Decrypt history event using signer
                match signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    Ok(decrypted) => {
                        // Parse JSON array: [["direction", "in"], ["amount", "100"], ["e", "id", "", "created"], ...]
                        match serde_json::from_str::<Vec<Vec<String>>>(&decrypted) {
                            Ok(pairs) => {
                                let mut direction = TransactionDirection::In;
                                let mut amount = 0u64;
                                let mut created_tokens = Vec::new();
                                let mut destroyed_tokens = Vec::new();

                                for pair in pairs {
                                    if pair.is_empty() {
                                        continue;
                                    }
                                    match pair[0].as_str() {
                                        "direction" => {
                                            if pair.len() > 1 {
                                                direction = if pair[1] == "in" {
                                                    TransactionDirection::In
                                                } else {
                                                    TransactionDirection::Out
                                                };
                                            }
                                        }
                                        "amount" => {
                                            if pair.len() > 1 {
                                                amount = pair[1].parse().unwrap_or(0);
                                            }
                                        }
                                        "e" => {
                                            // Event reference: ["e", "event_id", "", "marker"]
                                            if pair.len() > 3 {
                                                match pair[3].as_str() {
                                                    "created" => created_tokens.push(pair[1].clone()),
                                                    "destroyed" => destroyed_tokens.push(pair[1].clone()),
                                                    _ => {}
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                // Extract redeemed events from unencrypted tags
                                let redeemed_events: Vec<String> = event.tags.iter()
                                    .filter_map(|tag| {
                                        let vec = tag.clone().to_vec();
                                        if vec.len() > 3 && vec[0] == "e" && vec[3] == "redeemed" {
                                            Some(vec[1].clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();

                                history.push(HistoryItem {
                                    event_id: event.id.to_hex(),
                                    direction,
                                    amount,
                                    unit: "sat".to_string(),
                                    created_at: event.created_at.as_secs(),
                                    created_tokens,
                                    destroyed_tokens,
                                    redeemed_events,
                                });
                            }
                            Err(e) => {
                                log::error!("Failed to parse history event {}: {}", event.id, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to decrypt history event {}: {}", event.id, e);
                    }
                }
            }

            // Sort by created_at descending (newest first)
            history.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            log::info!("Loaded {} history items", history.len());
            *WALLET_HISTORY.write() = history;
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to fetch history: {}", e);
            Err(format!("Failed to fetch history: {}", e))
        }
    }
}

/// Create a new wallet with generated P2PK key
pub async fn create_wallet(mints: Vec<String>) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Generate new private key for P2PK ecash (separate from Nostr key)
    let wallet_secret = SecretKey::generate();
    let wallet_privkey = wallet_secret.to_secret_hex();

    log::info!("Creating new wallet with {} mints", mints.len());

    // Build JSON array: [["privkey", "hex"], ["mint", "url"], ...]
    let mut content_array: Vec<Vec<String>> = vec![
        vec!["privkey".to_string(), wallet_privkey.clone()]
    ];

    for mint_url in &mints {
        content_array.push(vec!["mint".to_string(), mint_url.clone()]);
    }

    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;

    // Encrypt content using signer
    let encrypted_content = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt wallet data: {}", e))?;

    // Build event manually
    let builder = nostr_sdk::EventBuilder::new(
        Kind::from(17375),
        encrypted_content
    );

    // Publish wallet event
    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Wallet created successfully");

            // Store wallet privkey in localStorage
            #[cfg(target_arch = "wasm32")]
            {
                if let Err(e) = LocalStorage::set(STORAGE_KEY_WALLET_PRIVKEY, &wallet_privkey) {
                    log::error!("Failed to store wallet privkey: {:?}", e);
                }
            }

            // Update local state
            *WALLET_STATE.write() = Some(WalletState {
                privkey: wallet_privkey,
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
    WALLET_STATE.read()
        .as_ref()
        .map(|w| w.initialized)
        .unwrap_or(false)
}

/// Get total number of mints
pub fn get_mint_count() -> usize {
    WALLET_STATE.read()
        .as_ref()
        .map(|w| w.mints.len())
        .unwrap_or(0)
}

/// Get mints list
pub fn get_mints() -> Vec<String> {
    WALLET_STATE.read()
        .as_ref()
        .map(|w| w.mints.clone())
        .unwrap_or_default()
}

/// Refresh wallet data from relays
pub async fn refresh_wallet() -> Result<(), String> {
    if !is_wallet_initialized() {
        return Err("Wallet not initialized".to_string());
    }

    log::info!("Refreshing wallet data");

    fetch_tokens().await?;
    fetch_history().await?;

    Ok(())
}
