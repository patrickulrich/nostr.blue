//! Enriched Transaction History
//!
//! Enhanced transaction history with additional metadata:
//! - Fee tracking per transaction
//! - Swap details (source/destination keysets)
//! - Error details for failed transactions
//! - Transaction descriptions from quotes

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use nostr_sdk::nips::nip60::TransactionDirection;
use serde::{Deserialize, Serialize};

use super::types::HistoryItem;

// =============================================================================
// Direction Wrapper (for serialization)
// =============================================================================

/// Direction enum that can be serialized (wraps TransactionDirection)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Incoming transaction
    In,
    /// Outgoing transaction
    Out,
}

impl From<TransactionDirection> for Direction {
    fn from(dir: TransactionDirection) -> Self {
        match dir {
            TransactionDirection::In => Direction::In,
            TransactionDirection::Out => Direction::Out,
        }
    }
}

impl From<Direction> for TransactionDirection {
    fn from(dir: Direction) -> Self {
        match dir {
            Direction::In => TransactionDirection::In,
            Direction::Out => TransactionDirection::Out,
        }
    }
}

// =============================================================================
// Enriched History Types
// =============================================================================

/// Enriched transaction history item
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnrichedHistoryItem {
    /// Base history item
    #[serde(flatten)]
    pub base: HistoryItemData,

    /// Transaction fee paid (in sats)
    #[serde(default)]
    pub fee_paid: Option<u64>,

    /// Fee percentage (for melt operations)
    #[serde(default)]
    pub fee_percent: Option<f64>,

    /// Transaction description/memo
    #[serde(default)]
    pub description: Option<String>,

    /// Mint URL
    #[serde(default)]
    pub mint_url: Option<String>,

    /// Transaction type (more specific than direction)
    #[serde(default)]
    pub tx_type: Option<TransactionType>,

    /// Error message if transaction failed
    #[serde(default)]
    pub error: Option<String>,

    /// Related quote ID
    #[serde(default)]
    pub quote_id: Option<String>,

    /// Lightning invoice (for mint/melt)
    #[serde(default)]
    pub invoice: Option<String>,

    /// Lightning preimage (for successful melts)
    #[serde(default)]
    pub preimage: Option<String>,

    /// Keyset information
    #[serde(default)]
    pub keyset_info: Option<KeysetInfo>,

    /// P2PK recipient (if P2PK send)
    #[serde(default)]
    pub p2pk_recipient: Option<String>,

    /// Swap details (if swap operation)
    #[serde(default)]
    pub swap_details: Option<SwapDetails>,
}

/// Base history item data (matches HistoryItem)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryItemData {
    pub event_id: String,
    pub direction: Direction,
    pub amount: u64,
    pub unit: String,
    pub created_at: u64,
    pub created_tokens: Vec<String>,
    pub destroyed_tokens: Vec<String>,
    pub redeemed_events: Vec<String>,
}

impl From<HistoryItem> for HistoryItemData {
    fn from(item: HistoryItem) -> Self {
        Self {
            event_id: item.event_id,
            direction: item.direction.into(),
            amount: item.amount,
            unit: item.unit,
            created_at: item.created_at,
            created_tokens: item.created_tokens,
            destroyed_tokens: item.destroyed_tokens,
            redeemed_events: item.redeemed_events,
        }
    }
}

/// Specific transaction types
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    /// Lightning receive (mint)
    LightningReceive,
    /// Lightning send (melt)
    LightningSend,
    /// Ecash send
    EcashSend,
    /// Ecash receive
    EcashReceive,
    /// P2PK send
    P2pkSend,
    /// P2PK receive
    P2pkReceive,
    /// HTLC send
    HtlcSend,
    /// HTLC receive
    HtlcReceive,
    /// Swap (consolidation/optimization)
    Swap,
    /// Cross-mint transfer
    Transfer,
    /// Keyset migration
    KeysetMigration,
    /// Restore recovery
    Restore,
}

