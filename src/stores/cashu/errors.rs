//! Cashu wallet error types
//!
//! Typed error handling for better context preservation and error matching.
//! Includes NUT error codes per NIP-00 specification.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use cdk::Error as CdkError;
use std::fmt;

// =============================================================================
// NUT Error Codes (per NUT-00 specification)
// =============================================================================

/// NUT error codes from the Cashu specification
/// These map to standardized error responses from mints
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum NutErrorCode {
    /// Token already spent
    TokenAlreadySpent = 11001,
    /// Token pending (locked in transaction)
    TokenPending = 11002,
    /// Transaction unbalanced (inputs != outputs + fee)
    TransactionUnbalanced = 11003,
    /// Unit not supported by mint
    UnsupportedUnit = 11004,
    /// Minting disabled
    MintingDisabled = 11005,
    /// Quote not paid
    QuoteNotPaid = 11006,
    /// Quote expired
    QuoteExpired = 11007,
    /// Quote pending
    QuotePending = 11008,
    /// Blinded message already signed
    BlindedMessageAlreadySigned = 11009,
    /// Amount out of limit range
    AmountOutOfLimitRange = 11010,
    /// Duplicate inputs
    DuplicateInputs = 11011,
    /// Duplicate outputs
    DuplicateOutputs = 11012,
    /// Multiple units in single request
    MultipleUnits = 11013,
    /// Unit mismatch
    UnitMismatch = 11014,
    /// Witness missing or invalid (P2PK)
    WitnessMissingOrInvalid = 11015,
    /// Duplicate signature
    DuplicateSignature = 11016,
    /// Lightning error
    LightningError = 20001,
    /// Invoice already paid
    InvoiceAlreadyPaid = 20002,
    /// Clear auth required (NUT-21)
    ClearAuthRequired = 21001,
    /// Clear auth failed (NUT-21)
    ClearAuthFailed = 21002,
    /// Blind auth required (NUT-22)
    BlindAuthRequired = 22001,
    /// Blind auth failed (NUT-22)
    BlindAuthFailed = 22002,
    /// Unknown/generic error
    Unknown = 65535,
}

impl NutErrorCode {
    /// Create from numeric code
    pub fn from_code(code: u16) -> Self {
        match code {
            11001 => Self::TokenAlreadySpent,
            11002 => Self::TokenPending,
            11003 => Self::TransactionUnbalanced,
            11004 => Self::UnsupportedUnit,
            11005 => Self::MintingDisabled,
            11006 => Self::QuoteNotPaid,
            11007 => Self::QuoteExpired,
            11008 => Self::QuotePending,
            11009 => Self::BlindedMessageAlreadySigned,
            11010 => Self::AmountOutOfLimitRange,
            11011 => Self::DuplicateInputs,
            11012 => Self::DuplicateOutputs,
            11013 => Self::MultipleUnits,
            11014 => Self::UnitMismatch,
            11015 => Self::WitnessMissingOrInvalid,
            11016 => Self::DuplicateSignature,
            20001 => Self::LightningError,
            20002 => Self::InvoiceAlreadyPaid,
            21001 => Self::ClearAuthRequired,
            21002 => Self::ClearAuthFailed,
            22001 => Self::BlindAuthRequired,
            22002 => Self::BlindAuthFailed,
            _ => Self::Unknown,
        }
    }

    /// Get numeric code value
    pub fn code(&self) -> u16 {
        *self as u16
    }

    /// Check if error is recoverable (can retry)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::TokenPending
                | Self::QuotePending
                | Self::LightningError
                | Self::Unknown
        )
    }

    /// Check if error indicates tokens are unusable
    pub fn is_token_lost(&self) -> bool {
        matches!(
            self,
            Self::TokenAlreadySpent | Self::BlindedMessageAlreadySigned
        )
    }
}

