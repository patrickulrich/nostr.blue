//! NIP-99 Marketplace Protocol Parsing Utilities
//!
//! Handles parsing of marketplace events per the market-spec protocol:
//! - Kind 30402: Product Listing (addressable)
//! - Kind 30405: Product Collection (addressable)
//! - Kind 30406: Shipping Option (addressable)
//! - Kind 31555: Product Review (addressable)
//! - Kind 16: Order Messages (via NIP-17 gift wrap)
//! - Kind 17: Payment Receipts (via NIP-17 gift wrap)
//!
//! This module provides data structures and parsing functions for a full
//! decentralized marketplace implementation on Nostr.

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// Get current Unix timestamp in seconds
/// Uses js_sys for WASM environments, std::time for native builds
pub fn now_secs() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

/// Extract product display name from NIP-99 coordinate (30402:pubkey:d-tag)
///
/// Parses the d-tag from a coordinate string and converts dashes to spaces
/// for human-readable display. Falls back to the full coordinate if malformed.
///
/// # Examples
/// ```
/// let coord = "30402:abc123:vintage-camera";
/// assert_eq!(extract_product_name_from_coordinate(coord), "vintage-camera");
/// ```
pub fn extract_product_name_from_coordinate(coordinate: &str) -> String {
    coordinate
        .split(':')
        .nth(2)
        .map(|s| s.to_string())
        .unwrap_or_else(|| coordinate.to_string())
}

// ============================================================================
// Event Kind Constants
// ============================================================================

/// Product listing (addressable)
pub const KIND_PRODUCT: u16 = 30402;
/// Product collection (addressable)
pub const KIND_COLLECTION: u16 = 30405;
/// Shipping option (addressable)
pub const KIND_SHIPPING: u16 = 30406;
/// Product review (addressable)
pub const KIND_REVIEW: u16 = 31555;
/// Order messages (encrypted via NIP-17)
pub const KIND_ORDER_MESSAGE: u16 = 16;
/// Payment receipts (encrypted via NIP-17)
pub const KIND_PAYMENT_RECEIPT: u16 = 17;

// ============================================================================
// Enums
// ============================================================================

/// Product type classification
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProductType {
    #[default]
    Simple,
    Variable,
    Variation,
}

impl ProductType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProductType::Simple => "simple",
            ProductType::Variable => "variable",
            ProductType::Variation => "variation",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "simple" => Some(ProductType::Simple),
            "variable" => Some(ProductType::Variable),
            "variation" => Some(ProductType::Variation),
            _ => None,
        }
    }
}

impl std::fmt::Display for ProductType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Product format (digital vs physical)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProductFormat {
    #[default]
    Digital,
    Physical,
}

impl ProductFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProductFormat::Digital => "digital",
            ProductFormat::Physical => "physical",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "digital" => Some(ProductFormat::Digital),
            "physical" => Some(ProductFormat::Physical),
            _ => None,
        }
    }

    pub fn is_digital(&self) -> bool {
        matches!(self, ProductFormat::Digital)
    }

    pub fn is_physical(&self) -> bool {
        matches!(self, ProductFormat::Physical)
    }
}

impl std::fmt::Display for ProductFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Product visibility status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProductVisibility {
    Hidden,
    #[default]
    OnSale,
    PreOrder,
}

impl ProductVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProductVisibility::Hidden => "hidden",
            ProductVisibility::OnSale => "on-sale",
            ProductVisibility::PreOrder => "pre-order",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hidden" => Some(ProductVisibility::Hidden),
            "on-sale" | "onsale" => Some(ProductVisibility::OnSale),
            "pre-order" | "preorder" => Some(ProductVisibility::PreOrder),
            _ => None,
        }
    }

    pub fn is_visible(&self) -> bool {
        !matches!(self, ProductVisibility::Hidden)
    }
}

impl std::fmt::Display for ProductVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Shipping service type
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShippingService {
    #[default]
    Standard,
    Express,
    Overnight,
    Pickup,
}

impl ShippingService {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShippingService::Standard => "standard",
            ShippingService::Express => "express",
            ShippingService::Overnight => "overnight",
            ShippingService::Pickup => "pickup",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "standard" => Some(ShippingService::Standard),
            "express" => Some(ShippingService::Express),
            "overnight" => Some(ShippingService::Overnight),
            "pickup" => Some(ShippingService::Pickup),
            _ => None,
        }
    }
}