impl TransactionType {
    /// Get human-readable name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::LightningReceive => "Lightning Receive",
            Self::LightningSend => "Lightning Send",
            Self::EcashSend => "Ecash Send",
            Self::EcashReceive => "Ecash Receive",
            Self::P2pkSend => "P2PK Send",
            Self::P2pkReceive => "P2PK Receive",
            Self::HtlcSend => "HTLC Send",
            Self::HtlcReceive => "HTLC Receive",
            Self::Swap => "Swap",
            Self::Transfer => "Transfer",
            Self::KeysetMigration => "Keyset Migration",
            Self::Restore => "Restore",
        }
    }

    /// Get icon name for display
    pub fn icon(&self) -> &'static str {
        match self {
            Self::LightningReceive => "lightning-in",
            Self::LightningSend => "lightning-out",
            Self::EcashSend | Self::P2pkSend | Self::HtlcSend => "send",
            Self::EcashReceive | Self::P2pkReceive | Self::HtlcReceive => "receive",
            Self::Swap | Self::KeysetMigration => "swap",
            Self::Transfer => "transfer",
            Self::Restore => "restore",
        }
    }
}

/// Keyset information for a transaction
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeysetInfo {
    /// Source keyset ID (for spends/swaps)
    pub source_keyset: Option<String>,
    /// Destination keyset ID (for receives/swaps)
    pub dest_keyset: Option<String>,
    /// Whether this was a keyset migration
    pub is_migration: bool,
}

/// Swap operation details
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwapDetails {
    /// Number of input proofs
    pub input_count: usize,
    /// Number of output proofs
    pub output_count: usize,
    /// Input value
    pub input_value: u64,
    /// Output value
    pub output_value: u64,
    /// Reason for swap
    pub reason: SwapReason,
}

/// Reason for swap operation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwapReason {
    /// Consolidation to reduce proof count
    Consolidation,
    /// Denomination optimization
    Optimization,
    /// Keyset migration
    Migration,
    /// Privacy refresh
    Privacy,
    /// Split for exact amount
    Split,
    /// Unknown/other
    Other,
}

// =============================================================================
// Enrichment Functions
// =============================================================================

/// Enrich a history item with additional data
pub fn enrich_history_item(
    item: HistoryItem,
    fee_paid: Option<u64>,
    description: Option<String>,
    tx_type: Option<TransactionType>,
) -> EnrichedHistoryItem {
    EnrichedHistoryItem {
        base: item.into(),
        fee_paid,
        fee_percent: None,
        description,
        mint_url: None,
        tx_type,
        error: None,
        quote_id: None,
        invoice: None,
        preimage: None,
        keyset_info: None,
        p2pk_recipient: None,
        swap_details: None,
    }
}

/// Create enriched history for lightning receive
pub fn create_lightning_receive_history(
    event_id: String,
    amount: u64,
    mint_url: String,
    quote_id: String,
    invoice: String,
    created_at: u64,
) -> EnrichedHistoryItem {
    EnrichedHistoryItem {
        base: HistoryItemData {
            event_id,
            direction: Direction::In,
            amount,
            unit: "sat".to_string(),
            created_at,
            created_tokens: Vec::new(),
            destroyed_tokens: Vec::new(),
            redeemed_events: Vec::new(),
        },
        fee_paid: None, // Mints typically don't charge for receiving
        fee_percent: None,
        description: Some("Lightning deposit".to_string()),
        mint_url: Some(mint_url),
        tx_type: Some(TransactionType::LightningReceive),
        error: None,
        quote_id: Some(quote_id),
        invoice: Some(invoice),
        preimage: None,
        keyset_info: None,
        p2pk_recipient: None,
        swap_details: None,
    }
}

