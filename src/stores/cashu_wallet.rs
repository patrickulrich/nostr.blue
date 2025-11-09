use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, SecretKey};
use nostr_sdk::nips::nip60::{WalletEvent, CashuProof, TransactionDirection};
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;
use serde::{Deserialize, Serialize};

// CDK database trait for calling methods on IndexedDbDatabase
use cdk::cdk_database::WalletDatabase;

/// Custom deserialization structure for token events (more lenient than rust-nostr)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenEventData {
    pub mint: String,
    pub proofs: Vec<ProofData>,
    #[serde(default)]
    pub del: Vec<String>,
}

/// Custom deserialization structure for proofs (allows missing fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// Removed: STORAGE_KEY_WALLET_PRIVKEY - wallet privkey is now derived deterministically
// and no longer stored in plaintext in LocalStorage

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

                        // Note: wallet privkey is no longer stored in plaintext LocalStorage
                        // It is derived deterministically from the user's Nostr key when needed

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
                                    // Calculate balance from non-deleted proofs using checked arithmetic
                                    let token_balance: u64 = proofs.iter()
                                        .map(|p| p.amount)
                                        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
                                        .ok_or_else(|| format!(
                                            "Proof amount overflow in token event {}",
                                            event.id.to_hex()
                                        ))?;

                                    // Use checked addition to prevent silent overflow
                                    total_balance = total_balance.checked_add(token_balance)
                                        .ok_or_else(|| format!(
                                            "Balance overflow when adding token event {} (balance: {}, adding: {})",
                                            event.id.to_hex(), total_balance, token_balance
                                        ))?;

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
                                let mut amount: Option<u64> = None;
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
                                                match pair[1].parse::<u64>() {
                                                    Ok(parsed_amount) => {
                                                        amount = Some(parsed_amount);
                                                    }
                                                    Err(e) => {
                                                        log::error!(
                                                            "Failed to parse amount in history event {}: '{}' - {}",
                                                            event.id.to_hex(),
                                                            pair[1],
                                                            e
                                                        );
                                                        // Keep amount as None to skip this event
                                                    }
                                                }
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

                                // Only add to history if amount was successfully parsed
                                if let Some(parsed_amount) = amount {
                                    history.push(HistoryItem {
                                        event_id: event.id.to_hex(),
                                        direction,
                                        amount: parsed_amount,
                                        unit: "sat".to_string(),
                                        created_at: event.created_at.as_secs(),
                                        created_tokens,
                                        destroyed_tokens,
                                        redeemed_events,
                                    });
                                } else {
                                    log::warn!(
                                        "Skipping history event {} due to missing or invalid amount",
                                        event.id.to_hex()
                                    );
                                }
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

            // Note: wallet privkey is no longer stored in plaintext LocalStorage
            // It is derived deterministically from the user's Nostr key when needed

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
#[allow(dead_code)]
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

// ============================================================================
// Phase 2: Send/Receive Utility Functions
// ============================================================================

/// Derive deterministic wallet seed from Nostr private key or signer
#[cfg(target_arch = "wasm32")]
async fn derive_wallet_seed() -> Result<[u8; 64], String> {
    use sha2::{Sha256, Digest};
    use gloo_storage::{LocalStorage, Storage};

    // Try to get Keys first (for nsec login)
    if let Some(keys) = auth_store::get_keys() {
        log::info!("Deriving seed from private key (nsec login)");
        let secret_key = keys.secret_key();

        // Derive seed using SHA-256 with domain separation
        let mut hasher = Sha256::new();
        hasher.update(secret_key.to_secret_bytes());
        hasher.update(b"cashu-wallet-seed-v1");
        let hash = hasher.finalize();

        let mut seed = [0u8; 64];
        seed[..32].copy_from_slice(&hash);

        // Second round for full 64 bytes
        let mut hasher = Sha256::new();
        hasher.update(&hash);
        hasher.update(b"cashu-wallet-seed-v1-ext");
        let hash2 = hasher.finalize();
        seed[32..].copy_from_slice(&hash2);

        return Ok(seed);
    }

    // For browser extension or remote signer, use a stored seed
    // TODO: SECURITY WARNING - For extension users, the seed is still stored in plaintext
    // in LocalStorage because we don't have access to their private key for encryption.
    // This is an XSS risk. Consider alternatives:
    // 1. Store encrypted seed in extension storage (requires extension API)
    // 2. Require wallet re-import each session for extension users
    // 3. Use WebCrypto API with a user-provided password
    log::warn!("Using browser extension or remote signer - seed will be stored in plaintext LocalStorage (XSS risk)");

    let pubkey = auth_store::get_pubkey()
        .ok_or("Not authenticated - no pubkey available")?;

    let storage_key = format!("cashu_seed_{}", pubkey);

    // Try to get existing seed
    if let Ok(seed_hex) = LocalStorage::get::<String>(&storage_key) {
        log::info!("Found existing seed in storage");
        let seed_bytes = hex::decode(&seed_hex)
            .map_err(|e| format!("Failed to decode stored seed: {}", e))?;

        if seed_bytes.len() == 64 {
            let mut seed = [0u8; 64];
            seed.copy_from_slice(&seed_bytes);
            return Ok(seed);
        }
    }

    // Generate new seed and store it
    log::warn!("Generating new seed for browser extension user - will be stored in plaintext");
    let mut seed = [0u8; 64];

    #[cfg(target_arch = "wasm32")]
    {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut seed);
    }

    // Store the seed (plaintext - SECURITY RISK for extension users)
    let seed_hex = hex::encode(&seed);
    LocalStorage::set(&storage_key, seed_hex)
        .map_err(|e| format!("Failed to store seed: {:?}", e))?;

    log::warn!("Generated and stored new seed in plaintext LocalStorage");
    Ok(seed)
}

#[cfg(not(target_arch = "wasm32"))]
async fn derive_wallet_seed() -> Result<[u8; 64], String> {
    Err("Seed derivation only available in WASM".to_string())
}

/// Convert ProofData (our custom type) to CDK Proof
fn proof_data_to_cdk_proof(data: &ProofData) -> Result<cdk::nuts::Proof, String> {
    use cdk::nuts::{Proof, Id, PublicKey};
    use cdk::Amount;
    use cdk::secret::Secret;
    use std::str::FromStr;

    Ok(Proof {
        amount: Amount::from(data.amount),
        keyset_id: Id::from_str(&data.id)
            .map_err(|e| format!("Invalid keyset ID '{}': {}", data.id, e))?,
        secret: Secret::from_str(&data.secret)
            .map_err(|e| format!("Invalid secret: {}", e))?,
        c: PublicKey::from_hex(&data.c)
            .map_err(|e| format!("Invalid C value: {}", e))?,
        witness: None,
        dleq: None,
    })
}