impl std::fmt::Display for ShippingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Order status from marketplace spec
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    #[default]
    Pending,
    Confirmed,
    Processing,
    Completed,
    Cancelled,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "pending",
            OrderStatus::Confirmed => "confirmed",
            OrderStatus::Processing => "processing",
            OrderStatus::Completed => "completed",
            OrderStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(OrderStatus::Pending),
            "confirmed" => Some(OrderStatus::Confirmed),
            "processing" => Some(OrderStatus::Processing),
            "completed" => Some(OrderStatus::Completed),
            "cancelled" | "canceled" => Some(OrderStatus::Cancelled),
            _ => None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            OrderStatus::Pending | OrderStatus::Confirmed | OrderStatus::Processing
        )
    }
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Shipping status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShippingStatus {
    #[default]
    Processing,
    Shipped,
    Delivered,
    Exception,
}

impl ShippingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShippingStatus::Processing => "processing",
            ShippingStatus::Shipped => "shipped",
            ShippingStatus::Delivered => "delivered",
            ShippingStatus::Exception => "exception",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "processing" => Some(ShippingStatus::Processing),
            "shipped" => Some(ShippingStatus::Shipped),
            "delivered" => Some(ShippingStatus::Delivered),
            "exception" => Some(ShippingStatus::Exception),
            _ => None,
        }
    }
}

impl std::fmt::Display for ShippingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Order message type (for Kind 16)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderMessageType {
    OrderCreation = 1,
    PaymentRequest = 2,
    StatusUpdate = 3,
    ShippingUpdate = 4,
    Message = 5,
}

impl OrderMessageType {
    /// Parse from numeric type value (per market-spec protocol)
    /// Used when receiving messages encoded with numeric type IDs
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(OrderMessageType::OrderCreation),
            2 => Some(OrderMessageType::PaymentRequest),
            3 => Some(OrderMessageType::StatusUpdate),
            4 => Some(OrderMessageType::ShippingUpdate),
            5 => Some(OrderMessageType::Message),
            _ => None,
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "order" | "order_creation" => Some(OrderMessageType::OrderCreation),
            "payment" | "payment_request" => Some(OrderMessageType::PaymentRequest),
            "status" | "status_update" => Some(OrderMessageType::StatusUpdate),
            "shipping" | "shipping_update" => Some(OrderMessageType::ShippingUpdate),
            "message" => Some(OrderMessageType::Message),
            _ => None,
        }
    }

    /// Convert to string for JSON serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderMessageType::OrderCreation => "order",
            OrderMessageType::PaymentRequest => "payment",
            OrderMessageType::StatusUpdate => "status",
            OrderMessageType::ShippingUpdate => "shipping",
            OrderMessageType::Message => "message",
        }
    }
}

impl std::fmt::Display for OrderMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Data Structures
// ============================================================================

/// Product price with currency and optional subscription frequency
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductPrice {
    /// Price amount (decimal)
    pub amount: f64,
    /// Currency code (ISO 4217 or "sats")
    pub currency: String,
    /// Optional subscription frequency (ISO 8601 duration: D, W, M, Y)
    pub frequency: Option<String>,
}

impl ProductPrice {
    pub fn new(amount: f64, currency: &str) -> Self {
        Self {
            amount,
            currency: currency.to_uppercase(),
            frequency: None,
        }
    }

    pub fn with_frequency(amount: f64, currency: &str, frequency: &str) -> Self {
        Self {
            amount,
            currency: currency.to_uppercase(),
            frequency: Some(frequency.to_string()),
        }
    }

    /// Display price with currency
    pub fn display(&self) -> String {
        if self.currency.eq_ignore_ascii_case("sats") || self.currency.eq_ignore_ascii_case("sat") {
            format!("{:.0} sats", self.amount)
        } else {
            format!("{:.2} {}", self.amount, self.currency)
        }
    }

    /// Check if this is a subscription product
    pub fn is_subscription(&self) -> bool {
        self.frequency.is_some()
    }

    /// Get price in satoshis
    /// Returns the amount directly if currency is sats/sat
    /// Converts from fiat using exchange rate if available
    /// Returns None if conversion is not possible (no exchange rate)
    pub fn to_sats(&self) -> Option<u64> {
        if self.currency.eq_ignore_ascii_case("sats") || self.currency.eq_ignore_ascii_case("sat") {
            return Some(self.amount as u64);
        }
        // Try to convert from fiat using exchange rate
        crate::services::btc_price::fiat_to_sats(self.amount, &self.currency)
    }

