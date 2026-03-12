//! NIP-69 P2P Order Event Parsing Utilities
//!
//! Handles parsing of peer-to-peer order events:
//! - Kind 38383: P2P Order (addressable)
//!
//! NIP-69 defines a standard for P2P trading orders to create a unified
//! liquidity pool across multiple platforms (Mostro, lnp2pBot, RoboSats, Peach).
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
/// Get current Unix timestamp in seconds (WASM-compatible)
fn now_secs() -> u64 {
    crate::platform::timestamp::now_secs()
}
/// P2P order event kind (addressable)
/// Note: Kind::PeerToPeerOrder is also available in nostr-sdk
pub const KIND_P2P_ORDER: u16 = 38383;
/// Order type: buy or sell
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    #[default]
    Buy,
    Sell,
}
impl OrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderType::Buy => "buy",
            OrderType::Sell => "sell",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "buy" => Some(OrderType::Buy),
            "sell" => Some(OrderType::Sell),
            _ => None,
        }
    }
}
impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// Order status from NIP-69
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    #[default]
    Pending,
    Canceled,
    #[serde(rename = "in-progress")]
    InProgress,
    Success,
    Expired,
}
impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "pending",
            OrderStatus::Canceled => "canceled",
            OrderStatus::InProgress => "in-progress",
            OrderStatus::Success => "success",
            OrderStatus::Expired => "expired",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(OrderStatus::Pending),
            "canceled" | "cancelled" => Some(OrderStatus::Canceled),
            "in-progress" | "in_progress" | "inprogress" => Some(OrderStatus::InProgress),
            "success" | "completed" => Some(OrderStatus::Success),
            "expired" => Some(OrderStatus::Expired),
            _ => None,
        }
    }
    /// Check if order is active (can be taken)
    pub fn is_active(&self) -> bool {
        matches!(self, OrderStatus::Pending)
    }
}
impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// Network type
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    Mainnet,
    Testnet,
    Signet,
}
impl Network {
    pub fn as_str(&self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Testnet => "testnet",
            Network::Signet => "signet",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mainnet" | "main" | "bitcoin" => Some(Network::Mainnet),
            "testnet" | "test" => Some(Network::Testnet),
            "signet" => Some(Network::Signet),
            _ => None,
        }
    }
}
impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// Layer for the trade
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    #[default]
    Lightning,
    Onchain,
    Liquid,
}
impl Layer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Layer::Lightning => "lightning",
            Layer::Onchain => "onchain",
            Layer::Liquid => "liquid",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "lightning" | "ln" => Some(Layer::Lightning),
            "onchain" | "on-chain" | "mainchain" => Some(Layer::Onchain),
            "liquid" | "lbtc" => Some(Layer::Liquid),
            _ => None,
        }
    }
}
impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// Fiat amount - can be fixed or a range
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FiatAmount {
    Fixed(f64),
    Range { min: f64, max: f64 },
}
impl FiatAmount {
    /// Display the fiat amount with currency
    pub fn display(&self, currency: &str) -> String {
        match self {
            FiatAmount::Fixed(amt) => format!("{:.0} {}", amt, currency),
            FiatAmount::Range { min, max } => {
                format!("{:.0}-{:.0} {}", min, max, currency)
            }
        }
    }
    /// Check if this fiat amount overlaps with a filter range
    pub fn overlaps(&self, filter_amount: f64, threshold: f64) -> bool {
        let filter_min = filter_amount * (1.0 - threshold);
        let filter_max = filter_amount * (1.0 + threshold);
        let (self_min, self_max) = match self {
            FiatAmount::Fixed(amt) => (*amt, *amt),
            FiatAmount::Range { min, max } => (*min, *max),
        };
        filter_min.max(self_min) <= filter_max.min(self_max)
    }
}
impl Default for FiatAmount {
    fn default() -> Self {
        FiatAmount::Fixed(0.0)
    }
}
/// Maker rating structure (parsed from JSON in rating tag)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    pub total_reviews: u32,
    pub total_rating: f64,
    pub last_rating: u32,
    pub max_rate: u32,
    pub min_rate: u32,
}
impl Rating {
    /// Calculate average rating
    pub fn average(&self) -> f64 {
        if self.total_reviews == 0 {
            0.0
        } else {
            self.total_rating / self.total_reviews as f64
        }
    }
    /// Parse rating from JSON string
    pub fn from_json(json_str: &str) -> Option<Self> {
        serde_json::from_str(json_str).ok()
    }
}
/// P2P Order parsed from a Kind 38383 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct P2POrder {
    /// Unique order ID (d-tag)
    pub order_id: String,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// NIP-19 naddr for this order
    pub naddr: String,
    /// Coordinate string: "38383:pubkey:d-tag"
    pub coordinate: String,
    /// Created timestamp
    pub created_at: u64,
    /// Order type: buy or sell
    pub order_type: OrderType,
    /// Fiat currency (ISO 4217 code)
    pub currency: String,
    /// Order status
    pub status: OrderStatus,
    /// Bitcoin amount in satoshis (0 = market rate)
    pub amount_sats: u64,
    /// Fiat amount (fixed or range)
    pub fiat_amount: FiatAmount,
    /// Premium percentage (e.g., 5.0 for +5%)
    pub premium: Option<f64>,
    /// Payment methods accepted
    pub payment_methods: Vec<String>,
    /// Network: mainnet, testnet, signet
    pub network: Network,
    /// Layer: lightning, onchain, liquid
    pub layer: Layer,
    /// Platform that created the order (y tag)
    pub platform: Option<String>,
    /// Source URL for the order
    pub source: Option<String>,
    /// Maker's display name
    pub maker_name: Option<String>,
    /// Maker's rating
    pub rating: Option<Rating>,
    /// Geohash for F2F trades
    pub geohash: Option<String>,
    /// Bond percentage (e.g., 3 for 3%)
    pub bond: Option<f64>,
    /// When the pending order expires (status should change to expired)
    pub expires_at: Option<u64>,
    /// When the relay should delete the event (NIP-40)
    pub expiration: Option<u64>,
}
impl P2POrder {
    /// Check if order has expired based on expires_at timestamp
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            now_secs() > expires_at
        } else {
            self.status == OrderStatus::Expired
        }
    }
    /// Check if order is still active and can be taken
    pub fn is_active(&self) -> bool {
        !self.is_expired() && self.status.is_active()
    }
    /// Get time remaining until expiration (in seconds)
    pub fn time_remaining(&self) -> Option<u64> {
        self.expires_at
            .map(|expires| expires.saturating_sub(now_secs()))
    }
    /// Calculate satoshis at current rate (for premium-based orders)
    pub fn calc_sats_at_rate(&self, exchange_rate: f64) -> u64 {
        if self.amount_sats > 0 {
            return self.amount_sats;
        }
        let fiat = match &self.fiat_amount {
            FiatAmount::Fixed(amt) => *amt,
            FiatAmount::Range { max, .. } => *max,
        };
        let premium = self.premium.unwrap_or(0.0);
        let premium_rate = exchange_rate * (1.0 + premium / 100.0);
        if premium_rate > 0.0 {
            ((fiat / premium_rate) * 100_000_000.0) as u64
        } else {
            0
        }
    }
}
/// Parse a Kind 38383 event into a P2POrder
pub fn parse_p2p_order(event: &Event) -> Result<P2POrder, String> {
    if event.kind.as_u16() != KIND_P2P_ORDER {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_P2P_ORDER,
            event.kind.as_u16()
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let order_id = get_tag_value(event, "d").ok_or("Missing required 'd' tag (order ID)")?;
    let coordinate = format!("{}:{}:{}", KIND_P2P_ORDER, pubkey, order_id);
    let naddr = build_naddr(&pubkey, &order_id).unwrap_or_default();
    let order_type = get_tag_value(event, "k")
        .and_then(|s| OrderType::from_str(&s))
        .ok_or("Missing or invalid 'k' tag (order type: buy/sell)")?;
    let currency = get_tag_value(event, "f").ok_or("Missing required 'f' tag (currency)")?;
    let status = get_tag_value(event, "s")
        .and_then(|s| OrderStatus::from_str(&s))
        .unwrap_or(OrderStatus::Pending);
    let amount_sats = get_tag_value(event, "amt")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let fiat_amount =
        parse_fiat_amount(event).ok_or("Missing or invalid 'fa' tag (fiat amount)")?;
    let premium = get_tag_value(event, "premium").and_then(|s| s.parse::<f64>().ok());
    let payment_methods = get_tag_values(event, "pm");
    let network = get_tag_value(event, "network")
        .and_then(|s| Network::from_str(&s))
        .unwrap_or(Network::Mainnet);
    let layer = get_tag_value(event, "layer")
        .and_then(|s| Layer::from_str(&s))
        .unwrap_or(Layer::Lightning);
    let platform = get_tag_value(event, "y");
    let source = get_tag_value(event, "source");
    let maker_name = get_tag_value(event, "name");
    let rating = get_tag_value(event, "rating").and_then(|s| Rating::from_json(&s));
    let geohash = get_tag_value(event, "g");
    let bond = get_tag_value(event, "bond").and_then(|s| s.parse::<f64>().ok());
    let expires_at = get_tag_value(event, "expires_at").and_then(|s| s.parse::<u64>().ok());
    let expiration = get_tag_value(event, "expiration").and_then(|s| s.parse::<u64>().ok());
    Ok(P2POrder {
        order_id,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
        order_type,
        currency,
        status,
        amount_sats,
        fiat_amount,
        premium,
        payment_methods,
        network,
        layer,
        platform,
        source,
        maker_name,
        rating,
        geohash,
        bond,
        expires_at,
        expiration,
    })
}
/// Get a tag value by name (first parameter after tag name)
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}
/// Get all values of a tag (for multi-value tags like pm)
fn get_tag_values(event: &Event, tag_name: &str) -> Vec<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .map(|t| t.as_slice()[1..].iter().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}
/// Parse fiat amount from fa tag (can be single value or range)
fn parse_fiat_amount(event: &Event) -> Option<FiatAmount> {
    let fa_tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("fa"))?;
    let slice = fa_tag.as_slice();
    if slice.len() >= 3 {
        let min = slice.get(1)?.parse::<f64>().ok()?;
        let max = slice.get(2)?.parse::<f64>().ok()?;
        Some(FiatAmount::Range { min, max })
    } else if slice.len() >= 2 {
        let amount = slice.get(1)?.parse::<f64>().ok()?;
        Some(FiatAmount::Fixed(amount))
    } else {
        None
    }
}
/// Build naddr from pubkey and order_id
fn build_naddr(pubkey: &str, order_id: &str) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coordinate = Coordinate::new(Kind::Custom(KIND_P2P_ORDER), pk).identifier(order_id);
    let nip19 = Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_order_type_from_str() {
        assert_eq!(OrderType::from_str("buy"), Some(OrderType::Buy));
        assert_eq!(OrderType::from_str("SELL"), Some(OrderType::Sell));
        assert_eq!(OrderType::from_str("invalid"), None);
    }
    #[test]
    fn test_order_status_from_str() {
        assert_eq!(OrderStatus::from_str("pending"), Some(OrderStatus::Pending));
        assert_eq!(
            OrderStatus::from_str("in-progress"),
            Some(OrderStatus::InProgress)
        );
        assert_eq!(
            OrderStatus::from_str("cancelled"),
            Some(OrderStatus::Canceled)
        );
    }
    #[test]
    fn test_fiat_amount_display() {
        let fixed = FiatAmount::Fixed(100.0);
        assert_eq!(fixed.display("USD"), "100 USD");
        let range = FiatAmount::Range {
            min: 50.0,
            max: 200.0,
        };
        assert_eq!(range.display("EUR"), "50-200 EUR");
    }
    #[test]
    fn test_fiat_amount_overlaps() {
        let fixed = FiatAmount::Fixed(100.0);
        assert!(fixed.overlaps(100.0, 0.1));
        assert!(fixed.overlaps(95.0, 0.1));
        assert!(!fixed.overlaps(50.0, 0.1));
        let range = FiatAmount::Range {
            min: 50.0,
            max: 150.0,
        };
        assert!(range.overlaps(100.0, 0.1));
        assert!(range.overlaps(45.0, 0.2));
        assert!(!range.overlaps(200.0, 0.1));
    }
    #[test]
    fn test_rating_average() {
        let rating = Rating {
            total_reviews: 10,
            total_rating: 45.0,
            last_rating: 5,
            max_rate: 5,
            min_rate: 1,
        };
        assert_eq!(rating.average(), 4.5);
    }
    #[test]
    fn test_layer_from_str() {
        assert_eq!(Layer::from_str("lightning"), Some(Layer::Lightning));
        assert_eq!(Layer::from_str("ln"), Some(Layer::Lightning));
        assert_eq!(Layer::from_str("onchain"), Some(Layer::Onchain));
        assert_eq!(Layer::from_str("liquid"), Some(Layer::Liquid));
    }
    #[test]
    fn test_network_from_str() {
        assert_eq!(Network::from_str("mainnet"), Some(Network::Mainnet));
        assert_eq!(Network::from_str("testnet"), Some(Network::Testnet));
        assert_eq!(Network::from_str("signet"), Some(Network::Signet));
    }
}