/// Convert CDK Proof to ProofData (our custom type)
fn cdk_proof_to_proof_data(proof: &cdk::nuts::Proof) -> ProofData {
    ProofData {
        id: proof.keyset_id.to_string(),
        amount: u64::from(proof.amount),
        secret: proof.secret.to_string(),
        c: proof.c.to_hex(),
    }
}

/// Remove a melt quote from the database without creating a full wallet
async fn remove_melt_quote_from_db(quote_id: &str) -> Result<(), String> {
    use std::sync::Arc;
    use cdk::cdk_database::WalletDatabase;

    // Create localstore directly without full wallet initialization
    let localstore = Arc::new(
        crate::stores::indexeddb_database::IndexedDbDatabase::new()
            .await
            .map_err(|e| format!("Failed to create IndexedDB for cleanup: {}", e))?
    );

    localstore.remove_melt_quote(quote_id).await
        .map_err(|e| format!("Failed to remove melt quote: {}", e))?;

    log::info!("Successfully removed melt quote {} from database", quote_id);
    Ok(())
}

/// Create ephemeral CDK wallet with injected proofs
///
/// Note on atomicity: Each ephemeral wallet creates its own connection to the shared
/// IndexedDB database. The increment_keyset_counter method uses IndexedDB transactions
/// to ensure atomic read-modify-write operations, so concurrent wallet operations
/// will not produce duplicate blinded messages even if multiple ephemeral wallets
/// are active simultaneously.
async fn create_ephemeral_wallet(
    mint_url: &str,
    proofs: Vec<cdk::nuts::Proof>
) -> Result<cdk::Wallet, String> {
    use cdk::Wallet;
    use cdk::nuts::{CurrencyUnit, State};
    use cdk::types::ProofInfo;
    use std::sync::Arc;

    // Create IndexedDB database connection for persistent storage
    // Multiple connections to the same database are safe - IndexedDB handles concurrency
    let localstore = Arc::new(
        crate::stores::indexeddb_database::IndexedDbDatabase::new()
            .await
            .map_err(|e| format!("Failed to create IndexedDB: {}", e))?
    );

    // Derive deterministic seed from Nostr key
    let seed = derive_wallet_seed().await?;

    // Create wallet
    let wallet = Wallet::new(
        mint_url,
        CurrencyUnit::Sat,
        localstore.clone(),
        seed,
        None // target_proof_count
    ).map_err(|e| format!("Failed to create wallet: {}", e))?;

    // Fetch mint info and keysets
    wallet.fetch_mint_info().await
        .map_err(|e| format!("Failed to fetch mint info: {}", e))?;

    // Ensure we have all keysets loaded
    wallet.refresh_keysets().await
        .map_err(|e| format!("Failed to refresh keysets: {}", e))?;

    log::debug!("Ephemeral wallet created for {}", mint_url);

    // Inject proofs if any provided
    if !proofs.is_empty() {
        use cdk::mint_url::MintUrl as CdkMintUrl;
        let mint_url_parsed: CdkMintUrl = mint_url.parse()
            .map_err(|e| format!("Invalid mint URL: {}", e))?;

        let proof_infos: Vec<_> = proofs.into_iter()
            .map(|p| {
                ProofInfo::new(
                    p,
                    mint_url_parsed.clone(),
                    State::Unspent,
                    CurrencyUnit::Sat
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to create proof info: {}", e))?;

        localstore.update_proofs(proof_infos, vec![]).await
            .map_err(|e| format!("Failed to inject proofs: {}", e))?;
    }

    Ok(wallet)
}

// ============================================================================
// Phase 2B: Receive Implementation
// ============================================================================

/// Receive ecash from a token string
pub async fn receive_tokens(token_string: String) -> Result<u64, String> {
    use cdk::nuts::Token;
    use cdk::wallet::ReceiveOptions;
    use std::str::FromStr;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Receiving token...");

    // Sanitize token string - remove ALL whitespace (spaces, tabs, newlines)
    // This is crucial because copy/paste often adds line breaks in the middle
    let token_string = token_string
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();

    if token_string.is_empty() {
        return Err("Token string is empty".to_string());
    }

    log::info!("Token string length: {}, starts with: {}",
        token_string.len(),
        token_string.chars().take(10).collect::<String>());

    // Validate token format
    if !token_string.starts_with("cashuA") && !token_string.starts_with("cashuB") {
        return Err(format!(
            "Invalid token format. Cashu tokens must start with 'cashuA' or 'cashuB'. Your token starts with: '{}'",
            token_string.chars().take(10).collect::<String>()
        ));
    }

    // Additional validation: check for non-ASCII or control characters that might indicate encoding issues
    if token_string.chars().any(|c| c.is_control()) {
        log::warn!("Token contains control characters");
        return Err("Token contains invalid control characters. Please copy the token again.".to_string());
    }

    // Extract and validate the base64 portion
    let base64_part = if token_string.starts_with("cashuA") {
        &token_string[6..]
    } else if token_string.starts_with("cashuB") {
        &token_string[6..]
    } else {
        ""
    };

    log::info!("Base64 portion length: {}, last 20 chars: {}",
        base64_part.len(),
        base64_part.chars().rev().take(20).collect::<String>().chars().rev().collect::<String>());

    // Check if base64 length is valid and try auto-correction
    let remainder = base64_part.len() % 4;
    let token_to_parse = if remainder != 0 {
        log::warn!("Base64 portion length {} is not a multiple of 4. Remainder: {}",
            base64_part.len(), remainder);

        // Try adding padding if it's close to being valid
        if remainder == 2 || remainder == 3 {
            let padding_needed = 4 - remainder;
            log::warn!(
                "Auto-correcting malformed token: adding {} padding character(s) to base64 portion (original length: {})",
                padding_needed,
                base64_part.len()
            );
            let padded = format!("{}{}", token_string, "=".repeat(padding_needed));
            log::info!("Attempting to parse with {} padding characters added", padding_needed);
            padded
        } else {
            token_string.clone()
        }
    } else {
        token_string.clone()
    };

    // Parse token (try padded version if applicable, otherwise use original)
    let token = Token::from_str(&token_to_parse)
        .map_err(|e| {
            log::error!("Token parse error: {:?}", e);
            log::error!("Token length: {}, base64 part length: {}", token_to_parse.len(), base64_part.len());
            let error_str = e.to_string();

            // Provide helpful error messages
            if error_str.contains("6-bit remainder") || error_str.contains("InvalidLength") {
                return format!(
                    "Token appears to be incomplete or corrupted (base64 length: {}, remainder: {}). Please ensure you copied the entire token.",
                    base64_part.len(),
                    remainder
                );
            } else if error_str.contains("InvalidByte") {
                return "Token contains invalid characters. Please copy the token again carefully.".to_string();
            }

            format!("Invalid token format: {}", e)
        })?;

    if token_to_parse != token_string {
        log::info!("Successfully parsed token after adding padding!");
    }

    let mint_url = token.mint_url()
        .map_err(|e| {
            log::error!("Mint URL extraction error: {:?}", e);
            format!("Failed to get mint URL: {}", e)
        })?
        .to_string();

    log::info!("Token from mint: {}", mint_url);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Receive token (contacts mint to swap proofs) with auto-cleanup on spent errors
    // Use the corrected/padded token to ensure padding fix is preserved
    let amount_received = match wallet.receive(
        &token_to_parse,
        ReceiveOptions::default()
    ).await {
        Ok(amount) => amount,
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already spent") || error_msg.contains("already redeemed") {
                log::warn!("Token already spent or redeemed, checking for spent proofs in wallet");

                // Cleanup any spent proofs in our wallet to keep state clean
                match cleanup_spent_proofs(mint_url.clone()).await {
                    Ok((cleaned_count, cleaned_amount)) if cleaned_count > 0 => {
                        log::info!("Cleaned up {} spent proofs worth {} sats from wallet", cleaned_count, cleaned_amount);
                        return Err(format!(
                            "This token has already been spent. However, we cleaned up {} spent proofs ({} sats) from your wallet.",
                            cleaned_count, cleaned_amount
                        ));
                    }
                    Ok(_) => {
                        log::info!("No spent proofs found in wallet");
                        return Err("This token has already been spent and cannot be redeemed.".to_string());
                    }
                    Err(cleanup_err) => {
                        log::error!("Cleanup failed: {}", cleanup_err);
                        return Err("This token has already been spent and cannot be redeemed.".to_string());
                    }
                }
            }
            return Err(format!("Failed to receive token: {}", e));
        }
    };

    log::info!("Received {} sats", u64::from(amount_received));

    // Get received proofs
    let new_proofs = wallet.get_unspent_proofs().await
        .map_err(|e| format!("Failed to get proofs: {}", e))?;

    // Convert to ProofData
    let proof_data: Vec<ProofData> = new_proofs.iter()
        .map(|p| cdk_proof_to_proof_data(p))
        .collect();

    // Create token event (kind 7375)
    let token_event_data = TokenEventData {
        mint: mint_url.clone(),
        proofs: proof_data.clone(),
        del: vec![],
    };

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(
        Kind::from(7375),
        encrypted
    );

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let event_output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    let event_id = event_output.id().to_hex();

    log::info!("Published token event: {}", event_id);

    // Update local state
    let mut tokens = WALLET_TOKENS.write();
    tokens.push(TokenData {
        event_id: event_id.clone(),
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: proof_data.iter().map(|p| CashuProof {
            id: p.id.clone(),
            amount: p.amount,
            secret: p.secret.clone(),
            c: p.c.clone(),
        }).collect(),
        created_at: chrono::Utc::now().timestamp() as u64,
    });

    // Recalculate balance from all tokens using checked arithmetic
    let new_balance: u64 = tokens.iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow in receive_tokens".to_string())?;

    drop(tokens);

    // Update balance
    let amount = u64::from(amount_received);
    *WALLET_BALANCE.write() = new_balance;

    log::info!("Balance after receive: {} sats", new_balance);

    // Create history event (kind 7376) with direction: "in"
    if let Err(e) = create_history_event("in", amount, vec![event_id.clone()], vec![]).await {
        log::error!("Failed to create history event: {}", e);
        // Don't fail the whole operation if history event creation fails
    }

    Ok(amount)
}

// ============================================================================
// Phase 2C: Send Implementation
// ============================================================================

/// Send ecash tokens
pub async fn send_tokens(
    mint_url: String,
    amount: u64,
) -> Result<String, String> {
    use cdk::wallet::{SendOptions, SendKind};
    use cdk::Amount;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Sending {} sats from {}", amount, mint_url);

    // Get available proofs for this mint
    let (all_proofs, event_ids_to_delete) = {
        let tokens = WALLET_TOKENS.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            return Err("No tokens found for this mint".to_string());
        }

        // Convert to CDK proofs
        let mut all_proofs = Vec::new();
        let mut event_ids_to_delete = Vec::new();

        for token in &mint_tokens {
            event_ids_to_delete.push(token.event_id.clone());
            for proof in &token.proofs {
                // Convert CashuProof to ProofData
                let temp_proof_data = ProofData {
                    id: proof.id.clone(),
                    amount: proof.amount,
                    secret: proof.secret.clone(),
                    c: proof.c.clone(),
                };
                all_proofs.push(proof_data_to_cdk_proof(&temp_proof_data)?);
            }
        }

        (all_proofs, event_ids_to_delete)
    }; // Read lock dropped here

    // Check balance
    let total_available: u64 = all_proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats, Required: {} sats",
            total_available, amount
        ));
    }

    // Prepare and confirm send with auto-retry on spent proofs
    let (token_string, keep_proofs) = {
        // Try sending with current proofs
        let result = async {
            let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;

            let prepared = wallet.prepare_send(
                Amount::from(amount),
                SendOptions {
                    conditions: None, // TODO: Support P2PK in Phase 2E
                    include_fee: true,
                    send_kind: SendKind::OnlineTolerance(Amount::from(1)),
                    ..Default::default()
                }
            ).await
            .map_err(|e| e.to_string())?;

            log::info!("Send fee: {} sats", u64::from(prepared.fee()));

            let token = prepared.confirm(None).await
                .map_err(|e| e.to_string())?;
            let keep_proofs = wallet.get_unspent_proofs().await
                .map_err(|e| e.to_string())?;

            Ok::<(cdk::nuts::Token, Vec<cdk::nuts::Proof>), String>((token, keep_proofs))
        }.await;

        match result {
            Ok((token, proofs)) => (token.to_string(), proofs),
            Err(e) => {
                let error_msg = e.to_string();

                // Auto-retry if proofs are already spent
                if error_msg.contains("already spent") || error_msg.contains("already redeemed") {
                    log::warn!("Some proofs already spent, cleaning up and retrying...");

                    // Cleanup spent proofs
                    let (cleaned_count, cleaned_amount) = cleanup_spent_proofs(mint_url.clone()).await
                        .map_err(|e| format!("Cleanup failed: {}", e))?;

                    log::info!("Cleaned up {} spent proofs worth {} sats, retrying send", cleaned_count, cleaned_amount);

                    // Get fresh proofs after cleanup
                    let fresh_proofs = {
                        let tokens = WALLET_TOKENS.read();
                        let mut proofs = Vec::new();

                        for token in tokens.iter().filter(|t| t.mint == mint_url) {
                            for proof in &token.proofs {
                                let temp = ProofData {
                                    id: proof.id.clone(),
                                    amount: proof.amount,
                                    secret: proof.secret.clone(),
                                    c: proof.c.clone(),
                                };
                                proofs.push(proof_data_to_cdk_proof(&temp)?);
                            }
                        }
                        proofs
                    };

                    // Check we still have enough after cleanup
                    let fresh_total: u64 = fresh_proofs.iter().map(|p| u64::from(p.amount)).sum();
                    if fresh_total < amount {
                        return Err(format!(
                            "Insufficient funds after cleanup. Available: {} sats, Required: {} sats",
                            fresh_total, amount
                        ));
                    }

                    // Retry send with fresh proofs
                    let wallet = create_ephemeral_wallet(&mint_url, fresh_proofs).await?;

                    let prepared = wallet.prepare_send(
                        Amount::from(amount),
                        SendOptions {
                            conditions: None,
                            include_fee: true,
                            send_kind: SendKind::OnlineTolerance(Amount::from(1)),
                            ..Default::default()
                        }
                    ).await
                    .map_err(|e| format!("Retry failed: {}", e))?;

                    let token = prepared.confirm(None).await
                        .map_err(|e| format!("Retry confirm failed: {}", e))?;

                    let keep_proofs = wallet.get_unspent_proofs().await
                        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

                    log::info!("Send succeeded after cleanup and retry");
                    (token.to_string(), keep_proofs)
                } else {
                    return Err(format!("Failed to send: {}", e));
                }
            }
        }
    };

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let mut new_event_id: Option<String> = None;

    // Update token events
    if !keep_proofs.is_empty() {
        // Create new token event with remaining proofs
        let proof_data: Vec<ProofData> = keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        let token_event_data = TokenEventData {
            mint: mint_url.clone(),
            proofs: proof_data.clone(),
            del: event_ids_to_delete.clone(), // Mark old token events as deleted
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(
            Kind::from(7375),
            encrypted
        );

        let event_output = client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish token event: {}", e))?;

        new_event_id = Some(event_output.id().to_hex());
        log::info!("Published new token event: {}", new_event_id.as_ref().unwrap());
    }

    // Delete old token events with kind-5
    if !event_ids_to_delete.is_empty() {
        let mut tags = Vec::new();
        for event_id in &event_ids_to_delete {
            tags.push(nostr_sdk::Tag::event(
                nostr_sdk::EventId::from_hex(event_id)
                    .map_err(|e| format!("Invalid event ID: {}", e))?
            ));
        }

        let deletion_builder = nostr_sdk::EventBuilder::new(
            Kind::from(5),
            "Spent token"
        ).tags(tags);

        client.send_event_builder(deletion_builder).await
            .map_err(|e| format!("Failed to publish deletion event: {}", e))?;

        log::info!("Published deletion events for {} token events", event_ids_to_delete.len());
    }

    // Update local state
    let mut tokens_write = WALLET_TOKENS.write();

    // Remove only the specific token events we used (not all tokens for this mint!)
    tokens_write.retain(|t| !event_ids_to_delete.contains(&t.event_id));

    // Add new token with remaining proofs if any
    if let Some(ref event_id) = new_event_id {
        let keep_proof_data: Vec<ProofData> = keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        tokens_write.push(TokenData {
            event_id: event_id.clone(),
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: keep_proof_data.iter().map(|p| CashuProof {
                id: p.id.clone(),
                amount: p.amount,
                secret: p.secret.clone(),
                c: p.c.clone(),
            }).collect(),
            created_at: chrono::Utc::now().timestamp() as u64,
        });
    }

    // Recalculate balance from remaining tokens using checked arithmetic
    let new_balance: u64 = tokens_write.iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow in send_tokens".to_string())?;

    drop(tokens_write);

    // Update balance
    *WALLET_BALANCE.write() = new_balance;

    log::info!("Balance after send: {} sats", new_balance);

    // Create history event (kind 7376) with direction: "out"
    let created = if let Some(ref id) = new_event_id { vec![id.clone()] } else { vec![] };
    if let Err(e) = create_history_event("out", amount, created, event_ids_to_delete.clone()).await {
        log::error!("Failed to create history event: {}", e);
        // Don't fail the whole operation if history event creation fails
    }

    Ok(token_string)
}