    /// Check if this price is in sats (no conversion needed)
    pub fn is_sats(&self) -> bool {
        self.currency.eq_ignore_ascii_case("sats") || self.currency.eq_ignore_ascii_case("sat")
    }
}

impl Default for ProductPrice {
    fn default() -> Self {
        Self {
            amount: 0.0,
            currency: "USD".to_string(),
            frequency: None,
        }
    }
}

/// Product image with optional dimensions and sorting order
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductImage {
    pub url: String,
    pub dimensions: Option<String>,
    pub sort_order: Option<i32>,
}

/// Product specification (key-value pair)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductSpec {
    pub key: String,
    pub value: String,
}

/// Product parsed from Kind 30402 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Product {
    // Core identifiers
    /// Unique product identifier (d-tag)
    pub d_tag: String,
    /// Event ID (hex)
    pub event_id: String,
    /// Merchant pubkey (hex)
    pub pubkey: String,
    /// NIP-19 naddr for this product
    pub naddr: String,
    /// Coordinate string: "30402:pubkey:d-tag"
    pub coordinate: String,
    /// Created timestamp
    pub created_at: u64,

    // Required fields
    /// Product title
    pub title: String,
    /// Price information
    pub price: ProductPrice,

    // Optional fields
    /// Product description (markdown content)
    pub description: Option<String>,
    /// Short summary
    pub summary: Option<String>,
    /// Product type (simple/variable/variation)
    pub product_type: ProductType,
    /// Product format (digital/physical)
    pub format: ProductFormat,
    /// Visibility status
    pub visibility: ProductVisibility,
    /// Stock quantity
    pub stock: Option<u32>,
    /// Product images
    pub images: Vec<ProductImage>,
    /// Product specifications
    pub specs: Vec<ProductSpec>,
    /// Category tags
    pub categories: Vec<String>,
    /// Shipping option references (coordinates)
    pub shipping_options: Vec<String>,
    /// Collection references (coordinates)
    pub collections: Vec<String>,
    /// Parent product reference (for variations)
    pub parent_product: Option<String>,
    /// Location string
    pub location: Option<String>,
    /// Geohash
    pub geohash: Option<String>,
    /// Weight [value, unit]
    pub weight: Option<(f64, String)>,
    /// Dimensions [l×w×h, unit]
    pub dimensions: Option<(String, String)>,
    /// Product condition (new, like_new, used, fair, refurbished)
    pub condition: Option<String>,

    // Digital delivery fields
    /// Download URL for digital products (sent after purchase)
    pub download_url: Option<String>,
    /// License key for software products
    pub license_key: Option<String>,
    /// Content hash for verification (sha256)
    pub content_hash: Option<String>,
    /// File size in bytes
    pub file_size: Option<u64>,
    /// MIME type (e.g., "application/pdf", "video/mp4")
    pub file_type: Option<String>,
}

impl Product {
    /// Check if product is in stock
    pub fn is_in_stock(&self) -> bool {
        self.stock.map(|s| s > 0).unwrap_or(true)
    }

    /// Check if product is visible
    pub fn is_visible(&self) -> bool {
        self.visibility.is_visible()
    }

    /// Check if product requires shipping
    pub fn requires_shipping(&self) -> bool {
        self.format.is_physical()
    }

    /// Check if this is a digital product with delivery content
    pub fn has_digital_delivery(&self) -> bool {
        self.format == ProductFormat::Digital &&
            (self.download_url.is_some() || self.license_key.is_some())
    }

    /// Get download info for display
    pub fn get_download_info(&self) -> Option<String> {
        if let Some(ref file_type) = self.file_type {
            let size_str = self.file_size
                .map(format_file_size)
                .unwrap_or_default();
            if size_str.is_empty() {
                Some(file_type.clone())
            } else {
                Some(format!("{} - {}", file_type, size_str))
            }
        } else {
            self.file_size.map(format_file_size)
        }
    }
}

