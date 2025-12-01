//! Mint Capability Checking
//!
//! Comprehensive NUT support detection and feature gating.
//! Checks mint capabilities before attempting operations.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use super::internal::get_or_create_wallet;

// =============================================================================
// NUT Capabilities
// =============================================================================

/// All known NUT numbers for capability checking
///
/// Note: NUT-06 (Optional amounts) and NUT-13 (Deterministic secrets) are not included
/// as they are client-side implementation details, not advertised mint capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Nut {
    /// NUT-04: Minting (Lightning receive)
    Minting = 4,
    /// NUT-05: Melting (Lightning send)
    Melting = 5,
    /// NUT-07: Proof state check
    ProofState = 7,
    /// NUT-08: Swap operations
    Swap = 8,
    /// NUT-09: Restore (recovery)
    Restore = 9,
    /// NUT-10: Spending conditions (P2PK base)
    SpendingConditions = 10,
    /// NUT-11: P2PK support
    P2pk = 11,
    /// NUT-12: DLEQ proofs
    Dleq = 12,
    /// NUT-14: HTLC
    Htlc = 14,
    /// NUT-15: Multi-path payments
    Mpp = 15,
    /// NUT-17: WebSocket subscriptions
    WebSocket = 17,
    /// NUT-19: Cached responses
    CachedResponses = 19,
    /// NUT-20: Quote signatures
    QuoteSignatures = 20,
    /// NUT-21: Clear auth
    ClearAuth = 21,
    /// NUT-22: Blind auth
    BlindAuth = 22,
}

/// Mint capabilities derived from mint info
#[derive(Debug, Clone, Default)]
pub struct MintCapabilities {
    /// Supported NUTs
    pub supported_nuts: Vec<u8>,
    /// Whether minting is enabled
    pub minting_enabled: bool,
    /// Whether melting is enabled
    pub melting_enabled: bool,
    /// Supported payment methods for minting
    pub mint_methods: Vec<String>,
    /// Supported payment methods for melting
    pub melt_methods: Vec<String>,
    /// Maximum mint amount (if limited)
    pub max_mint_amount: Option<u64>,
    /// Minimum mint amount
    pub min_mint_amount: Option<u64>,
    /// Maximum melt amount (if limited)
    pub max_melt_amount: Option<u64>,
    /// Minimum melt amount
    pub min_melt_amount: Option<u64>,
    /// Maximum number of inputs per request
    pub max_inputs: Option<usize>,
    /// Maximum number of outputs per request
    pub max_outputs: Option<usize>,
    /// Input fee in ppk (per proof, per thousand)
    pub input_fee_ppk: u64,
    /// Whether auth is required
    pub auth_required: bool,
    /// Auth type if required
    pub auth_type: Option<String>,
}

impl MintCapabilities {
    /// Check if a specific NUT is supported
    pub fn supports_nut(&self, nut: Nut) -> bool {
        self.supported_nuts.contains(&(nut as u8))
    }

    /// Check if P2PK is supported
    pub fn supports_p2pk(&self) -> bool {
        self.supports_nut(Nut::P2pk) && self.supports_nut(Nut::SpendingConditions)
    }

    /// Check if HTLC is supported
    pub fn supports_htlc(&self) -> bool {
        self.supports_nut(Nut::Htlc) && self.supports_nut(Nut::SpendingConditions)
    }

    /// Check if MPP is supported
    pub fn supports_mpp(&self) -> bool {
        self.supports_nut(Nut::Mpp)
    }

    /// Check if WebSocket is supported
    pub fn supports_websocket(&self) -> bool {
        self.supports_nut(Nut::WebSocket)
    }

    /// Check if DLEQ verification is available
    pub fn supports_dleq(&self) -> bool {
        self.supports_nut(Nut::Dleq)
    }

    /// Check if restore is available
    pub fn supports_restore(&self) -> bool {
        self.supports_nut(Nut::Restore)
    }

    /// Check if mint can perform basic operations
    pub fn is_operational(&self) -> bool {
        self.minting_enabled || self.melting_enabled
    }

    /// Get list of missing required NUTs for an operation
    pub fn missing_nuts_for(&self, operation: OperationKind) -> Vec<Nut> {
        let required = operation.required_nuts();
        required
            .into_iter()
            .filter(|nut| !self.supports_nut(*nut))
            .collect()
    }
}