// ============================================================================
// Phase 2D: Cleanup & Error Handling
// ============================================================================

/// Create a history event (kind 7376)
async fn create_history_event(
    direction: &str,
    amount: u64,
    created_tokens: Vec<String>,
    destroyed_tokens: Vec<String>,
) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Build content array: [["direction", "in"], ["amount", "100"], ["e", "id", "", "created"], ...]
    let mut content_array = vec![
        vec!["direction".to_string(), direction.to_string()],
        vec!["amount".to_string(), amount.to_string()],
    ];

    for token_id in created_tokens {
        content_array.push(vec![
            "e".to_string(),
            token_id,
            "".to_string(),
            "created".to_string()
        ]);
    }

    for token_id in destroyed_tokens {
        content_array.push(vec![
            "e".to_string(),
            token_id,
            "".to_string(),
            "destroyed".to_string()
        ]);
    }

    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt history: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(
        Kind::from(7376),
        encrypted
    );

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish history event: {}", e))?;

    log::info!("Created history event: {} {} sats", direction, amount);

    Ok(())
}

/// Check and cleanup spent proofs for a mint
/// Returns the number of proofs cleaned up and the amount
pub async fn cleanup_spent_proofs(mint_url: String) -> Result<(usize, u64), String> {
    use cdk::nuts::State;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Checking for spent proofs on {}", mint_url);

    // Get all token events and proofs for this mint (scope the read to drop lock early)
    let (cdk_proofs, event_ids_to_delete, all_mint_proofs) = {
        let tokens = WALLET_TOKENS.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            log::info!("No proofs to check");
            return Ok((0, 0));
        }

        let event_ids: Vec<String> = mint_tokens.iter()
            .map(|t| t.event_id.clone())
            .collect();

        let all_proofs: Vec<CashuProof> = mint_tokens.iter()
            .flat_map(|t| &t.proofs)
            .cloned()
            .collect();

        // Convert to CDK proofs
        let cdk_proofs: Result<Vec<_>, _> = all_proofs.iter()
            .map(|p| {
                let temp = ProofData {
                    id: p.id.clone(),
                    amount: p.amount,
                    secret: p.secret.clone(),
                    c: p.c.clone(),
                };
                proof_data_to_cdk_proof(&temp)
            })
            .collect();

        (cdk_proofs?, event_ids, all_proofs)
    }; // Read lock dropped here

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Check states at mint
    let states = wallet.check_proofs_spent(cdk_proofs.clone()).await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;

    // Find spent and unspent proofs
    let mut spent_secrets = std::collections::HashSet::new();
    let mut spent_amount = 0u64;

    for (state, proof) in states.iter().zip(cdk_proofs.iter()) {
        if matches!(state.state, State::Spent) {
            spent_secrets.insert(proof.secret.to_string());
            spent_amount += u64::from(proof.amount);
        }
    }

    if spent_secrets.is_empty() {
        log::info!("No spent proofs found");
        return Ok((0, 0));
    }

    let spent_count = spent_secrets.len();
    log::info!("Found {} spent proofs worth {} sats, cleaning up", spent_count, spent_amount);

    // Filter to keep only unspent proofs
    let unspent_proofs: Vec<CashuProof> = all_mint_proofs.into_iter()
        .filter(|p| !spent_secrets.contains(&p.secret))
        .collect();

    // Get signer and pubkey for creating events
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let mut new_event_id: Option<String> = None;

    // Create new token event with unspent proofs if any remain
    if !unspent_proofs.is_empty() {
        let proof_data: Vec<ProofData> = unspent_proofs.iter()
            .map(|p| ProofData {
                id: p.id.clone(),
                amount: p.amount,
                secret: p.secret.clone(),
                c: p.c.clone(),
            })
            .collect();

        let token_event_data = TokenEventData {
            mint: mint_url.clone(),
            proofs: proof_data.clone(),
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(
            Kind::from(7375),
            encrypted
        );

        let event_output = client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish token event: {}", e))?;

        new_event_id = Some(event_output.id().to_hex());
        log::info!("Published new token event with {} unspent proofs: {}", unspent_proofs.len(), new_event_id.as_ref().unwrap());
    }

    // Delete old token events with kind-5
    if !event_ids_to_delete.is_empty() {
        let mut tags = Vec::new();
        for event_id in &event_ids_to_delete {
            tags.push(nostr_sdk::Tag::event(
                nostr_sdk::EventId::from_hex(event_id)
                    .map_err(|e| format!("Invalid event ID: {}", e))?
            ));
        }

        let deletion_builder = nostr_sdk::EventBuilder::new(
            Kind::from(5),
            "Cleaned up spent proofs"
        ).tags(tags);

        client.send_event_builder(deletion_builder).await
            .map_err(|e| format!("Failed to publish deletion event: {}", e))?;

        log::info!("Published deletion events for {} token events", event_ids_to_delete.len());
    }

    // Update local state
    let mut tokens_write = WALLET_TOKENS.write();

    // Remove old tokens for this mint
    tokens_write.retain(|t| t.mint != mint_url);

    // Add new token with unspent proofs if any
    if let Some(ref event_id) = new_event_id {
        tokens_write.push(TokenData {
            event_id: event_id.clone(),
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: unspent_proofs,
            created_at: chrono::Utc::now().timestamp() as u64,
        });
    }

    // Recalculate balance from all tokens using checked arithmetic
    let new_balance: u64 = tokens_write.iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow in cleanup_spent_proofs".to_string())?;

    drop(tokens_write);

    // Update balance
    *WALLET_BALANCE.write() = new_balance;

    log::info!("Cleanup complete. Removed {} proofs worth {} sats. New balance: {} sats",
        spent_count, spent_amount, new_balance);

    Ok((spent_count, spent_amount))
}