impl fmt::Display for NutErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TokenAlreadySpent => write!(f, "Token already spent (11001)"),
            Self::TokenPending => write!(f, "Token pending (11002)"),
            Self::TransactionUnbalanced => write!(f, "Transaction unbalanced (11003)"),
            Self::UnsupportedUnit => write!(f, "Unsupported unit (11004)"),
            Self::MintingDisabled => write!(f, "Minting disabled (11005)"),
            Self::QuoteNotPaid => write!(f, "Quote not paid (11006)"),
            Self::QuoteExpired => write!(f, "Quote expired (11007)"),
            Self::QuotePending => write!(f, "Quote pending (11008)"),
            Self::BlindedMessageAlreadySigned => write!(f, "Blinded message already signed (11009)"),
            Self::AmountOutOfLimitRange => write!(f, "Amount out of limit range (11010)"),
            Self::DuplicateInputs => write!(f, "Duplicate inputs (11011)"),
            Self::DuplicateOutputs => write!(f, "Duplicate outputs (11012)"),
            Self::MultipleUnits => write!(f, "Multiple units (11013)"),
            Self::UnitMismatch => write!(f, "Unit mismatch (11014)"),
            Self::WitnessMissingOrInvalid => write!(f, "Witness missing or invalid (11015)"),
            Self::DuplicateSignature => write!(f, "Duplicate signature (11016)"),
            Self::LightningError => write!(f, "Lightning error (20001)"),
            Self::InvoiceAlreadyPaid => write!(f, "Invoice already paid (20002)"),
            Self::ClearAuthRequired => write!(f, "Clear auth required (21001)"),
            Self::ClearAuthFailed => write!(f, "Clear auth failed (21002)"),
            Self::BlindAuthRequired => write!(f, "Blind auth required (22001)"),
            Self::BlindAuthFailed => write!(f, "Blind auth failed (22002)"),
            Self::Unknown => write!(f, "Unknown error (65535)"),
        }
    }
}

/// Cashu wallet error type
#[derive(Debug)]
pub enum CashuWalletError {
    // ==========================================================================
    // Initialization Errors
    // ==========================================================================
    WalletNotInitialized,
    SeedDerivation(String),
    SignerUnavailable,
    TermsNotAccepted,

    // ==========================================================================
    // Mint Errors
    // ==========================================================================
    MintNotFound { mint_url: String },
    MintConnection { mint_url: String, message: String },
    MintOperationLocked { mint_url: String },
    MintFeatureNotSupported { feature: String },

    // ==========================================================================
    // Token Errors
    // ==========================================================================
    InvalidToken { reason: String },
    TokenAlreadySpent,
    TokenPending,
    InsufficientFunds { available: u64, required: u64 },
    NoSpendableProofs,

    // ==========================================================================
    // Quote Errors
    // ==========================================================================
    QuoteNotFound { quote_id: String },
    QuoteExpired { quote_id: String },
    QuoteUnpaid { quote_id: String },
    QuoteFailed { message: String },

    // ==========================================================================
    // DLEQ Verification Errors (NUT-12)
    // ==========================================================================
    /// DLEQ proofs are missing from token (mint may not support NUT-12)
    DleqProofMissing,
    /// DLEQ proof verification failed (invalid signature)
    DleqVerificationFailed,

    // ==========================================================================
    // P2PK Errors
    // ==========================================================================
    InvalidPubkey(String),
    P2PKConditionsNotMet(String),

    // ==========================================================================
    // Nostr Errors
    // ==========================================================================
    NostrPublish(String),
    Encryption(String),
    Decryption(String),
    NostrEventNotFound,

    // ==========================================================================
    // Database Errors
    // ==========================================================================
    Database(String),
    IndexedDb(String),

    // ==========================================================================
    // CDK Errors
    // ==========================================================================
    Cdk(CdkError),

    // ==========================================================================
    // Transaction Errors
    // ==========================================================================
    TransactionNotFound { tx_id: u64 },
    InvalidTransactionState { expected: String, actual: String },

    // ==========================================================================
    // Internal Errors
    // ==========================================================================
    Internal(String),
    Cancelled,
    Timeout(String),
}