/// Format file size in human-readable format
fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Product Collection parsed from Kind 30405 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductCollection {
    // Identifiers
    pub d_tag: String,
    pub event_id: String,
    pub pubkey: String,
    pub naddr: String,
    pub coordinate: String,
    pub created_at: u64,

    // Required fields
    pub title: String,
    pub products: Vec<String>, // Product coordinates

    // Optional fields
    pub description: Option<String>,
    pub summary: Option<String>,
    pub image: Option<String>,
    pub location: Option<String>,
    pub geohash: Option<String>,
    pub shipping_options: Vec<String>,
}

/// Shipping Option parsed from Kind 30406 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShippingOption {
    // Identifiers
    pub d_tag: String,
    pub event_id: String,
    pub pubkey: String,
    pub naddr: String,
    pub coordinate: String,
    pub created_at: u64,

    // Required fields
    pub title: String,
    pub base_price: f64,
    pub currency: String,
    pub countries: Vec<String>, // ISO 3166-1 alpha-2 codes
    pub service: ShippingService,

    // Optional fields
    pub description: Option<String>,
    pub carrier: Option<String>,
    pub regions: Vec<String>, // ISO 3166-2 codes
    pub duration_min: Option<u32>,
    pub duration_max: Option<u32>,
    pub duration_unit: Option<String>, // H, D, W
    pub location: Option<String>,
    pub geohash: Option<String>,
    pub weight_min: Option<(f64, String)>,
    pub weight_max: Option<(f64, String)>,
    pub dim_min: Option<(String, String)>,
    pub dim_max: Option<(String, String)>,
    pub price_per_weight: Option<(f64, String)>,
    pub price_per_volume: Option<(f64, String)>,
    pub price_per_distance: Option<(f64, String)>,
}

impl ShippingOption {
    /// Display price with currency
    pub fn display_price(&self) -> String {
        format!("{:.2} {}", self.base_price, self.currency)
    }

    /// Display delivery estimate
    pub fn display_duration(&self) -> Option<String> {
        match (self.duration_min, self.duration_max, &self.duration_unit) {
            (Some(min), Some(max), Some(unit)) => {
                let unit_name = match unit.as_str() {
                    "H" => "hours",
                    "D" => "days",
                    "W" => "weeks",
                    _ => unit.as_str(),
                };
                Some(format!("{}-{} {}", min, max, unit_name))
            }
            _ => None,
        }
    }
}

/// Product Review parsed from Kind 31555 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductReview {
    // Identifiers
    pub d_tag: String,
    pub event_id: String,
    pub reviewer_pubkey: String,
    pub created_at: u64,

    // Required fields
    pub product_coordinate: String,
    pub thumb_rating: f64, // 0 or 1

    // Optional fields
    pub content: String,
    pub value_rating: Option<f64>,
    pub quality_rating: Option<f64>,
    pub delivery_rating: Option<f64>,
    pub communication_rating: Option<f64>,
    pub custom_ratings: Vec<(String, f64)>,
}

impl ProductReview {
    /// Calculate total score per market-spec formula
    /// Total = (Thumb × 0.5) + (0.5 × (Σ(Category Ratings) ÷ Number of Categories))
    pub fn total_score(&self) -> f64 {
        let mut category_sum = 0.0;
        let mut category_count = 0;

        if let Some(v) = self.value_rating {
            category_sum += v;
            category_count += 1;
        }
        if let Some(v) = self.quality_rating {
            category_sum += v;
            category_count += 1;
        }
        if let Some(v) = self.delivery_rating {
            category_sum += v;
            category_count += 1;
        }
        if let Some(v) = self.communication_rating {
            category_sum += v;
            category_count += 1;
        }
        for (_, v) in &self.custom_ratings {
            category_sum += v;
            category_count += 1;
        }

        if category_count == 0 {
            self.thumb_rating
        } else {
            (self.thumb_rating * 0.5) + (0.5 * (category_sum / category_count as f64))
        }
    }

    /// Convert 0-1 score to 5-star rating
    pub fn as_stars(&self) -> f64 {
        self.total_score() * 5.0
    }
}

/// Order item in an order
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderItem {
    pub product_coordinate: String,
    pub quantity: u32,
}

/// Shopping cart item
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CartItem {
    pub product: Product,
    pub quantity: u32,
    pub selected_shipping: Option<String>,
}

impl CartItem {
    /// Calculate line total in original currency
    pub fn line_total(&self) -> f64 {
        self.product.price.amount * self.quantity as f64
    }
}