/// Remove a mint and all its associated tokens from the wallet
/// Creates deletion events for all token events from this mint
/// Returns (event_count, total_amount) of removed tokens
pub async fn remove_mint(mint_url: String) -> Result<(usize, u64), String> {
    log::info!("Removing mint: {}", mint_url);

    // Get all token events for this mint (scoped read)
    let (event_ids_to_delete, total_amount, token_count) = {
        let tokens = WALLET_TOKENS.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            log::info!("No tokens found for this mint");
            return Ok((0, 0));
        }

        let event_ids: Vec<String> = mint_tokens.iter()
            .map(|t| t.event_id.clone())
            .collect();

        let amount: u64 = mint_tokens.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .sum();

        (event_ids, amount, mint_tokens.len())
    }; // Read lock dropped

    log::info!("Found {} token events worth {} sats to remove", token_count, total_amount);

    // Create kind-5 deletion event for all token events
    let mut tags = Vec::new();
    for event_id in &event_ids_to_delete {
        tags.push(nostr_sdk::Tag::event(
            nostr_sdk::EventId::parse(event_id)
                .map_err(|e| format!("Invalid event ID: {}", e))?
        ));
    }

    let deletion_builder = nostr_sdk::EventBuilder::new(
        Kind::from(5),
        format!("Removed mint: {}", mint_url)
    ).tags(tags);

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    client.send_event_builder(deletion_builder).await
        .map_err(|e| format!("Failed to publish deletion event: {}", e))?;

    log::info!("Published deletion event for {} token events", event_ids_to_delete.len());

    // Update local state - remove all tokens for this mint
    let mut tokens_write = WALLET_TOKENS.write();
    tokens_write.retain(|t| t.mint != mint_url);
    drop(tokens_write);

    // Remove mint from wallet state
    let mut state_write = WALLET_STATE.write();
    if let Some(ref mut state) = *state_write {
        state.mints.retain(|m| m != &mint_url);
    }
    drop(state_write);

    // Recalculate balance using checked arithmetic
    let tokens = WALLET_TOKENS.read();
    let new_balance: u64 = tokens.iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow in remove_mint".to_string())?;
    drop(tokens);

    *WALLET_BALANCE.write() = new_balance;

    log::info!("Mint removed. Deleted {} events worth {} sats. New balance: {} sats",
        token_count, total_amount, new_balance);

    Ok((token_count, total_amount))
}