impl fmt::Display for CashuWalletError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WalletNotInitialized => write!(f, "Wallet not initialized"),
            Self::SeedDerivation(msg) => write!(f, "Seed derivation failed: {}", msg),
            Self::SignerUnavailable => write!(f, "Signer not available"),
            Self::TermsNotAccepted => write!(f, "Terms not accepted"),

            Self::MintNotFound { mint_url } => write!(f, "Mint not found: {}", mint_url),
            Self::MintConnection { mint_url, message } => {
                write!(f, "Failed to connect to mint {}: {}", mint_url, message)
            }
            Self::MintOperationLocked { mint_url } => {
                write!(f, "Mint operation already in progress: {}", mint_url)
            }
            Self::MintFeatureNotSupported { feature } => {
                write!(f, "Mint does not support required feature: {}", feature)
            }

            Self::InvalidToken { reason } => write!(f, "Invalid token format: {}", reason),
            Self::TokenAlreadySpent => write!(f, "Token already spent"),
            Self::TokenPending => write!(f, "Token pending at mint"),
            Self::InsufficientFunds { available, required } => {
                write!(f, "Insufficient funds: available={}, required={}", available, required)
            }
            Self::NoSpendableProofs => write!(f, "No spendable proofs available"),

            Self::QuoteNotFound { quote_id } => write!(f, "Quote not found: {}", quote_id),
            Self::QuoteExpired { quote_id } => write!(f, "Quote expired: {}", quote_id),
            Self::QuoteUnpaid { quote_id } => write!(f, "Quote unpaid: {}", quote_id),
            Self::QuoteFailed { message } => write!(f, "Quote failed: {}", message),

            Self::DleqProofMissing => write!(f, "Token does not contain DLEQ proofs for offline verification"),
            Self::DleqVerificationFailed => write!(f, "DLEQ proof verification failed - invalid signature"),

            Self::InvalidPubkey(msg) => write!(f, "Invalid pubkey format: {}", msg),
            Self::P2PKConditionsNotMet(msg) => write!(f, "P2PK spending conditions not met: {}", msg),

            Self::NostrPublish(msg) => write!(f, "Failed to publish Nostr event: {}", msg),
            Self::Encryption(msg) => write!(f, "Failed to encrypt content: {}", msg),
            Self::Decryption(msg) => write!(f, "Failed to decrypt content: {}", msg),
            Self::NostrEventNotFound => write!(f, "Nostr event not found"),

            Self::Database(msg) => write!(f, "Database error: {}", msg),
            Self::IndexedDb(msg) => write!(f, "IndexedDB error: {}", msg),

            Self::Cdk(err) => write!(f, "CDK error: {}", err),

            Self::TransactionNotFound { tx_id } => write!(f, "Transaction not found: {}", tx_id),
            Self::InvalidTransactionState { expected, actual } => {
                write!(f, "Transaction in invalid state: expected {}, got {}", expected, actual)
            }

            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
            Self::Cancelled => write!(f, "Operation cancelled"),
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
        }
    }
}

impl std::error::Error for CashuWalletError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cdk(err) => Some(err),
            _ => None,
        }
    }
}

impl From<CdkError> for CashuWalletError {
    fn from(err: CdkError) -> Self {
        Self::Cdk(err)
    }
}

/// Result type alias for cashu wallet operations
pub type CashuResult<T> = Result<T, CashuWalletError>;

impl CashuWalletError {
    /// Check if this error indicates tokens are already spent
    pub fn is_token_spent(&self) -> bool {
        matches!(
            self,
            Self::TokenAlreadySpent | Self::TokenPending | Self::Cdk(CdkError::TokenAlreadySpent)
        ) || self.is_token_spent_string()
    }

    /// Check error string for spent indicators (fallback)
    fn is_token_spent_string(&self) -> bool {
        let msg = self.to_string().to_lowercase();
        msg.contains("already spent")
            || msg.contains("already redeemed")
            || msg.contains("token pending")
    }

    /// Check if this is an insufficient funds error
    pub fn is_insufficient_funds(&self) -> bool {
        matches!(
            self,
            Self::InsufficientFunds { .. }
                | Self::NoSpendableProofs
                | Self::Cdk(CdkError::InsufficientFunds)
        ) || self.to_string().to_lowercase().contains("insufficient")
    }

    /// Check if this is a quote expiry error
    pub fn is_quote_expired(&self) -> bool {
        matches!(self, Self::QuoteExpired { .. })
            || self.to_string().to_lowercase().contains("expired")
    }

    /// Check if this is a connection/network error
    pub fn is_connection_error(&self) -> bool {
        matches!(self, Self::MintConnection { .. } | Self::Timeout(_))
    }

    /// Check if this is a DLEQ proof missing error
    pub fn is_dleq_missing(&self) -> bool {
        matches!(
            self,
            Self::DleqProofMissing | Self::Cdk(CdkError::DleqProofNotProvided)
        )
    }

    /// Check if this is a DLEQ verification failure
    pub fn is_dleq_invalid(&self) -> bool {
        matches!(
            self,
            Self::DleqVerificationFailed | Self::Cdk(CdkError::CouldNotVerifyDleq)
        )
    }

    /// Convert from string error (for legacy compatibility)
    pub fn from_string(s: impl Into<String>) -> Self {
        let msg = s.into();
        let lower = msg.to_lowercase();

        if lower.contains("already spent") || lower.contains("already redeemed") {
            return Self::TokenAlreadySpent;
        }
        if lower.contains("token pending") {
            return Self::TokenPending;
        }
        if lower.contains("insufficient") {
            // Preserve original error text - typed InsufficientFunds requires parsed values
            // which aren't available from string. Using Internal preserves diagnostic context.
            return Self::Internal(msg);
        }
        if lower.contains("expired") {
            // Preserve original error text - typed QuoteExpired requires parsed quote_id
            // which isn't available from string. Using Internal preserves diagnostic context.
            return Self::Internal(msg);
        }
        if lower.contains("not initialized") {
            return Self::WalletNotInitialized;
        }

        Self::Internal(msg)
    }
}