/// Shop Order
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShopOrder {
    // Identifiers
    pub order_id: String,
    pub buyer_pubkey: String,
    pub merchant_pubkey: String,

    // Order details
    pub items: Vec<OrderItem>,
    pub amount_sats: u64,
    pub status: OrderStatus,
    pub shipping_status: Option<ShippingStatus>,

    // Shipping
    pub shipping_option: Option<String>,
    pub shipping_address: Option<String>,
    pub tracking_number: Option<String>,
    pub carrier: Option<String>,

    // Contact
    pub email: Option<String>,
    pub phone: Option<String>,

    // Timestamps
    pub created_at: u64,
    pub updated_at: u64,
    pub paid_at: Option<u64>,
    pub shipped_at: Option<u64>,
    pub delivered_at: Option<u64>,

    // Notes
    pub note: Option<String>,

    // Digital delivery (populated when merchant sends delivery info)
    pub download_url: Option<String>,
    pub license_key: Option<String>,
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse a Kind 30402 event into a Product
pub fn parse_product(event: &Event) -> Result<Product, String> {
    if event.kind.as_u16() != KIND_PRODUCT {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_PRODUCT,
            event.kind.as_u16()
        ));
    }

    let pubkey = event.pubkey.to_hex();

    // Required: d-tag
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;

    // Required: title
    let title = get_tag_value(event, "title").ok_or("Missing required 'title' tag")?;

    // Required: price [amount, currency, frequency?]
    let price = parse_price_tag(event).ok_or("Missing or invalid 'price' tag")?;

    // Build coordinate and naddr
    let coordinate = format!("{}:{}:{}", KIND_PRODUCT, pubkey, d_tag);
    let naddr = build_naddr(&pubkey, &d_tag, KIND_PRODUCT).unwrap_or_default();

    // Optional: type [type, format]
    let (product_type, format) = parse_type_tag(event);

    // Optional: visibility
    let visibility = get_tag_value(event, "visibility")
        .and_then(|s| ProductVisibility::from_str(&s))
        .unwrap_or_default();

    // Optional: stock
    let stock = get_tag_value(event, "stock").and_then(|s| s.parse().ok());

    // Optional: summary
    let summary = get_tag_value(event, "summary");

    // Optional: images
    let images = parse_image_tags(event);

    // Optional: specs
    let specs = parse_spec_tags(event);

    // Optional: categories (t tags)
    let categories = get_all_tag_values(event, "t");

    // Optional: shipping_option tags
    let shipping_options = get_all_tag_values(event, "shipping_option");

    // Optional: collection references (a tags starting with 30405:)
    let collections = get_all_tag_values(event, "a")
        .into_iter()
        .filter(|a| a.starts_with("30405:"))
        .collect();

    // Optional: parent product reference (a tag starting with 30402:)
    let parent_product = get_all_tag_values(event, "a")
        .into_iter()
        .find(|a| a.starts_with("30402:"));

    // Optional: location
    let location = get_tag_value(event, "location");

    // Optional: geohash
    let geohash = get_tag_value(event, "g");

    // Optional: weight
    let weight = parse_weight_tag(event, "weight");

    // Optional: dimensions
    let dimensions = parse_dim_tag(event);

    // Optional: condition
    let condition = get_tag_value(event, "condition");

    // Digital delivery fields
    let download_url = get_tag_value(event, "download");
    let license_key = get_tag_value(event, "license");
    let content_hash = parse_hash_tag(event, "sha256");
    let file_size = get_tag_value(event, "size").and_then(|s| s.parse().ok());
    let file_type = get_tag_value(event, "type")
        .or_else(|| get_tag_value(event, "mime"));

    Ok(Product {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
        title,
        price,
        description: Some(event.content.clone()).filter(|s| !s.is_empty()),
        summary,
        product_type,
        format,
        visibility,
        stock,
        images,
        specs,
        categories,
        shipping_options,
        collections,
        parent_product,
        location,
        geohash,
        weight,
        dimensions,
        condition,
        download_url,
        license_key,
        content_hash,
        file_size,
        file_type,
    })
}