// ============================================================================
// Phase 3: Lightning Payment Support (Mint & Melt Operations)
// ============================================================================

/// Mint quote information for receiving lightning payments
#[derive(Clone, Debug, PartialEq)]
pub struct MintQuoteInfo {
    pub quote_id: String,
    pub invoice: String,
    pub amount: u64,
    pub expiry: u64,
    pub mint_url: String,
}

/// Melt quote information for sending lightning payments
#[derive(Clone, Debug, PartialEq)]
pub struct MeltQuoteInfo {
    pub quote_id: String,
    pub invoice: String,
    pub amount: u64,
    pub fee_reserve: u64,
    pub mint_url: String,
}

/// Quote status for polling
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // Some variants reserved for future use
pub enum QuoteStatus {
    Unpaid,
    Paid,
    Pending,
    Failed,
    Expired,
}

/// Global signal for pending mint quotes
pub static PENDING_MINT_QUOTES: GlobalSignal<Vec<MintQuoteInfo>> = Signal::global(Vec::new);

/// Global signal for pending melt quotes
pub static PENDING_MELT_QUOTES: GlobalSignal<Vec<MeltQuoteInfo>> = Signal::global(Vec::new);

/// Create a mint quote (request lightning invoice to receive sats)
pub async fn create_mint_quote(
    mint_url: String,
    amount_sats: u64,
    description: Option<String>,
) -> Result<MintQuoteInfo, String> {
    use cdk::Amount;

    log::info!("Creating mint quote for {} sats at {}", amount_sats, mint_url);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Create mint quote
    let quote = wallet.mint_quote(
        Amount::from(amount_sats),
        description
    ).await
    .map_err(|e| format!("Failed to create mint quote: {}", e))?;

    log::info!("Mint quote created: {}", quote.id);

    let quote_info = MintQuoteInfo {
        quote_id: quote.id.clone(),
        invoice: quote.request,
        amount: u64::from(quote.amount.unwrap_or(Amount::ZERO)),
        expiry: quote.expiry,
        mint_url: mint_url.clone(),
    };

    // Store in global state for tracking
    PENDING_MINT_QUOTES.write().push(quote_info.clone());

    Ok(quote_info)
}