/// Operation kinds for capability checking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    /// Basic minting (lightning receive)
    Mint,
    /// Basic melting (lightning send)
    Melt,
    /// P2PK send
    P2pkSend,
    /// HTLC send
    HtlcSend,
    /// Multi-path payment
    MppMelt,
    /// Token swap
    Swap,
    /// Proof state check
    CheckProofs,
    /// Wallet restore
    Restore,
}

impl OperationKind {
    /// Get required NUTs for this operation
    pub fn required_nuts(&self) -> Vec<Nut> {
        match self {
            Self::Mint => vec![Nut::Minting],
            Self::Melt => vec![Nut::Melting],
            Self::P2pkSend => vec![Nut::SpendingConditions, Nut::P2pk],
            Self::HtlcSend => vec![Nut::SpendingConditions, Nut::Htlc],
            Self::MppMelt => vec![Nut::Melting, Nut::Mpp],
            Self::Swap => vec![Nut::Swap],
            Self::CheckProofs => vec![Nut::ProofState],
            Self::Restore => vec![Nut::Restore],
        }
    }
}

// =============================================================================
// Capability Fetching
// =============================================================================

/// Fetch and parse mint capabilities
pub async fn get_mint_capabilities(mint_url: &str) -> Result<MintCapabilities, String> {
    log::info!("Fetching capabilities for mint: {}", mint_url);

    let wallet = get_or_create_wallet(mint_url).await?;

    let mint_info = wallet
        .fetch_mint_info()
        .await
        .map_err(|e| format!("Failed to fetch mint info: {}", e))?
        .ok_or("Mint info not available")?;

    let mut caps = MintCapabilities::default();

    // Parse NUT support
    // NUT-4: Minting
    if !mint_info.nuts.nut04.methods.is_empty() {
        caps.supported_nuts.push(4);
        caps.minting_enabled = !mint_info.nuts.nut04.disabled;
        caps.mint_methods = mint_info
            .nuts
            .nut04
            .methods
            .iter()
            .map(|m| m.method.to_string())
            .collect();

        // Extract limits if available
        for method in &mint_info.nuts.nut04.methods {
            if let Some(max) = method.max_amount {
                caps.max_mint_amount = Some(u64::from(max));
            }
            if let Some(min) = method.min_amount {
                caps.min_mint_amount = Some(u64::from(min));
            }
        }
    }

    // NUT-5: Melting
    if !mint_info.nuts.nut05.methods.is_empty() {
        caps.supported_nuts.push(5);
        caps.melting_enabled = !mint_info.nuts.nut05.disabled;
        caps.melt_methods = mint_info
            .nuts
            .nut05
            .methods
            .iter()
            .map(|m| m.method.to_string())
            .collect();

        for method in &mint_info.nuts.nut05.methods {
            if let Some(max) = method.max_amount {
                caps.max_melt_amount = Some(u64::from(max));
            }
            if let Some(min) = method.min_amount {
                caps.min_melt_amount = Some(u64::from(min));
            }
        }
    }

    // NUT-7: Proof state
    if mint_info.nuts.nut07.supported {
        caps.supported_nuts.push(7);
    }

    // NUT-8: Swap
    if mint_info.nuts.nut08.supported {
        caps.supported_nuts.push(8);
    }

    // NUT-9: Restore
    if mint_info.nuts.nut09.supported {
        caps.supported_nuts.push(9);
    }

    // NUT-10: Spending conditions
    if mint_info.nuts.nut10.supported {
        caps.supported_nuts.push(10);
    }

    // NUT-11: P2PK
    if mint_info.nuts.nut11.supported {
        caps.supported_nuts.push(11);
    }

    // NUT-12: DLEQ
    if mint_info.nuts.nut12.supported {
        caps.supported_nuts.push(12);
    }

    // NUT-14: HTLC
    if mint_info.nuts.nut14.supported {
        caps.supported_nuts.push(14);
    }

    // NUT-15: Multi-path payments (MPP)
    // CDK pattern: check if methods list is non-empty
    if !mint_info.nuts.nut15.methods.is_empty() {
        caps.supported_nuts.push(15);
    }

    // NUT-17: WebSocket subscriptions
    // CDK pattern: check if supported methods list is non-empty
    if !mint_info.nuts.nut17.supported.is_empty() {
        caps.supported_nuts.push(17);
    }

    // NUT-19: Cached responses
    // Check if cached_endpoints is non-empty or ttl is set
    if !mint_info.nuts.nut19.cached_endpoints.is_empty() || mint_info.nuts.nut19.ttl.is_some() {
        caps.supported_nuts.push(19);
    }

    // NUT-20: Quote signatures
    if mint_info.nuts.nut20.supported {
        caps.supported_nuts.push(20);
    }

    // NUT-21: Clear auth
    if mint_info.nuts.nut21.is_some() {
        caps.supported_nuts.push(21);
        caps.auth_required = true;
        caps.auth_type = Some("clear".to_string());
    }

    // NUT-22: Blind auth
    if mint_info.nuts.nut22.is_some() {
        caps.supported_nuts.push(22);
        caps.auth_required = true;
        if caps.auth_type.is_none() {
            caps.auth_type = Some("blind".to_string());
        }
    }

    // Sort for consistent display
    caps.supported_nuts.sort();

    log::info!(
        "Mint {} capabilities: {:?}",
        mint_url,
        caps.supported_nuts
    );

    Ok(caps)
}

