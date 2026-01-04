//! Mempool.space API Client
//!
//! Fetches Bitcoin transaction and address data from mempool.space.
//! Supports custom endpoints for self-hosted instances.

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

/// Default mempool.space API endpoint
pub const DEFAULT_ENDPOINT: &str = "https://mempool.space/api";

// ============================================================================
// API Response Types
// ============================================================================

/// Transaction data from mempool.space
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BitcoinTransaction {
    /// Transaction ID
    pub txid: String,
    /// Transaction version
    pub version: i32,
    /// Locktime
    pub locktime: u32,
    /// Inputs
    pub vin: Vec<TxInput>,
    /// Outputs
    pub vout: Vec<TxOutput>,
    /// Transaction size in bytes
    pub size: u32,
    /// Transaction weight
    pub weight: u32,
    /// Virtual size in vbytes
    pub vsize: u32,
    /// Transaction fee in satoshis
    pub fee: u64,
    /// Confirmation status
    pub status: TxStatus,
}

/// Transaction input
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxInput {
    /// Previous transaction ID
    pub txid: String,
    /// Previous output index
    pub vout: u32,
    /// Previous output being spent
    pub prevout: Option<TxOutput>,
    /// Sequence number
    pub sequence: u64,
    /// Is this a coinbase input?
    #[serde(default)]
    pub is_coinbase: bool,
}

/// Transaction output
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxOutput {
    /// Output value in satoshis
    pub value: u64,
    /// Output script pubkey (hex)
    pub scriptpubkey: String,
    /// Script pubkey assembly
    #[serde(default)]
    pub scriptpubkey_asm: String,
    /// Script type
    #[serde(default)]
    pub scriptpubkey_type: String,
    /// Output address (if applicable)
    pub scriptpubkey_address: Option<String>,
}

/// Transaction confirmation status
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxStatus {
    /// Whether the transaction is confirmed
    pub confirmed: bool,
    /// Block height (if confirmed)
    pub block_height: Option<u64>,
    /// Block hash (if confirmed)
    pub block_hash: Option<String>,
    /// Block timestamp (if confirmed)
    pub block_time: Option<u64>,
}

/// Address information
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BitcoinAddress {
    /// The address
    pub address: String,
    /// Confirmed chain statistics
    pub chain_stats: AddressStats,
    /// Unconfirmed mempool statistics
    pub mempool_stats: AddressStats,
}

/// Address statistics
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AddressStats {
    /// Number of funded outputs
    pub funded_txo_count: u64,
    /// Total funded amount in satoshis
    pub funded_txo_sum: u64,
    /// Number of spent outputs
    pub spent_txo_count: u64,
    /// Total spent amount in satoshis
    pub spent_txo_sum: u64,
    /// Total transaction count
    pub tx_count: u64,
}

// ============================================================================
// API Functions
// ============================================================================

/// Fetch transaction details
pub async fn get_transaction(endpoint: &str, txid: &str) -> Result<BitcoinTransaction, String> {
    let url = format!("{}/tx/{}", endpoint.trim_end_matches('/'), txid);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}: {}", response.status(), response.status_text()));
    }

    response
        .json::<BitcoinTransaction>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Fetch address information
pub async fn get_address(endpoint: &str, address: &str) -> Result<BitcoinAddress, String> {
    let url = format!("{}/address/{}", endpoint.trim_end_matches('/'), address);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}: {}", response.status(), response.status_text()));
    }

    response
        .json::<BitcoinAddress>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Truncate a txid or address for display
pub fn truncate_bitcoin_id(id: &str) -> String {
    if id.len() > 16 {
        format!("{}...{}", &id[0..8], &id[id.len() - 8..])
    } else {
        id.to_string()
    }
}