/// Check mint quote payment status
pub async fn check_mint_quote_status(
    mint_url: String,
    quote_id: String,
) -> Result<QuoteStatus, String> {
    log::info!("Checking mint quote status: {}", quote_id);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Check quote status
    let response = wallet.mint_quote_state(&quote_id).await
        .map_err(|e| format!("Failed to check mint quote status: {}", e))?;

    use cdk::nuts::MintQuoteState;

    let status = match response.state {
        MintQuoteState::Unpaid => QuoteStatus::Unpaid,
        MintQuoteState::Paid => QuoteStatus::Paid,
        MintQuoteState::Issued => QuoteStatus::Paid, // Already minted
    };

    log::info!("Quote {} status: {:?}", quote_id, status);

    Ok(status)
}

/// Mint tokens from a paid quote
pub async fn mint_tokens_from_quote(
    mint_url: String,
    quote_id: String,
) -> Result<u64, String> {
    use cdk::nuts::MintQuoteState;

    log::info!("Minting tokens from quote: {}", quote_id);

    // Create ephemeral wallet (now shares database with the wallet that created the quote)
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Verify the quote is paid and ready to mint
    let quote_response = wallet.mint_quote_state(&quote_id).await
        .map_err(|e| format!("Failed to fetch quote state: {}", e))?;

    log::info!("Quote state: {:?}, amount: {:?}", quote_response.state, quote_response.amount);

    // Verify the quote is paid AND not already issued
    match quote_response.state {
        MintQuoteState::Paid => {
            // Good to proceed
        }
        MintQuoteState::Issued => {
            return Err("Quote has already been minted. Tokens were already issued for this payment.".to_string());
        }
        MintQuoteState::Unpaid => {
            return Err("Quote has not been paid yet. Please pay the lightning invoice first.".to_string());
        }
    }

    log::info!("Quote is paid, proceeding to mint tokens");

    // Mint tokens - the quote is already in the shared database from create_mint_quote
    let proofs = match wallet.mint(
        &quote_id,
        cdk::amount::SplitTarget::default(),
        None  // No spending conditions (TODO: support P2PK in future)
    ).await {
        Ok(proofs) => {
            log::info!("Mint succeeded, received {} proofs", proofs.len());
            proofs
        }
        Err(e) => {
            let error_msg = e.to_string();
            log::error!("Mint failed with error: {:?}", e);
            log::error!("Error message: {}", error_msg);

            // CRITICAL: Clean up the quote from shared database even on failure
            // Why: The mint might have partially processed the blinded messages before failing
            // If the mint marked them as "used" but didn't return signatures, retrying with
            // the same quote will fail with "Blinded Message is already signed"
            if let Err(cleanup_err) = wallet.localstore.remove_mint_quote(&quote_id).await {
                log::warn!("Failed to remove mint quote after error: {}", cleanup_err);
            } else {
                log::info!("Cleaned up failed quote {} from database", quote_id);
            }

            // Also remove from pending quotes signal
            PENDING_MINT_QUOTES.write().retain(|q| q.quote_id != quote_id);

            // If the quote lookup fails, it means the shared database lost the quote somehow
            // 1. The quote was already used/issued
            // 2. The quote is not in the correct state
            // 3. The request was malformed

            if error_msg.contains("missing field `signatures`") {
                // Try to extract more context about what went wrong
                if error_msg.contains("line 1 column") {
                    return Err(format!(
                        "Mint returned an error instead of tokens. The quote has been cleaned up. \
                        Please generate a NEW invoice and try again (do not retry with the same invoice). \
                        Technical error: {}",
                        error_msg
                    ));
                }
            }

            return Err(format!("Failed to mint tokens: {}", error_msg));
        }
    };

    let amount_minted: u64 = proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    log::info!("Minted {} sats", amount_minted);

    // Convert to ProofData
    let proof_data: Vec<ProofData> = proofs.iter()
        .map(|p| cdk_proof_to_proof_data(p))
        .collect();

    // Create token event (kind 7375) - same as receive_tokens
    let token_event_data = TokenEventData {
        mint: mint_url.clone(),
        proofs: proof_data.clone(),
        del: vec![],
    };

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(
        Kind::from(7375),
        encrypted
    );

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let event_output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    let event_id = event_output.id().to_hex();

    log::info!("Published token event: {}", event_id);

    // Update local state
    let mut tokens = WALLET_TOKENS.write();
    tokens.push(TokenData {
        event_id: event_id.clone(),
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: proof_data.iter().map(|p| CashuProof {
            id: p.id.clone(),
            amount: p.amount,
            secret: p.secret.clone(),
            c: p.c.clone(),
        }).collect(),
        created_at: chrono::Utc::now().timestamp() as u64,
    });
    drop(tokens);

    // Update balance using checked arithmetic
    let current_balance = *WALLET_BALANCE.read();
    let new_balance = current_balance.checked_add(amount_minted)
        .ok_or_else(|| format!("Balance overflow when adding {} to {}", amount_minted, current_balance))?;
    *WALLET_BALANCE.write() = new_balance;

    // Create history event
    create_history_event_with_type(
        "in",
        amount_minted,
        vec![event_id.clone()],
        vec![],
        Some("lightning_mint"),
        None,
    ).await?;

    // Remove from pending quotes
    PENDING_MINT_QUOTES.write().retain(|q| q.quote_id != quote_id);

    // Clean up: Remove quote from shared database to prevent reuse
    if let Err(e) = wallet.localstore.remove_mint_quote(&quote_id).await {
        log::warn!("Failed to remove mint quote from database: {}", e);
    }

    log::info!("Mint complete: {} sats", amount_minted);

    Ok(amount_minted)
}