/// Check if an operation is supported by a mint
pub async fn check_operation_supported(
    mint_url: &str,
    operation: OperationKind,
) -> Result<(), String> {
    let caps = get_mint_capabilities(mint_url).await?;
    let missing = caps.missing_nuts_for(operation);

    if missing.is_empty() {
        Ok(())
    } else {
        let missing_str: Vec<String> = missing.iter().map(|n| format!("NUT-{}", *n as u8)).collect();
        Err(format!(
            "Mint does not support required features: {}",
            missing_str.join(", ")
        ))
    }
}

/// Quick check if P2PK is supported (cached)
pub async fn supports_p2pk(mint_url: &str) -> bool {
    match get_mint_capabilities(mint_url).await {
        Ok(caps) => caps.supports_p2pk(),
        Err(_) => false,
    }
}

/// Quick check if MPP is supported
pub async fn supports_mpp(mint_url: &str) -> bool {
    match get_mint_capabilities(mint_url).await {
        Ok(caps) => caps.supports_mpp(),
        Err(_) => false,
    }
}

/// Quick check if restore is supported
pub async fn supports_restore(mint_url: &str) -> bool {
    match get_mint_capabilities(mint_url).await {
        Ok(caps) => caps.supports_restore(),
        Err(_) => false,
    }
}

// =============================================================================
// Limit Checking
// =============================================================================

/// Check if an amount is within mint limits for minting
pub async fn check_mint_limits(mint_url: &str, amount: u64) -> Result<(), String> {
    let caps = get_mint_capabilities(mint_url).await?;

    if let Some(max) = caps.max_mint_amount {
        if amount > max {
            return Err(format!(
                "Amount {} exceeds mint maximum of {} sats",
                amount, max
            ));
        }
    }

    if let Some(min) = caps.min_mint_amount {
        if amount < min {
            return Err(format!(
                "Amount {} is below mint minimum of {} sats",
                amount, min
            ));
        }
    }

    Ok(())
}

/// Check if an amount is within mint limits for melting
pub async fn check_melt_limits(mint_url: &str, amount: u64) -> Result<(), String> {
    let caps = get_mint_capabilities(mint_url).await?;

    if let Some(max) = caps.max_melt_amount {
        if amount > max {
            return Err(format!(
                "Amount {} exceeds melt maximum of {} sats",
                amount, max
            ));
        }
    }

    if let Some(min) = caps.min_melt_amount {
        if amount < min {
            return Err(format!(
                "Amount {} is below melt minimum of {} sats",
                amount, min
            ));
        }
    }

    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_required_nuts() {
        let p2pk_nuts = OperationKind::P2pkSend.required_nuts();
        assert!(p2pk_nuts.contains(&Nut::SpendingConditions));
        assert!(p2pk_nuts.contains(&Nut::P2pk));
    }

    #[test]
    fn test_capabilities_supports() {
        let mut caps = MintCapabilities::default();
        caps.supported_nuts = vec![4, 5, 7, 8, 10, 11];

        assert!(caps.supports_nut(Nut::Minting));
        assert!(caps.supports_p2pk());
        assert!(!caps.supports_mpp());
        assert!(!caps.supports_htlc());
    }
}