/// Parse a Kind 30405 event into a ProductCollection
pub fn parse_collection(event: &Event) -> Result<ProductCollection, String> {
    if event.kind.as_u16() != KIND_COLLECTION {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_COLLECTION,
            event.kind.as_u16()
        ));
    }

    let pubkey = event.pubkey.to_hex();

    // Required: d-tag
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;

    // Required: title
    let title = get_tag_value(event, "title").ok_or("Missing required 'title' tag")?;

    // Required: product references (a tags)
    let products = get_all_tag_values(event, "a")
        .into_iter()
        .filter(|a| a.starts_with("30402:"))
        .collect();

    // Build coordinate and naddr
    let coordinate = format!("{}:{}:{}", KIND_COLLECTION, pubkey, d_tag);
    let naddr = build_naddr(&pubkey, &d_tag, KIND_COLLECTION).unwrap_or_default();

    Ok(ProductCollection {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
        title,
        products,
        description: Some(event.content.clone()).filter(|s| !s.is_empty()),
        summary: get_tag_value(event, "summary"),
        image: get_tag_value(event, "image"),
        location: get_tag_value(event, "location"),
        geohash: get_tag_value(event, "g"),
        shipping_options: get_all_tag_values(event, "shipping_option"),
    })
}

/// Parse a Kind 30406 event into a ShippingOption
pub fn parse_shipping(event: &Event) -> Result<ShippingOption, String> {
    if event.kind.as_u16() != KIND_SHIPPING {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_SHIPPING,
            event.kind.as_u16()
        ));
    }

    let pubkey = event.pubkey.to_hex();

    // Required: d-tag
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;

    // Required: title
    let title = get_tag_value(event, "title").ok_or("Missing required 'title' tag")?;

    // Required: price [base_cost, currency]
    let (base_price, currency) =
        parse_shipping_price_tag(event).ok_or("Missing or invalid 'price' tag")?;

    // Required: country
    let countries = get_tag_values(event, "country");
    if countries.is_empty() {
        return Err("Missing required 'country' tag".to_string());
    }

    // Required: service
    let service = get_tag_value(event, "service")
        .and_then(|s| ShippingService::from_str(&s))
        .ok_or("Missing or invalid 'service' tag")?;

    // Build coordinate and naddr
    let coordinate = format!("{}:{}:{}", KIND_SHIPPING, pubkey, d_tag);
    let naddr = build_naddr(&pubkey, &d_tag, KIND_SHIPPING).unwrap_or_default();

    // Optional: duration [min, max, unit]
    let (duration_min, duration_max, duration_unit) = parse_duration_tag(event);

    Ok(ShippingOption {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
        title,
        base_price,
        currency,
        countries,
        service,
        description: Some(event.content.clone()).filter(|s| !s.is_empty()),
        carrier: get_tag_value(event, "carrier"),
        regions: get_tag_values(event, "region"),
        duration_min,
        duration_max,
        duration_unit,
        location: get_tag_value(event, "location"),
        geohash: get_tag_value(event, "g"),
        weight_min: parse_weight_tag(event, "weight-min"),
        weight_max: parse_weight_tag(event, "weight-max"),
        dim_min: parse_dim_tag_named(event, "dim-min"),
        dim_max: parse_dim_tag_named(event, "dim-max"),
        price_per_weight: parse_price_unit_tag(event, "price-weight"),
        price_per_volume: parse_price_unit_tag(event, "price-volume"),
        price_per_distance: parse_price_unit_tag(event, "price-distance"),
    })
}

/// Parse a Kind 31555 event into a ProductReview
pub fn parse_review(event: &Event) -> Result<ProductReview, String> {
    if event.kind.as_u16() != KIND_REVIEW {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_REVIEW,
            event.kind.as_u16()
        ));
    }

    // Required: d-tag (product reference)
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;

    // The d-tag should be "a:30402:pubkey:product-d-tag"
    let product_coordinate = if let Some(stripped) = d_tag.strip_prefix("a:") {
        stripped.to_string()
    } else {
        d_tag.clone()
    };

    // Required: rating with "thumb" label
    let thumb_rating = parse_rating_tag(event, "thumb").ok_or("Missing required 'thumb' rating")?;

    // Optional category ratings
    let value_rating = parse_rating_tag(event, "value");
    let quality_rating = parse_rating_tag(event, "quality");
    let delivery_rating = parse_rating_tag(event, "delivery");
    let communication_rating = parse_rating_tag(event, "communication");

    // Custom ratings
    let custom_ratings = parse_custom_ratings(event);

    Ok(ProductReview {
        d_tag,
        event_id: event.id.to_hex(),
        reviewer_pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        product_coordinate,
        thumb_rating,
        content: event.content.clone(),
        value_rating,
        quality_rating,
        delivery_rating,
        communication_rating,
        custom_ratings,
    })
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get a tag value by name (first value after tag name)
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}