/// Create a melt quote (request to pay a lightning invoice)
pub async fn create_melt_quote(
    mint_url: String,
    invoice: String,
) -> Result<MeltQuoteInfo, String> {
    log::info!("Creating melt quote for invoice at {}", mint_url);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Create melt quote
    let quote = wallet.melt_quote(invoice.clone(), None).await
        .map_err(|e| format!("Failed to create melt quote: {}", e))?;

    log::info!("Melt quote created: {}", quote.id);

    let quote_info = MeltQuoteInfo {
        quote_id: quote.id.clone(),
        invoice: quote.request,
        amount: u64::from(quote.amount),
        fee_reserve: u64::from(quote.fee_reserve),
        mint_url: mint_url.clone(),
    };

    // Store in global state
    PENDING_MELT_QUOTES.write().push(quote_info.clone());

    Ok(quote_info)
}

/// Check melt quote status
#[allow(dead_code)] // Reserved for future melt quote status polling
pub async fn check_melt_quote_status(
    mint_url: String,
    quote_id: String,
) -> Result<QuoteStatus, String> {
    log::info!("Checking melt quote status: {}", quote_id);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Check quote status
    let response = wallet.melt_quote_status(&quote_id).await
        .map_err(|e| format!("Failed to check melt quote status: {}", e))?;

    use cdk::nuts::MeltQuoteState;

    let status = match response.state {
        MeltQuoteState::Unpaid => QuoteStatus::Unpaid,
        MeltQuoteState::Paid => QuoteStatus::Paid,
        MeltQuoteState::Pending => QuoteStatus::Pending,
        MeltQuoteState::Failed => QuoteStatus::Failed,
        _ => QuoteStatus::Unpaid,
    };

    log::info!("Melt quote {} status: {:?}", quote_id, status);

    Ok(status)
}