/// Create enriched history for lightning send
pub fn create_lightning_send_history(
    event_id: String,
    amount: u64,
    fee_paid: u64,
    mint_url: String,
    quote_id: String,
    invoice: String,
    preimage: Option<String>,
    created_at: u64,
) -> EnrichedHistoryItem {
    let fee_percent = if amount > 0 {
        Some((fee_paid as f64 / amount as f64) * 100.0)
    } else {
        None
    };

    EnrichedHistoryItem {
        base: HistoryItemData {
            event_id,
            direction: Direction::Out,
            amount,
            unit: "sat".to_string(),
            created_at,
            created_tokens: Vec::new(),
            destroyed_tokens: Vec::new(),
            redeemed_events: Vec::new(),
        },
        fee_paid: Some(fee_paid),
        fee_percent,
        description: Some("Lightning payment".to_string()),
        mint_url: Some(mint_url),
        tx_type: Some(TransactionType::LightningSend),
        error: None,
        quote_id: Some(quote_id),
        invoice: Some(invoice),
        preimage,
        keyset_info: None,
        p2pk_recipient: None,
        swap_details: None,
    }
}

/// Create enriched history for P2PK send
pub fn create_p2pk_send_history(
    event_id: String,
    amount: u64,
    fee_paid: Option<u64>,
    mint_url: String,
    recipient: String,
    created_at: u64,
) -> EnrichedHistoryItem {
    // Use UTF-8 safe character slicing to avoid panic on multi-byte chars
    let recipient_short: String = recipient.chars().take(8).collect();
    EnrichedHistoryItem {
        base: HistoryItemData {
            event_id,
            direction: Direction::Out,
            amount,
            unit: "sat".to_string(),
            created_at,
            created_tokens: Vec::new(),
            destroyed_tokens: Vec::new(),
            redeemed_events: Vec::new(),
        },
        fee_paid,
        fee_percent: None,
        description: Some(format!("P2PK send to {}", recipient_short)),
        mint_url: Some(mint_url),
        tx_type: Some(TransactionType::P2pkSend),
        error: None,
        quote_id: None,
        invoice: None,
        preimage: None,
        keyset_info: None,
        p2pk_recipient: Some(recipient),
        swap_details: None,
    }
}

/// Create enriched history for swap
pub fn create_swap_history(
    event_id: String,
    mint_url: String,
    input_count: usize,
    output_count: usize,
    input_value: u64,
    output_value: u64,
    reason: SwapReason,
    created_at: u64,
) -> EnrichedHistoryItem {
    let fee_paid = input_value.saturating_sub(output_value);

    EnrichedHistoryItem {
        base: HistoryItemData {
            event_id,
            direction: Direction::In, // Net effect is neutral
            amount: output_value,
            unit: "sat".to_string(),
            created_at,
            created_tokens: Vec::new(),
            destroyed_tokens: Vec::new(),
            redeemed_events: Vec::new(),
        },
        fee_paid: Some(fee_paid),
        fee_percent: None,
        description: Some(match reason {
            SwapReason::Consolidation => "Proof consolidation".to_string(),
            SwapReason::Optimization => "Denomination optimization".to_string(),
            SwapReason::Migration => "Keyset migration".to_string(),
            SwapReason::Privacy => "Privacy refresh".to_string(),
            SwapReason::Split => "Amount split".to_string(),
            SwapReason::Other => "Swap".to_string(),
        }),
        mint_url: Some(mint_url),
        tx_type: Some(TransactionType::Swap),
        error: None,
        quote_id: None,
        invoice: None,
        preimage: None,
        keyset_info: None,
        p2pk_recipient: None,
        swap_details: Some(SwapDetails {
            input_count,
            output_count,
            input_value,
            output_value,
            reason,
        }),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_type_display() {
        assert_eq!(TransactionType::LightningSend.display_name(), "Lightning Send");
        assert_eq!(TransactionType::P2pkReceive.display_name(), "P2PK Receive");
    }

    #[test]
    fn test_swap_reason() {
        let reason = SwapReason::Consolidation;
        assert!(matches!(reason, SwapReason::Consolidation));
    }
}