/// Get all values from a single tag (for multi-value tags like country)
fn get_tag_values(event: &Event, tag_name: &str) -> Vec<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .map(|t| t.as_slice()[1..].iter().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

/// Get values from all tags with the same name (for repeatable tags like image, t)
fn get_all_tag_values(event: &Event, tag_name: &str) -> Vec<String> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect()
}

/// Parse price tag: ["price", amount, currency, frequency?]
fn parse_price_tag(event: &Event) -> Option<ProductPrice> {
    let tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("price"))?;

    let slice = tag.as_slice();
    let amount = slice.get(1)?.parse::<f64>().ok()?;
    let currency = slice.get(2)?;
    let frequency = slice.get(3).filter(|s| !s.is_empty());

    Some(match frequency {
        Some(freq) => ProductPrice::with_frequency(amount, currency, freq),
        None => ProductPrice::new(amount, currency),
    })
}

/// Parse shipping price tag: ["price", base_cost, currency]
fn parse_shipping_price_tag(event: &Event) -> Option<(f64, String)> {
    let tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("price"))?;

    let slice = tag.as_slice();
    let base_price = slice.get(1)?.parse::<f64>().ok()?;
    let currency = slice.get(2)?.to_string();

    Some((base_price, currency))
}

/// Parse type tag: ["type", type, format]
fn parse_type_tag(event: &Event) -> (ProductType, ProductFormat) {
    let tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("type"));

    match tag {
        Some(t) => {
            let slice = t.as_slice();
            let product_type = slice
                .get(1)
                .and_then(|s| ProductType::from_str(s))
                .unwrap_or_default();
            let format = slice
                .get(2)
                .and_then(|s| ProductFormat::from_str(s))
                .unwrap_or_default();
            (product_type, format)
        }
        None => (ProductType::default(), ProductFormat::default()),
    }
}

/// Parse image tags: ["image", url, dimensions?, sort_order?]
fn parse_image_tags(event: &Event) -> Vec<ProductImage> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("image"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let url = slice.get(1)?.to_string();
            let dimensions = slice
                .get(2)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let sort_order = slice.get(3).and_then(|s| s.parse().ok());
            Some(ProductImage {
                url,
                dimensions,
                sort_order,
            })
        })
        .collect()
}

/// Parse spec tags: ["spec", key, value]
fn parse_spec_tags(event: &Event) -> Vec<ProductSpec> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("spec"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let key = slice.get(1)?.to_string();
            let value = slice.get(2)?.to_string();
            Some(ProductSpec { key, value })
        })
        .collect()
}

/// Parse weight tag: ["weight", value, unit]
fn parse_weight_tag(event: &Event, tag_name: &str) -> Option<(f64, String)> {
    let tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))?;

    let slice = tag.as_slice();
    let value = slice.get(1)?.parse::<f64>().ok()?;
    let unit = slice.get(2)?.to_string();

    Some((value, unit))
}

/// Parse dim tag: ["dim", l×w×h, unit]
fn parse_dim_tag(event: &Event) -> Option<(String, String)> {
    parse_dim_tag_named(event, "dim")
}

fn parse_dim_tag_named(event: &Event, tag_name: &str) -> Option<(String, String)> {
    let tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))?;

    let slice = tag.as_slice();
    let dims = slice.get(1)?.to_string();
    let unit = slice.get(2)?.to_string();

    Some((dims, unit))
}

/// Parse hash tag: ["hash", algorithm, value] -> returns value if algorithm matches
fn parse_hash_tag(event: &Event, algorithm: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("hash") &&
                slice.get(1).map(|s| s.as_str()) == Some(algorithm)
        })
        .and_then(|t| t.as_slice().get(2).map(|s| s.to_string()))
}

/// Parse duration tag: ["duration", min, max, unit]
fn parse_duration_tag(event: &Event) -> (Option<u32>, Option<u32>, Option<String>) {
    let tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("duration"));

    match tag {
        Some(t) => {
            let slice = t.as_slice();
            let min = slice.get(1).and_then(|s| s.parse().ok());
            let max = slice.get(2).and_then(|s| s.parse().ok());
            let unit = slice.get(3).map(|s| s.to_string());
            (min, max, unit)
        }
        None => (None, None, None),
    }
}