/// Melt tokens to pay a lightning invoice
pub async fn melt_tokens(
    mint_url: String,
    quote_id: String,
) -> Result<(bool, Option<String>, u64), String> {
    use nostr_sdk::signer::NostrSigner;

    log::info!("Melting tokens to pay invoice via quote: {}", quote_id);

    // Get melt quote details
    let quote_info = PENDING_MELT_QUOTES.read()
        .iter()
        .find(|q| q.quote_id == quote_id)
        .cloned()
        .ok_or("Melt quote not found")?;

    let amount_needed = quote_info.amount + quote_info.fee_reserve;

    // Get available proofs for this mint
    let (all_proofs, event_ids_to_delete) = {
        let tokens = WALLET_TOKENS.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            return Err("No tokens found for this mint".to_string());
        }

        // Convert to CDK proofs
        let mut all_proofs = Vec::new();
        let mut event_ids_to_delete = Vec::new();

        for token in &mint_tokens {
            event_ids_to_delete.push(token.event_id.clone());
            for proof in &token.proofs {
                let temp_proof_data = ProofData {
                    id: proof.id.clone(),
                    amount: proof.amount,
                    secret: proof.secret.clone(),
                    c: proof.c.clone(),
                };
                all_proofs.push(proof_data_to_cdk_proof(&temp_proof_data)?);
            }
        }

        (all_proofs, event_ids_to_delete)
    };

    // Check balance
    let total_available: u64 = all_proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    if total_available < amount_needed {
        return Err(format!(
            "Insufficient funds. Need {} sats (amount: {}, fee: {}), have: {} sats",
            amount_needed, quote_info.amount, quote_info.fee_reserve, total_available
        ));
    }

    // Melt with auto-retry on spent proofs
    let (melted, keep_proofs) = {
        let result = async {
            let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;

            // Execute melt
            let melted = wallet.melt(&quote_id).await
                .map_err(|e| e.to_string())?;

            let keep_proofs = wallet.get_unspent_proofs().await
                .map_err(|e| e.to_string())?;

            Ok::<(cdk::types::Melted, Vec<cdk::nuts::Proof>), String>((melted, keep_proofs))
        }.await;

        match result {
            Ok((melted, proofs)) => (melted, proofs),
            Err(e) => {
                let error_msg = e.to_string();

                // Auto-retry if proofs are already spent
                if error_msg.contains("already spent") || error_msg.contains("already redeemed") {
                    log::warn!("Some proofs already spent, cleaning up and retrying...");

                    // Cleanup spent proofs
                    let (cleaned_count, cleaned_amount) = cleanup_spent_proofs(mint_url.clone()).await
                        .map_err(|e| format!("Cleanup failed: {}", e))?;

                    log::info!("Cleaned up {} spent proofs worth {} sats, retrying melt", cleaned_count, cleaned_amount);

                    // Get fresh proofs after cleanup
                    let fresh_proofs = {
                        let tokens = WALLET_TOKENS.read();
                        let mut proofs = Vec::new();

                        for token in tokens.iter().filter(|t| t.mint == mint_url) {
                            for proof in &token.proofs {
                                let temp = ProofData {
                                    id: proof.id.clone(),
                                    amount: proof.amount,
                                    secret: proof.secret.clone(),
                                    c: proof.c.clone(),
                                };
                                proofs.push(proof_data_to_cdk_proof(&temp)?);
                            }
                        }
                        proofs
                    };

                    // Check we still have enough after cleanup
                    let fresh_total: u64 = fresh_proofs.iter().map(|p| u64::from(p.amount)).sum();
                    if fresh_total < amount_needed {
                        return Err(format!(
                            "Insufficient funds after cleanup. Need: {} sats, have: {} sats",
                            amount_needed, fresh_total
                        ));
                    }

                    // Retry melt with fresh proofs
                    let wallet = create_ephemeral_wallet(&mint_url, fresh_proofs).await?;
                    let melted = wallet.melt(&quote_id).await
                        .map_err(|e| format!("Retry failed: {}", e))?;
                    let keep_proofs = wallet.get_unspent_proofs().await
                        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

                    log::info!("Melt succeeded after cleanup and retry");
                    (melted, keep_proofs)
                } else {
                    // Clean up the melt quote on failure to prevent issues with retries
                    log::warn!("Melt failed, cleaning up quote from database");
                    PENDING_MELT_QUOTES.write().retain(|q| q.quote_id != quote_id);

                    // Remove from wallet database using dedicated cleanup function
                    if let Err(cleanup_err) = remove_melt_quote_from_db(&quote_id).await {
                        log::error!("Failed to remove melt quote from database: {}", cleanup_err);
                        // Continue anyway - the quote is already removed from PENDING_MELT_QUOTES
                    }

                    return Err(format!("Failed to melt: {}. Quote has been cleaned up, please try again with a new quote.", e));
                }
            }
        }
    };

    let paid = melted.state == cdk::nuts::MeltQuoteState::Paid;
    let preimage = melted.preimage;
    let fee_paid = u64::from(melted.fee_paid);

    log::info!("Melt result: paid={}, fee_paid={}", paid, fee_paid);

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let mut new_event_id: Option<String> = None;

    // Update token events with remaining proofs
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        let token_event_data = TokenEventData {
            mint: mint_url.clone(),
            proofs: proof_data.clone(),
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(
            Kind::from(7375),
            encrypted
        );

        let event_output = client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish token event: {}", e))?;

        new_event_id = Some(event_output.id().to_hex());
        log::info!("Published new token event: {}", new_event_id.as_ref().unwrap());
    }

    // Delete old token events
    if !event_ids_to_delete.is_empty() {
        let mut tags = Vec::new();
        for event_id in &event_ids_to_delete {
            tags.push(nostr_sdk::Tag::event(
                nostr_sdk::EventId::from_hex(event_id)
                    .map_err(|e| format!("Invalid event ID: {}", e))?
            ));
        }

        let deletion_builder = nostr_sdk::EventBuilder::new(
            Kind::from(5),
            "Melted token"
        ).tags(tags);

        client.send_event_builder(deletion_builder).await
            .map_err(|e| format!("Failed to publish deletion event: {}", e))?;

        log::info!("Published deletion event for {} token events", event_ids_to_delete.len());
    }

    // Update local state
    let mut tokens_write = WALLET_TOKENS.write();
    tokens_write.retain(|t| !event_ids_to_delete.contains(&t.event_id));

    if let Some(ref event_id) = new_event_id {
        let proof_data: Vec<ProofData> = keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        tokens_write.push(TokenData {
            event_id: event_id.clone(),
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: proof_data.iter().map(|p| CashuProof {
                id: p.id.clone(),
                amount: p.amount,
                secret: p.secret.clone(),
                c: p.c.clone(),
            }).collect(),
            created_at: chrono::Utc::now().timestamp() as u64,
        });
    }
    drop(tokens_write);

    // Update balance using checked arithmetic
    let tokens = WALLET_TOKENS.read();
    let new_balance: u64 = tokens.iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow in melt_tokens".to_string())?;
    drop(tokens);

    *WALLET_BALANCE.write() = new_balance;

    // Create history event
    let created_events = if let Some(ref event_id) = new_event_id {
        vec![event_id.clone()]
    } else {
        vec![]
    };

    create_history_event_with_type(
        "out",
        quote_info.amount + fee_paid,
        created_events,
        event_ids_to_delete,
        Some("lightning_melt"),
        Some(&quote_info.invoice),
    ).await?;

    // Remove from pending quotes
    PENDING_MELT_QUOTES.write().retain(|q| q.quote_id != quote_id);

    log::info!("Melt complete: paid={}, amount={}, fee={}", paid, quote_info.amount, fee_paid);

    Ok((paid, preimage, fee_paid))
}

/// Extended history event creation with operation type and invoice
async fn create_history_event_with_type(
    direction: &str,
    amount: u64,
    created_tokens: Vec<String>,
    destroyed_tokens: Vec<String>,
    operation_type: Option<&str>,
    invoice: Option<&str>,
) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Build content array
    let mut content_array = vec![
        vec!["direction".to_string(), direction.to_string()],
        vec!["amount".to_string(), amount.to_string()],
        vec!["unit".to_string(), "sat".to_string()],
    ];

    // Add operation type if provided
    if let Some(op_type) = operation_type {
        content_array.push(vec!["type".to_string(), op_type.to_string()]);
    }

    // Add invoice if provided (for lightning operations)
    if let Some(inv) = invoice {
        content_array.push(vec!["invoice".to_string(), inv.to_string()]);
    }

    // Add created token events
    for event_id in created_tokens {
        content_array.push(vec!["e".to_string(), event_id, "".to_string(), "created".to_string()]);
    }

    // Add destroyed token events
    for event_id in destroyed_tokens {
        content_array.push(vec!["e".to_string(), event_id, "".to_string(), "destroyed".to_string()]);
    }

    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(
        Kind::from(7376),
        encrypted
    );

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let event_output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish history event: {}", e))?;

    log::info!("Published history event: {}", event_output.id().to_hex());

    Ok(())
}