// =============================================================================
// Legacy Compatibility Helpers
// =============================================================================

/// Helper function to check if a CDK error indicates tokens are already spent
pub fn is_token_already_spent_error(error: &CdkError) -> bool {
    match error {
        CdkError::TokenAlreadySpent => true,
        CdkError::TokenPending => true,
        _ => is_token_spent_error_string(&error.to_string()),
    }
}

/// Helper function to check if an error message indicates tokens are already spent
pub fn is_token_spent_error_string(error_msg: &str) -> bool {
    let msg = error_msg.to_lowercase();
    msg.contains("already spent")
        || msg.contains("already redeemed")
        || msg.contains("token pending")
}

/// Helper function to check if a CDK error indicates insufficient funds
pub fn is_insufficient_funds_error(error: &CdkError) -> bool {
    match error {
        CdkError::InsufficientFunds => true,
        _ => is_insufficient_funds_error_string(&error.to_string()),
    }
}

/// Helper function to check if an error message indicates insufficient funds
pub fn is_insufficient_funds_error_string(error_msg: &str) -> bool {
    error_msg.to_lowercase().contains("insufficient")
}

/// Helper function to check if a CDK error indicates DLEQ proof is missing (NUT-12)
pub fn is_dleq_missing_error(error: &CdkError) -> bool {
    matches!(error, CdkError::DleqProofNotProvided)
}

/// Helper function to check if a CDK error indicates DLEQ verification failed (NUT-12)
pub fn is_dleq_verification_error(error: &CdkError) -> bool {
    matches!(error, CdkError::CouldNotVerifyDleq)
}

// =============================================================================
// Conversion Traits
// =============================================================================

impl From<String> for CashuWalletError {
    fn from(s: String) -> Self {
        Self::from_string(s)
    }
}

impl From<&str> for CashuWalletError {
    fn from(s: &str) -> Self {
        Self::from_string(s)
    }
}

// =============================================================================
// CDK to NUT Error Code Mapping
// =============================================================================

/// Map CDK error to NUT error code
pub fn cdk_error_to_nut_code(error: &CdkError) -> NutErrorCode {
    match error {
        CdkError::TokenAlreadySpent => NutErrorCode::TokenAlreadySpent,
        CdkError::TokenPending => NutErrorCode::TokenPending,
        CdkError::UnsupportedUnit => NutErrorCode::UnsupportedUnit,
        CdkError::MintingDisabled => NutErrorCode::MintingDisabled,
        CdkError::PaymentFailed => NutErrorCode::LightningError,
        CdkError::RequestAlreadyPaid => NutErrorCode::InvoiceAlreadyPaid,
        CdkError::SignatureMissingOrInvalid => NutErrorCode::WitnessMissingOrInvalid,
        CdkError::TransactionUnbalanced(_, _, _) => NutErrorCode::TransactionUnbalanced,
        CdkError::BlindAuthRequired => NutErrorCode::BlindAuthRequired,
        CdkError::ClearAuthRequired => NutErrorCode::ClearAuthRequired,
        CdkError::BlindAuthFailed => NutErrorCode::BlindAuthFailed,
        CdkError::ClearAuthFailed => NutErrorCode::ClearAuthFailed,
        _ => NutErrorCode::Unknown,
    }
}

/// Extract NUT error code from CashuWalletError
impl CashuWalletError {
    /// Get NUT error code if applicable
    pub fn nut_error_code(&self) -> Option<NutErrorCode> {
        match self {
            Self::TokenAlreadySpent => Some(NutErrorCode::TokenAlreadySpent),
            Self::TokenPending => Some(NutErrorCode::TokenPending),
            Self::QuoteExpired { .. } => Some(NutErrorCode::QuoteExpired),
            Self::QuoteUnpaid { .. } => Some(NutErrorCode::QuoteNotPaid),
            Self::Cdk(cdk_err) => Some(cdk_error_to_nut_code(cdk_err)),
            _ => None,
        }
    }

    /// Check if error is recoverable based on NUT error code
    pub fn is_recoverable(&self) -> bool {
        self.nut_error_code()
            .map(|code| code.is_recoverable())
            .unwrap_or(true) // Unknown errors might be recoverable
    }

    /// Check if tokens involved in this error are lost (cannot be recovered)
    pub fn are_tokens_lost(&self) -> bool {
        self.nut_error_code()
            .map(|code| code.is_token_lost())
            .unwrap_or(false)
    }
}