/// Parse price-per-unit tag: ["price-weight", price, unit]
fn parse_price_unit_tag(event: &Event, tag_name: &str) -> Option<(f64, String)> {
    let tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))?;

    let slice = tag.as_slice();
    let price = slice.get(1)?.parse::<f64>().ok()?;
    let unit = slice.get(2)?.to_string();

    Some((price, unit))
}

/// Parse rating tag with specific category: ["rating", score, category]
fn parse_rating_tag(event: &Event, category: &str) -> Option<f64> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("rating"))
        .find(|t| t.as_slice().get(2).map(|s| s.as_str()) == Some(category))
        .and_then(|t| t.as_slice().get(1)?.parse::<f64>().ok())
}

/// Parse custom rating tags (not thumb, value, quality, delivery, communication)
fn parse_custom_ratings(event: &Event) -> Vec<(String, f64)> {
    let standard_categories = ["thumb", "value", "quality", "delivery", "communication"];

    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("rating"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let score = slice.get(1)?.parse::<f64>().ok()?;
            let category = slice.get(2)?.to_string();

            if standard_categories.contains(&category.as_str()) {
                None
            } else {
                Some((category, score))
            }
        })
        .collect()
}

/// Build naddr from pubkey and d-tag
fn build_naddr(pubkey: &str, d_tag: &str, kind: u16) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coordinate = Coordinate::new(Kind::Custom(kind), pk).identifier(d_tag);
    let nip19 = Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_product_type_from_str() {
        assert_eq!(ProductType::from_str("simple"), Some(ProductType::Simple));
        assert_eq!(
            ProductType::from_str("variable"),
            Some(ProductType::Variable)
        );
        assert_eq!(
            ProductType::from_str("variation"),
            Some(ProductType::Variation)
        );
    }

    #[test]
    fn test_product_format_from_str() {
        assert_eq!(
            ProductFormat::from_str("digital"),
            Some(ProductFormat::Digital)
        );
        assert_eq!(
            ProductFormat::from_str("physical"),
            Some(ProductFormat::Physical)
        );
    }

    #[test]
    fn test_product_price_display() {
        let price = ProductPrice::new(100.0, "USD");
        assert_eq!(price.display(), "100.00 USD");

        let sats_price = ProductPrice::new(10000.0, "sats");
        assert_eq!(sats_price.display(), "10000 sats");
    }

    #[test]
    fn test_order_status_from_str() {
        assert_eq!(OrderStatus::from_str("pending"), Some(OrderStatus::Pending));
        assert_eq!(
            OrderStatus::from_str("confirmed"),
            Some(OrderStatus::Confirmed)
        );
        assert_eq!(
            OrderStatus::from_str("cancelled"),
            Some(OrderStatus::Cancelled)
        );
        assert_eq!(
            OrderStatus::from_str("canceled"),
            Some(OrderStatus::Cancelled)
        );
    }

    #[test]
    fn test_review_total_score() {
        let review = ProductReview {
            d_tag: "a:30402:test:product".to_string(),
            event_id: "test".to_string(),
            reviewer_pubkey: "test".to_string(),
            created_at: 0,
            product_coordinate: "30402:test:product".to_string(),
            thumb_rating: 1.0,
            content: "Great product!".to_string(),
            value_rating: Some(0.8),
            quality_rating: Some(1.0),
            delivery_rating: Some(0.6),
            communication_rating: Some(0.9),
            custom_ratings: vec![],
        };

        // (1.0 * 0.5) + (0.5 * ((0.8 + 1.0 + 0.6 + 0.9) / 4))
        // = 0.5 + (0.5 * 0.825)
        // = 0.5 + 0.4125
        // = 0.9125
        assert!((review.total_score() - 0.9125).abs() < 0.001);
    }

    #[test]
    fn test_shipping_service_from_str() {
        assert_eq!(
            ShippingService::from_str("standard"),
            Some(ShippingService::Standard)
        );
        assert_eq!(
            ShippingService::from_str("express"),
            Some(ShippingService::Express)
        );
        assert_eq!(
            ShippingService::from_str("pickup"),
            Some(ShippingService::Pickup)
        );
    }
}
