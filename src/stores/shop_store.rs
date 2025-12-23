//! Shop Store
//! Handles NIP-99 Marketplace events - products, collections, orders, and cart management

use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use nostr::Event as NostrEvent;

// Use std::result::Result to avoid ambiguity between dioxus and nostr_sdk
type Result<T> = std::result::Result<T, String>;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;

use serde::{Serialize, Deserialize};
use crate::stores::nostr_client;
use crate::stores::indexeddb_database::IndexedDbDatabase;
use crate::utils::nip99::{
    parse_product, parse_collection, parse_shipping, parse_review, now_secs,
    Product, ProductCollection, ShippingOption, ProductReview, CartItem, ShopOrder,
    OrderItem, OrderStatus, ShippingStatus, ProductVisibility, ProductFormat,
    OrderMessageType, KIND_PRODUCT, KIND_COLLECTION, KIND_SHIPPING, KIND_REVIEW,
    KIND_ORDER_MESSAGE, KIND_PAYMENT_RECEIPT,
};
use std::sync::Arc;

// ============================================================================
// Cache sizes
// ============================================================================

const PRODUCT_CACHE_SIZE: usize = 500;
const SHIPPING_CACHE_SIZE: usize = 100;
const PROCESSED_EVENTS_CACHE_SIZE: usize = 1000;

// ============================================================================
// Global Signals - Products
// ============================================================================

/// Product cache (keyed by naddr string)
pub static PRODUCTS_CACHE: GlobalSignal<LruCache<String, Product>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PRODUCT_CACHE_SIZE).unwrap()));

/// Whether the shop store has been initialized
pub static SHOP_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Currently loading products
pub static LOADING_PRODUCTS: GlobalSignal<bool> = GlobalSignal::new(|| false);

// ============================================================================
// Global Signals - Shipping Options
// ============================================================================

/// Shipping options cache (keyed by naddr string)
pub static SHIPPING_CACHE: GlobalSignal<LruCache<String, ShippingOption>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(SHIPPING_CACHE_SIZE).unwrap()));

// ============================================================================
// Global Signals - Reviews
// ============================================================================

/// Reviews cache (keyed by product coordinate -> Vec<Review>)
pub static REVIEWS_CACHE: GlobalSignal<HashMap<String, Vec<ProductReview>>> =
    GlobalSignal::new(HashMap::new);

// ============================================================================
// Global Signals - Shopping Cart
// ============================================================================

/// Shopping cart items (local, not persisted to Nostr)
pub static CART_ITEMS: GlobalSignal<Vec<CartItem>> = GlobalSignal::new(Vec::new);

/// Cart total in sats (computed from items)
pub static CART_TOTAL_SATS: GlobalSignal<u64> = GlobalSignal::new(|| 0);

// ============================================================================
// Global Signals - Orders
// ============================================================================

/// Orders where current user is the buyer
pub static BUYER_ORDERS: GlobalSignal<Vec<ShopOrder>> = GlobalSignal::new(Vec::new);

/// Orders where current user is the merchant/seller
pub static SELLER_ORDERS: GlobalSignal<Vec<ShopOrder>> = GlobalSignal::new(Vec::new);

/// IndexedDB database handle for order persistence
pub static SHOP_DB: GlobalSignal<Option<Arc<IndexedDbDatabase>>> = GlobalSignal::new(|| None);

/// Processed gift wrap event IDs to prevent duplicate order message processing
/// This prevents the same order update from being applied multiple times
/// Uses LRU cache to prevent unbounded memory growth
pub static PROCESSED_ORDER_EVENTS: GlobalSignal<LruCache<String, ()>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PROCESSED_EVENTS_CACHE_SIZE).unwrap()));

// ============================================================================
// Global Signals - Merchant
// ============================================================================

/// Current user's products (for merchant dashboard)
pub static MY_PRODUCTS: GlobalSignal<Vec<Product>> = GlobalSignal::new(Vec::new);

/// Loading state for my products
pub static LOADING_MY_PRODUCTS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Current user's collections (Kind 30405)
pub static MY_COLLECTIONS: GlobalSignal<Vec<ProductCollection>> = GlobalSignal::new(Vec::new);

/// Loading state for collections
pub static LOADING_MY_COLLECTIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);

// ============================================================================
// Product Functions
// ============================================================================

/// Get a product from cache by naddr
pub fn get_cached_product(naddr: &str) -> Option<Product> {
    PRODUCTS_CACHE.read().peek(naddr).cloned()
}

/// Cache a product
pub fn cache_product(product: Product) {
    PRODUCTS_CACHE.write().put(product.naddr.clone(), product);
}

/// Parse and cache products from events
pub fn cache_product_events(events: &[NostrEvent]) {
    let mut cache = PRODUCTS_CACHE.write();
    for event in events {
        if let Ok(product) = parse_product(event) {
            cache.put(product.naddr.clone(), product);
        }
    }
}

// ============================================================================
// Shipping Functions
// ============================================================================

/// Get a shipping option from cache by naddr
pub fn get_cached_shipping(naddr: &str) -> Option<ShippingOption> {
    SHIPPING_CACHE.read().peek(naddr).cloned()
}

/// Cache a shipping option
pub fn cache_shipping(shipping: ShippingOption) {
    SHIPPING_CACHE.write().put(shipping.naddr.clone(), shipping);
}

/// Fetch shipping options by coordinates (e.g., "30406:pubkey:d-tag")
/// Returns parsed ShippingOption objects with display methods available
pub async fn fetch_shipping_options(coordinates: &[String]) -> Result<Vec<ShippingOption>> {
    if coordinates.is_empty() {
        return Ok(vec![]);
    }

    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    // Build filters for each shipping coordinate
    let mut shipping_options = Vec::new();

    for coord in coordinates {
        // Parse coordinate: "30406:pubkey:d-tag"
        let parts: Vec<&str> = coord.split(':').collect();
        if parts.len() >= 3 && parts[0] == "30406" {
            if let Ok(pubkey) = PublicKey::from_hex(parts[1]) {
                let d_tag = parts[2..].join(":");
                let filter = Filter::new()
                    .kind(Kind::Custom(KIND_SHIPPING))
                    .author(pubkey)
                    .identifier(&d_tag)
                    .limit(1);

                if let Ok(events) = client.fetch_events(filter, Duration::from_secs(5)).await {
                    for event in events.iter() {
                        if let Ok(shipping) = parse_shipping(event) {
                            // Cache it for later
                            cache_shipping(shipping.clone());
                            shipping_options.push(shipping);
                        }
                    }
                }
            }
        }
    }

    Ok(shipping_options)
}

// ============================================================================
// Review Functions
// ============================================================================

/// Get reviews for a product by coordinate
pub fn get_product_reviews(product_coordinate: &str) -> Vec<ProductReview> {
    REVIEWS_CACHE
        .read()
        .get(product_coordinate)
        .cloned()
        .unwrap_or_default()
}

/// Cache a review
pub fn cache_review(review: ProductReview) {
    let mut cache = REVIEWS_CACHE.write();
    let reviews = cache
        .entry(review.product_coordinate.clone())
        .or_default();

    // Avoid duplicates
    if !reviews.iter().any(|r| r.event_id == review.event_id) {
        reviews.push(review);
        // Sort by created_at descending
        reviews.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    }
}

/// Parse and cache reviews from events
pub fn cache_review_events(events: &[NostrEvent]) {
    for event in events {
        if let Ok(review) = parse_review(event) {
            cache_review(review);
        }
    }
}

/// Calculate average rating for a product
pub fn get_product_average_rating(product_coordinate: &str) -> Option<f64> {
    let reviews = get_product_reviews(product_coordinate);
    if reviews.is_empty() {
        return None;
    }
    let total: f64 = reviews.iter().map(|r| r.total_score()).sum();
    Some(total / reviews.len() as f64)
}

// ============================================================================
// Cart Functions
// ============================================================================

/// Add a product to the cart
pub fn add_to_cart(product: Product, quantity: u32) {
    let mut items = CART_ITEMS.write();

    // Check if already in cart
    if let Some(existing) = items.iter_mut().find(|item| item.product.naddr == product.naddr) {
        existing.quantity += quantity;
    } else {
        items.push(CartItem {
            product,
            quantity,
            selected_shipping: None,
        });
    }

    drop(items);
    recalculate_cart_total();
}

/// Update quantity for a cart item
pub fn update_cart_quantity(naddr: &str, quantity: u32) {
    let mut items = CART_ITEMS.write();

    if quantity == 0 {
        items.retain(|item| item.product.naddr != naddr);
    } else if let Some(item) = items.iter_mut().find(|item| item.product.naddr == naddr) {
        item.quantity = quantity;
    }

    drop(items);
    recalculate_cart_total();
}

/// Remove an item from the cart
pub fn remove_from_cart(naddr: &str) {
    let mut items = CART_ITEMS.write();
    items.retain(|item| item.product.naddr != naddr);
    drop(items);
    recalculate_cart_total();
}

/// Set shipping option for a cart item
pub fn set_cart_item_shipping(naddr: &str, shipping_naddr: Option<String>) {
    let mut items = CART_ITEMS.write();
    if let Some(item) = items.iter_mut().find(|item| item.product.naddr == naddr) {
        item.selected_shipping = shipping_naddr;
    }
    drop(items);
    recalculate_cart_total();
}

/// Clear the entire cart
pub fn clear_cart() {
    CART_ITEMS.write().clear();
    *CART_TOTAL_SATS.write() = 0;
}

/// Get cart item count
pub fn get_cart_count() -> usize {
    CART_ITEMS.read().iter().map(|item| item.quantity as usize).sum()
}

/// Recalculate cart total (internal helper)
fn recalculate_cart_total() {
    let items = CART_ITEMS.read();
    let mut total: u64 = 0;

    for item in items.iter() {
        // Simple calculation assuming price is in sats
        // Convert price to sats using exchange rate if needed
        if let Some(sats) = item.product.price.to_sats() {
            total += sats * item.quantity as u64;
        } else {
            // Unable to convert - this shouldn't block cart calculation
            // The checkout will validate conversion is possible before payment
            log::warn!(
                "Unable to convert {} {} to sats for product {}",
                item.product.price.amount,
                item.product.price.currency,
                item.product.title
            );
        }

        // Add shipping cost if selected (convert currency if needed)
        if let Some(shipping_naddr) = &item.selected_shipping {
            if let Some(shipping) = get_cached_shipping(shipping_naddr) {
                if let Some(shipping_sats) = shipping.to_sats() {
                    total += shipping_sats;
                } else {
                    log::warn!(
                        "Unable to convert shipping cost {} {} to sats for {}",
                        shipping.base_price,
                        shipping.currency,
                        shipping.title
                    );
                }
            }
        }
    }

    *CART_TOTAL_SATS.write() = total;
}

// ============================================================================
// Filter State
// ============================================================================

/// Filter state for client-side filtering
#[derive(Clone, Debug, Default)]
pub struct ShopFilterState {
    /// Filter by category
    pub category: Option<String>,
    /// Minimum price in sats
    pub min_price_sats: Option<u64>,
    /// Maximum price in sats
    pub max_price_sats: Option<u64>,
    /// Filter by product format
    pub format: Option<ProductFormat>,
    /// Filter by visibility
    pub visibility: Option<ProductVisibility>,
    /// Filter by merchant pubkey
    pub merchant_pubkey: Option<String>,
    /// Filter by collection
    pub collection: Option<String>,
    /// Search query (matches title, description)
    pub search_query: Option<String>,
    /// In stock only
    pub in_stock_only: bool,
}

impl ShopFilterState {
    /// Check if any filters are active
    pub fn is_empty(&self) -> bool {
        self.category.is_none()
            && self.min_price_sats.is_none()
            && self.max_price_sats.is_none()
            && self.format.is_none()
            && self.visibility.is_none()
            && self.merchant_pubkey.is_none()
            && self.collection.is_none()
            && self.search_query.is_none()
            && !self.in_stock_only
    }
}

/// Filter products based on filter state
pub fn filter_products(products: &[Product], filters: &ShopFilterState) -> Vec<Product> {
    products
        .iter()
        .filter(|product| {
            // Price filter (sats only)
            if let Some(price_sats) = product.price.to_sats() {
                if let Some(min) = filters.min_price_sats {
                    if price_sats < min {
                        return false;
                    }
                }
                if let Some(max) = filters.max_price_sats {
                    if price_sats > max {
                        return false;
                    }
                }
            }

            // Category filter
            if let Some(category) = &filters.category {
                if !product.categories.iter().any(|c| c.eq_ignore_ascii_case(category)) {
                    return false;
                }
            }

            // Format filter
            if let Some(format) = &filters.format {
                if &product.format != format {
                    return false;
                }
            }

            // Visibility filter (default to showing on-sale only)
            if let Some(visibility) = &filters.visibility {
                if &product.visibility != visibility {
                    return false;
                }
            } else if !product.is_visible() {
                return false;
            }

            // Merchant filter
            if let Some(pubkey) = &filters.merchant_pubkey {
                if &product.pubkey != pubkey {
                    return false;
                }
            }

            // Collection filter
            if let Some(collection) = &filters.collection {
                if !product.collections.contains(collection) {
                    return false;
                }
            }

            // In stock filter
            if filters.in_stock_only && !product.is_in_stock() {
                return false;
            }

            // Search query filter
            if let Some(query) = &filters.search_query {
                let query_lower = query.to_lowercase();
                let matches_title = product.title.to_lowercase().contains(&query_lower);
                let matches_description = product
                    .description
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&query_lower))
                    .unwrap_or(false);
                let matches_summary = product
                    .summary
                    .as_ref()
                    .map(|s| s.to_lowercase().contains(&query_lower))
                    .unwrap_or(false);
                let matches_category = product
                    .categories
                    .iter()
                    .any(|c| c.to_lowercase().contains(&query_lower));

                if !matches_title && !matches_description && !matches_summary && !matches_category {
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect()
}

/// Sort options for products
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ProductSortBy {
    #[default]
    Newest,
    Oldest,
    PriceLow,
    PriceHigh,
    Rating,
    Title,
}

impl ProductSortBy {
    pub fn label(&self) -> &'static str {
        match self {
            ProductSortBy::Newest => "Newest First",
            ProductSortBy::Oldest => "Oldest First",
            ProductSortBy::PriceLow => "Price: Low to High",
            ProductSortBy::PriceHigh => "Price: High to Low",
            ProductSortBy::Rating => "Highest Rated",
            ProductSortBy::Title => "Alphabetical",
        }
    }
}

/// Sort products
pub fn sort_products(products: &mut [Product], sort_by: ProductSortBy) {
    match sort_by {
        ProductSortBy::Newest => products.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
        ProductSortBy::Oldest => products.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        ProductSortBy::PriceLow => products.sort_by(|a, b| {
            a.price
                .amount
                .partial_cmp(&b.price.amount)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        ProductSortBy::PriceHigh => products.sort_by(|a, b| {
            b.price
                .amount
                .partial_cmp(&a.price.amount)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        ProductSortBy::Rating => {
            // Sort by average rating (requires reviews to be loaded)
            products.sort_by(|a, b| {
                let rating_a = get_product_average_rating(&a.coordinate).unwrap_or(0.0);
                let rating_b = get_product_average_rating(&b.coordinate).unwrap_or(0.0);
                rating_b
                    .partial_cmp(&rating_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        ProductSortBy::Title => {
            products.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
    }
}

// ============================================================================
// Filter Builders (Nostr queries)
// ============================================================================

/// Build filter for fetching products (limited)
pub fn products_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_PRODUCT))
        .limit(limit)
}

/// Build filter for fetching products by author (merchant)
pub fn products_filter_by_author(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_PRODUCT))
        .author(pubkey)
        .limit(limit)
}

/// Build filter for fetching a specific product by coordinate
pub fn product_filter_by_coordinate(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_PRODUCT))
        .author(pubkey)
        .identifier(identifier)
}

/// Build filter for fetching reviews for a product
pub fn reviews_filter_for_product(product_coordinate: &str, limit: usize) -> Filter {
    // Reviews use the product coordinate as the d-tag prefix
    let d_prefix = format!("a:{}", product_coordinate);
    Filter::new()
        .kind(Kind::Custom(KIND_REVIEW))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::D), d_prefix)
        .limit(limit)
}

// ============================================================================
// Fetch Functions
// ============================================================================

/// Fetch products with limit
pub async fn fetch_products(limit: usize) -> Result<Vec<Product>> {
    *LOADING_PRODUCTS.write() = true;

    let filter = products_filter(limit);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
            .await;

    *LOADING_PRODUCTS.write() = false;

    match result {
        Ok(events) => {
            cache_product_events(&events);
            let products: Vec<Product> = events
                .iter()
                .filter_map(|e| parse_product(e).ok())
                .collect();
            Ok(products)
        }
        Err(e) => Err(e),
    }
}


/// Fetch products by merchant
pub async fn fetch_products_by_merchant(pubkey: &str, limit: usize) -> Result<Vec<Product>> {
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = products_filter_by_author(pk, limit);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    cache_product_events(&events);
    let products: Vec<Product> = events
        .iter()
        .filter_map(|e| parse_product(e).ok())
        .collect();
    Ok(products)
}

/// Fetch a specific product by naddr
pub async fn fetch_product_by_naddr(naddr: &str) -> Result<Option<Product>> {
    // Check cache first
    if let Some(cached) = get_cached_product(naddr) {
        return Ok(Some(cached));
    }

    // Parse naddr
    let nip19 = Nip19Coordinate::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    let coordinate = nip19.coordinate;
    let pk = coordinate.public_key;
    let identifier = coordinate.identifier.clone();

    let filter = product_filter_by_coordinate(pk, &identifier);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    if let Some(event) = events.first() {
        if let Ok(product) = parse_product(event) {
            cache_product(product.clone());
            return Ok(Some(product));
        }
    }

    Ok(None)
}

/// Fetch my products (for merchant dashboard)
pub async fn fetch_my_products() -> Result<Vec<Product>> {
    let pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    *LOADING_MY_PRODUCTS.write() = true;

    let result = fetch_products_by_merchant(&pubkey, 100).await;

    *LOADING_MY_PRODUCTS.write() = false;

    match result {
        Ok(products) => {
            *MY_PRODUCTS.write() = products.clone();
            Ok(products)
        }
        Err(e) => Err(e),
    }
}

/// Fetch reviews for a product (limit defaults to 50)
pub async fn fetch_product_reviews(product_coordinate: &str) -> Result<Vec<ProductReview>> {
    let filter = reviews_filter_for_product(product_coordinate, 50);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    cache_review_events(&events);
    Ok(get_product_reviews(product_coordinate))
}

// ============================================================================
// Cache Management
// ============================================================================

/// Clear all shop caches (e.g., on logout)
pub fn clear_caches() {
    PRODUCTS_CACHE.write().clear();
    SHIPPING_CACHE.write().clear();
    REVIEWS_CACHE.write().clear();
    CART_ITEMS.write().clear();
    BUYER_ORDERS.write().clear();
    SELLER_ORDERS.write().clear();
    MY_PRODUCTS.write().clear();
    MY_COLLECTIONS.write().clear();
    *CART_TOTAL_SATS.write() = 0;
    *SHOP_INITIALIZED.write() = false;
}

// ============================================================================
// Statistics
// ============================================================================

/// Shop statistics
pub struct ShopStats {
    pub total_products: usize,
    pub digital_products: usize,
    pub physical_products: usize,
    pub categories: Vec<String>,
    pub merchants: usize,
}

/// Calculate statistics from cached products
pub fn get_shop_stats() -> ShopStats {
    let cache = PRODUCTS_CACHE.read();

    let mut digital_count = 0;
    let mut physical_count = 0;
    let mut categories = HashSet::new();
    let mut merchants = HashSet::new();

    for (_, product) in cache.iter() {
        match product.format {
            ProductFormat::Digital => digital_count += 1,
            ProductFormat::Physical => physical_count += 1,
        }

        for cat in &product.categories {
            categories.insert(cat.clone());
        }

        merchants.insert(product.pubkey.clone());
    }

    ShopStats {
        total_products: cache.len(),
        digital_products: digital_count,
        physical_products: physical_count,
        categories: categories.into_iter().collect(),
        merchants: merchants.len(),
    }
}

// =============================================================================
// Shop Store Initialization & Order Persistence
// =============================================================================

/// Initialize the shop store and restore persisted orders from IndexedDB
pub async fn init_shop_store() -> Result<()> {
    if *SHOP_INITIALIZED.read() {
        log::debug!("Shop store already initialized");
        return Ok(());
    }

    log::info!("Initializing shop store...");

    // Initialize IndexedDB for order persistence
    match IndexedDbDatabase::new().await {
        Ok(db) => {
            let db_arc = Arc::new(db);
            *SHOP_DB.write() = Some(db_arc.clone());
            log::info!("Shop IndexedDB initialized");

            // Restore orders from IndexedDB
            if let Err(e) = restore_orders_from_db(&db_arc).await {
                log::warn!("Failed to restore orders from IndexedDB: {}", e);
            }
        }
        Err(e) => {
            log::warn!("Failed to initialize shop IndexedDB: {:?}", e);
            // Continue without persistence - orders will be memory-only
        }
    }

    *SHOP_INITIALIZED.write() = true;
    Ok(())
}

/// Restore orders from IndexedDB into memory signals
async fn restore_orders_from_db(db: &IndexedDbDatabase) -> Result<()> {
    log::info!("Restoring orders from IndexedDB...");

    let orders = db.get_all_orders().await
        .map_err(|e| format!("Failed to load orders: {:?}", e))?;

    if orders.is_empty() {
        log::info!("No persisted orders found");
        return Ok(());
    }

    // Get current user pubkey to separate buyer/seller orders
    let user_pubkey = nostr_client::get_user_pubkey().await
        .map(|pk| pk.to_hex())
        .unwrap_or_default();

    let mut buyer_orders = Vec::new();
    let mut seller_orders = Vec::new();

    for order in orders {
        if order.buyer_pubkey == user_pubkey {
            buyer_orders.push(order);
        } else if order.merchant_pubkey == user_pubkey {
            seller_orders.push(order.clone());
        }
    }

    log::info!("Restored {} buyer orders, {} seller orders",
        buyer_orders.len(), seller_orders.len());

    // Update signals
    *BUYER_ORDERS.write() = buyer_orders;
    *SELLER_ORDERS.write() = seller_orders;

    Ok(())
}

/// Persist an order to IndexedDB
pub async fn persist_order(order: &ShopOrder) -> Result<()> {
    let db = SHOP_DB.read();
    if let Some(db) = db.as_ref() {
        db.save_order(order).await
            .map_err(|e| format!("Failed to persist order: {:?}", e))?;
        log::debug!("Persisted order {} to IndexedDB", order.order_id);
    }
    Ok(())
}

/// Update a persisted order in IndexedDB
pub async fn update_persisted_order(order: &ShopOrder) -> Result<()> {
    let db = SHOP_DB.read();
    if let Some(db) = db.as_ref() {
        db.update_order(order).await
            .map_err(|e| format!("Failed to update order: {:?}", e))?;
        log::debug!("Updated order {} in IndexedDB", order.order_id);
    }
    Ok(())
}

// =============================================================================
// NIP-17 Order Messaging
// =============================================================================

/// Order message content structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderMessageContent {
    /// Message type (serializes as string for compatibility)
    pub message_type: String,
    /// Order ID
    pub order_id: String,
    /// Message payload (depends on type)
    pub payload: serde_json::Value,
    /// Timestamp
    pub timestamp: u64,
}

impl OrderMessageContent {
    /// Get the message type as enum (for type-safe matching)
    /// Supports both string ("order", "payment") and numeric ("1", "2") formats
    pub fn get_type(&self) -> Option<OrderMessageType> {
        // Try parsing as string first
        if let Some(t) = OrderMessageType::from_str(&self.message_type) {
            return Some(t);
        }
        // Try parsing as numeric (market-spec compatibility)
        if let Ok(num) = self.message_type.parse::<u8>() {
            return OrderMessageType::from_u8(num);
        }
        None
    }

    pub fn new_order(order_id: &str, items: Vec<OrderItem>, shipping_address: Option<String>) -> Self {
        Self {
            message_type: OrderMessageType::OrderCreation.as_str().to_string(),
            order_id: order_id.to_string(),
            payload: serde_json::json!({
                "items": items,
                "shipping_address": shipping_address,
            }),
            timestamp: now_secs(),
        }
    }

    pub fn new_payment(order_id: &str, payment_method: &str, payment_proof: &str) -> Self {
        Self {
            message_type: OrderMessageType::PaymentRequest.as_str().to_string(),
            order_id: order_id.to_string(),
            payload: serde_json::json!({
                "method": payment_method,
                "proof": payment_proof,
            }),
            timestamp: now_secs(),
        }
    }

    pub fn new_status(order_id: &str, status: &str, message: Option<&str>) -> Self {
        Self {
            message_type: OrderMessageType::StatusUpdate.as_str().to_string(),
            order_id: order_id.to_string(),
            payload: serde_json::json!({
                "status": status,
                "message": message,
            }),
            timestamp: now_secs(),
        }
    }

    pub fn new_shipping(order_id: &str, carrier: &str, tracking_number: &str) -> Self {
        Self {
            message_type: OrderMessageType::ShippingUpdate.as_str().to_string(),
            order_id: order_id.to_string(),
            payload: serde_json::json!({
                "carrier": carrier,
                "tracking_number": tracking_number,
            }),
            timestamp: now_secs(),
        }
    }
}

/// Helper to create and send gift-wrapped events to both recipient and sender
///
/// Creates NIP-17 gift wraps for both the recipient and sender (for their records),
/// then sends both events. Returns (receiver_event_id, sender_event_id).
async fn send_gift_wrapped_rumor(
    client: &nostr_sdk::Client,
    signer: &impl nostr_sdk::NostrSigner,
    recipient_pk: PublicKey,
    sender_pk: PublicKey,
    rumor: nostr::UnsignedEvent,
    log_context: &str,
) -> Result<(String, String)> {
    // Create gift wrap for RECEIVER
    let receiver_gift_wrap = EventBuilder::gift_wrap(signer, &recipient_pk, rumor.clone(), [])
        .await
        .map_err(|e| format!("Failed to create receiver gift wrap: {}", e))?;

    // Create gift wrap for SENDER (NIP-17 requirement for sender copy)
    let sender_gift_wrap = EventBuilder::gift_wrap(signer, &sender_pk, rumor, [])
        .await
        .map_err(|e| format!("Failed to create sender gift wrap: {}", e))?;

    // Send to receiver
    let receiver_result = client.send_event(&receiver_gift_wrap).await
        .map_err(|e| format!("Failed to send to receiver: {}", e))?;

    log::info!("Sent {} to receiver: {:?}", log_context, receiver_result.val);

    // Send sender copy
    let sender_result = client.send_event(&sender_gift_wrap).await
        .map_err(|e| format!("Failed to send sender copy: {}", e))?;

    log::info!("Sent {} copy to sender: {:?}", log_context, sender_result.val);

    Ok((receiver_result.val.to_hex(), sender_result.val.to_hex()))
}

/// Send an encrypted order message to a recipient via NIP-17 gift wrap
pub async fn send_order_message(
    recipient_pubkey: &str,
    content: OrderMessageContent,
) -> Result<String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let recipient_pk = PublicKey::parse(recipient_pubkey)
        .map_err(|e| format!("Invalid recipient pubkey: {}", e))?;

    let signer = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?;

    let sender_pk = signer.get_public_key().await
        .map_err(|e| format!("Failed to get sender pubkey: {}", e))?;

    // Serialize order message content
    let message_json = serde_json::to_string(&content)
        .map_err(|e| format!("Failed to serialize order message: {}", e))?;

    log::info!("Sending order message to {}: type={}", recipient_pubkey, content.message_type);

    // Build the rumor (kind 14 for private message)
    let rumor = EventBuilder::private_msg_rumor(recipient_pk, message_json)
        .build(sender_pk);

    // Send gift-wrapped message to both recipient and sender
    send_gift_wrapped_rumor(&client, &signer, recipient_pk, sender_pk, rumor, "order message").await?;

    Ok(content.order_id)
}

/// Send a Kind 17 payment receipt to merchant via NIP-17 gift wrap
///
/// This is sent by the buyer after successful payment to confirm the transaction.
/// Includes payment proof that the merchant can verify.
pub async fn send_payment_receipt(
    merchant_pubkey: &str,
    order_id: &str,
    amount_sats: u64,
    payment_method: &str, // "cashu", "lightning", "bitcoin", "fiat"
    medium_reference: &str, // mint URL, invoice, address, or transaction ID
    proof: &str, // token, preimage, txid, or receipt ID
) -> Result<String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let recipient_pk = PublicKey::parse(merchant_pubkey)
        .map_err(|e| format!("Invalid merchant pubkey: {}", e))?;

    let signer = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?;

    let sender_pk = signer.get_public_key().await
        .map_err(|e| format!("Failed to get sender pubkey: {}", e))?;

    // Build Kind 17 payment receipt with proper tags per market-spec
    // Content: human-readable payment confirmation
    let content = format!("Payment receipt for order {} - {} sats via {}", order_id, amount_sats, payment_method);

    // Build tags per market-spec:
    // - p: merchant pubkey
    // - subject: "order-receipt"
    // - order: order ID
    // - payment: [medium, medium-reference, proof]
    // - amount: payment amount in sats
    let tags = vec![
        Tag::public_key(recipient_pk),
        Tag::custom(TagKind::Custom("subject".into()), vec!["order-receipt".to_string()]),
        Tag::custom(TagKind::Custom("order".into()), vec![order_id.to_string()]),
        Tag::custom(TagKind::Custom("payment".into()), vec![
            payment_method.to_string(),
            medium_reference.to_string(),
            proof.to_string()
        ]),
        Tag::custom(TagKind::Custom("amount".into()), vec![amount_sats.to_string(), "sat".to_string()]),
    ];

    // Build the Kind 17 rumor event
    let rumor = EventBuilder::new(Kind::Custom(KIND_PAYMENT_RECEIPT), &content)
        .tags(tags)
        .build(sender_pk);

    log::info!("Sending payment receipt for order {} to merchant {}", order_id, merchant_pubkey);

    // Send gift-wrapped receipt to both merchant and buyer
    send_gift_wrapped_rumor(&client, &signer, recipient_pk, sender_pk, rumor, "payment receipt").await?;

    Ok(order_id.to_string())
}

/// Validate payment proof format based on payment method
///
/// Returns Ok(()) if valid, Err with message if invalid.
fn validate_payment_proof(payment_method: &str, payment_proof: &str) -> Result<()> {
    if payment_proof.is_empty() {
        return Err("Payment proof is required".to_string());
    }

    match payment_method {
        "lightning" => {
            // Lightning preimage should be 64 hex characters (32 bytes)
            // Multiple preimages are comma-separated for multi-merchant orders
            for preimage in payment_proof.split(',') {
                let trimmed = preimage.trim();
                // Skip "manual_payment" placeholder for manual confirmations
                if trimmed == "manual_payment" {
                    continue;
                }
                if trimmed.len() != 64 {
                    return Err(format!(
                        "Invalid Lightning preimage length: expected 64 hex chars, got {}",
                        trimmed.len()
                    ));
                }
                if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err("Invalid Lightning preimage: must be hexadecimal".to_string());
                }
            }
        }
        "cashu" => {
            // Cashu tokens start with "cashuA" or "cashuB"
            if !payment_proof.starts_with("cashu") {
                return Err("Invalid Cashu token format: must start with 'cashu'".to_string());
            }
        }
        "bitcoin" => {
            // Bitcoin txid should be 64 hex characters
            if payment_proof.len() != 64 {
                return Err("Invalid Bitcoin txid: expected 64 hex chars".to_string());
            }
            if !payment_proof.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("Invalid Bitcoin txid: must be hexadecimal".to_string());
            }
        }
        _ => {
            // Unknown payment method - allow but log warning
            log::warn!("Unknown payment method '{}', skipping proof validation", payment_method);
        }
    }

    Ok(())
}

/// Create and place a shop order
///
/// This creates an order and sends the initial order message to the merchant.
/// Returns the order ID.
pub async fn create_shop_order(
    items: Vec<CartItem>,
    shipping_address: Option<String>,
    payment_method: &str,
    payment_proof: &str,
) -> Result<String> {
    if items.is_empty() {
        return Err("Cannot create order with no items".to_string());
    }

    // Validate payment proof format
    validate_payment_proof(payment_method, payment_proof)?;

    // Generate unique order ID with timestamp + random suffix for collision resistance
    let order_id = crate::utils::format::generate_unique_id();

    // Group items by merchant
    let mut merchants: HashMap<String, Vec<OrderItem>> = HashMap::new();

    for item in &items {
        let merchant_pubkey = item.product.pubkey.clone();
        let order_item = OrderItem {
            product_coordinate: item.product.coordinate.clone(),
            quantity: item.quantity,
        };

        merchants.entry(merchant_pubkey)
            .or_default()
            .push(order_item);
    }

    // Calculate total first (needed for payment receipt)
    // Validate all items have sats-convertible prices before proceeding
    let total_sats: u64 = {
        let mut total = 0u64;
        for item in &items {
            let price_sats = if item.product.price.currency.eq_ignore_ascii_case("sats")
                || item.product.price.currency.eq_ignore_ascii_case("sat")
            {
                item.product.price.amount as u64
            } else {
                // Non-sats currency - reject checkout until conversion is implemented
                return Err(format!(
                    "Cannot checkout: \"{}\" has unsupported currency '{}'. Only sats pricing is currently supported.",
                    item.product.title,
                    item.product.price.currency
                ));
            };
            total = total.saturating_add(price_sats.saturating_mul(item.quantity as u64));
        }
        total
    };

    // Determine medium reference based on payment method
    // For cashu, the token itself contains mint info; for lightning, use invoice if available
    let medium_reference = match payment_method {
        "cashu" => "ecash".to_string(), // Per market-spec: "ecash" as medium
        "lightning" => "lightning".to_string(),
        "bitcoin" => "bitcoin".to_string(),
        _ => payment_method.to_string(),
    };

    // Send order message to each merchant
    for (merchant_pubkey, merchant_items) in &merchants {
        // Send order creation message (Kind 16)
        let order_msg = OrderMessageContent::new_order(
            &order_id,
            merchant_items.clone(),
            shipping_address.clone(),
        );
        send_order_message(merchant_pubkey, order_msg).await?;

        // Send payment confirmation (Kind 16)
        let payment_msg = OrderMessageContent::new_payment(
            &order_id,
            payment_method,
            payment_proof,
        );
        send_order_message(merchant_pubkey, payment_msg).await?;

        // Send payment receipt (Kind 17) per market-spec
        if let Err(e) = send_payment_receipt(
            merchant_pubkey,
            &order_id,
            total_sats,
            payment_method,
            &medium_reference,
            payment_proof,
        ).await {
            log::warn!("Failed to send payment receipt: {}", e);
            // Don't fail the order if receipt fails - order and payment messages were sent
        }
    }

    // total_sats already calculated above

    // Get user pubkey
    let buyer_pubkey = nostr_client::get_user_pubkey().await
        .map(|pk| pk.to_hex())
        .unwrap_or_default();

    let now = now_secs();

    // Create separate orders for each merchant (multi-merchant support)
    // This ensures each merchant can track their portion of the order
    let merchant_count = merchants.len();
    for (merchant_pubkey, merchant_items) in &merchants {
        // Calculate this merchant's subtotal
        let merchant_total: u64 = items.iter()
            .filter(|item| &item.product.pubkey == merchant_pubkey)
            .map(|item| {
                let price = if item.product.price.currency.eq_ignore_ascii_case("sats")
                    || item.product.price.currency.eq_ignore_ascii_case("sat")
                {
                    item.product.price.amount as u64
                } else {
                    0 // Non-sats already rejected earlier
                };
                price.saturating_mul(item.quantity as u64)
            })
            .sum();

        // Generate unique order ID for this merchant's portion
        // For single merchant, use base order_id; for multi-merchant, append merchant suffix
        let merchant_order_id = if merchant_count > 1 {
            let short_pk = if merchant_pubkey.len() >= 8 {
                &merchant_pubkey[..8]
            } else {
                merchant_pubkey.as_str()
            };
            format!("{}-{}", order_id, short_pk)
        } else {
            order_id.clone()
        };

        // Get shipping info for this merchant's items
        let merchant_shipping = items.iter()
            .find(|item| &item.product.pubkey == merchant_pubkey)
            .and_then(|item| item.selected_shipping.clone());

        let order = ShopOrder {
            order_id: merchant_order_id,
            buyer_pubkey: buyer_pubkey.clone(),
            merchant_pubkey: merchant_pubkey.clone(),
            items: merchant_items.clone(),
            amount_sats: merchant_total,
            status: OrderStatus::Pending,
            shipping_status: None,
            shipping_option: merchant_shipping,
            shipping_address: shipping_address.clone(),
            tracking_number: None,
            carrier: None,
            email: None,
            phone: None,
            created_at: now,
            updated_at: now,
            paid_at: Some(now), // Paid at creation since we already processed payment
            shipped_at: None,
            delivered_at: None,
            note: None,
            download_url: None,
            license_key: None,
        };

        // Add to buyer orders (persists to IndexedDB and updates state)
        add_buyer_order(order).await;
    }

    log::info!("Created order: {} with {} items, total: {} sats",
        order_id, items.len(), total_sats);

    Ok(order_id)
}

// =============================================================================
// Product Publishing
// =============================================================================

// ProductPrice, ProductImage, ProductSpec, ProductType available from nip99 if needed

/// Form data for creating a new product
#[derive(Clone, Debug, Default)]
pub struct ProductFormData {
    pub title: String,
    pub description: String,
    pub price_amount: f64,
    pub price_currency: String,
    pub images: Vec<String>,
    pub categories: Vec<String>,
    pub is_digital: bool,
    pub stock: Option<u32>,
    pub specs: Vec<(String, String)>,
    pub shipping_regions: Vec<String>,
    /// Product condition: new, like_new, used, fair, refurbished
    pub condition: Option<String>,
}

/// Publish a new product (Kind 30402)
pub async fn publish_product(data: ProductFormData) -> Result<String> {
    use nostr_sdk::TagKind;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Validate required fields
    if data.title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    if data.price_amount <= 0.0 {
        return Err("Price must be greater than 0".to_string());
    }

    // Generate d-tag with collision-resistant unique ID
    let d_tag = crate::utils::format::generate_unique_id();

    // Build tags
    let mut tags = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::custom("title"), vec![data.title.clone()]),
        Tag::custom(TagKind::custom("price"), vec![
            data.price_amount.to_string(),
            data.price_currency.clone(),
        ]),
    ];

    // Add summary/description
    if !data.description.is_empty() {
        tags.push(Tag::custom(TagKind::custom("summary"), vec![data.description.clone()]));
    }

    // Add images
    for img_url in &data.images {
        tags.push(Tag::custom(TagKind::custom("image"), vec![img_url.clone()]));
    }

    // Add categories/tags
    for cat in &data.categories {
        tags.push(Tag::hashtag(cat));
    }

    // Add format (digital/physical)
    let format = if data.is_digital { "digital" } else { "physical" };
    tags.push(Tag::custom(TagKind::custom("format"), vec![format.to_string()]));

    // Add stock if specified
    if let Some(stock) = data.stock {
        tags.push(Tag::custom(TagKind::custom("stock"), vec![stock.to_string()]));
    }

    // Add specs
    for (key, value) in &data.specs {
        tags.push(Tag::custom(TagKind::custom("spec"), vec![key.clone(), value.clone()]));
    }

    // Add shipping regions for physical products
    if !data.is_digital {
        for region in &data.shipping_regions {
            tags.push(Tag::custom(TagKind::custom("shipping"), vec![region.clone()]));
        }
    }

    // Add condition if specified
    if let Some(ref condition) = data.condition {
        if !condition.is_empty() {
            tags.push(Tag::custom(TagKind::custom("condition"), vec![condition.clone()]));
        }
    }

    // Build and sign event
    let content = data.description.clone();
    let event_builder = EventBuilder::new(Kind::Custom(KIND_PRODUCT), content)
        .tags(tags);

    let output = client.send_event_builder(event_builder).await
        .map_err(|e| format!("Failed to publish product: {}", e))?;

    log::info!("Published product: {} (event: {:?})", data.title, output.val);

    // Refresh my products
    spawn(async {
        if let Err(e) = fetch_my_products().await {
            log::error!("Failed to refresh my products: {}", e);
        }
    });

    Ok(d_tag)
}

// =============================================================================
// Reviews (Kind 31555)
// =============================================================================

/// Publish a product review (Kind 31555)
pub async fn publish_review(
    product_coordinate: &str,
    overall_rating: f64,
    content: String,
    value_rating: Option<f64>,
    quality_rating: Option<f64>,
    delivery_rating: Option<f64>,
    communication_rating: Option<f64>,
) -> Result<String> {
    use nostr_sdk::TagKind;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Generate d-tag with collision-resistant unique ID
    let d_tag = crate::utils::format::generate_unique_id();

    // Build tags
    // Rating tag format: ["rating", score, category] per parser expectations
    let mut tags = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::custom("a"), vec![product_coordinate.to_string()]),
        Tag::custom(TagKind::custom("rating"), vec![
            format!("{:.1}", overall_rating),
            "thumb".to_string(), // Use "thumb" for overall rating per NIP-99
        ]),
    ];

    // Add category ratings with format: ["rating", score, category]
    if let Some(v) = value_rating {
        tags.push(Tag::custom(TagKind::custom("rating"), vec![
            format!("{:.1}", v), "value".to_string()
        ]));
    }
    if let Some(q) = quality_rating {
        tags.push(Tag::custom(TagKind::custom("rating"), vec![
            format!("{:.1}", q), "quality".to_string()
        ]));
    }
    if let Some(d) = delivery_rating {
        tags.push(Tag::custom(TagKind::custom("rating"), vec![
            format!("{:.1}", d), "delivery".to_string()
        ]));
    }
    if let Some(c) = communication_rating {
        tags.push(Tag::custom(TagKind::custom("rating"), vec![
            format!("{:.1}", c), "communication".to_string()
        ]));
    }

    // Build and sign event
    let event_builder = EventBuilder::new(Kind::Custom(KIND_REVIEW), content)
        .tags(tags);

    let output = client.send_event_builder(event_builder).await
        .map_err(|e| format!("Failed to publish review: {}", e))?;

    log::info!("Published review for {}: {:?}", product_coordinate, output.val);

    Ok(d_tag)
}

// =============================================================================
// Collections (Kind 30405)
// =============================================================================

/// Fetch a collection by naddr
pub async fn fetch_collection_by_naddr(naddr: &str) -> Result<Option<ProductCollection>> {
    // Verify client is initialized (we use fetch_events_aggregated below which handles the client)
    if nostr_client::NOSTR_CLIENT.read().is_none() {
        return Err("Client not initialized".to_string());
    }

    // Parse naddr to get pubkey and d-tag
    let coordinate = nostr_sdk::nips::nip19::Nip19::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    let (pubkey, d_tag) = match &coordinate {
        nostr_sdk::nips::nip19::Nip19::Coordinate(c) => {
            (c.public_key, c.identifier.clone())
        }
        _ => return Err("Not a valid collection address".to_string()),
    };

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_COLLECTION))
        .author(pubkey)
        .identifier(&d_tag)
        .limit(1);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await
        .map_err(|e| format!("Failed to fetch collection: {}", e))?;

    if let Some(event) = events.first() {
        match parse_collection(event) {
            Ok(collection) => Ok(Some(collection)),
            Err(e) => {
                log::warn!("Failed to parse collection: {}", e);
                Ok(None)
            }
        }
    } else {
        Ok(None)
    }
}

/// Fetch products in a collection
pub async fn fetch_collection_products(collection: &ProductCollection) -> Result<Vec<Product>> {
    let mut products = Vec::new();

    for coord in &collection.products {
        // Parse coordinate: "30402:pubkey:d-tag"
        let parts: Vec<&str> = coord.split(':').collect();
        if parts.len() >= 3 {
            let pubkey = PublicKey::parse(parts[1]).ok();
            let d_tag = parts[2];

            if let Some(pk) = pubkey {
                let filter = Filter::new()
                    .kind(Kind::Custom(KIND_PRODUCT))
                    .author(pk)
                    .identifier(d_tag)
                    .limit(1);

                if let Ok(events) = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await {
                    if let Some(event) = events.first() {
                        if let Ok(product) = parse_product(event) {
                            products.push(product);
                        }
                    }
                }
            }
        }
    }

    Ok(products)
}

/// Data for creating/updating a collection
#[derive(Clone, Debug)]
pub struct CollectionFormData {
    pub title: String,
    pub description: String,
    pub image: Option<String>,
    pub products: Vec<String>, // Product coordinates
    pub shipping_options: Vec<String>,
}

/// Fetch current user's collections
pub async fn fetch_my_collections() -> Result<Vec<ProductCollection>> {
    let pubkey = nostr_client::get_user_pubkey()
        .await
        .map_err(|e| format!("Not authenticated: {}", e))?
        .to_hex();

    *LOADING_MY_COLLECTIONS.write() = true;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_COLLECTION))
        .author(PublicKey::parse(&pubkey).map_err(|e| e.to_string())?)
        .limit(100);

    let events = client
        .fetch_events(filter, std::time::Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch collections: {}", e))?;

    let mut collections = Vec::new();
    for event in events.into_iter() {
        if let Ok(collection) = parse_collection(&event) {
            collections.push(collection);
        }
    }

    // Sort by created_at descending
    collections.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    *MY_COLLECTIONS.write() = collections.clone();
    *LOADING_MY_COLLECTIONS.write() = false;

    Ok(collections)
}

/// Create or update a collection (Kind 30405)
pub async fn publish_collection(data: CollectionFormData, existing_d_tag: Option<String>) -> Result<String> {
    use nostr_sdk::TagKind;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Title is required
    if data.title.trim().is_empty() {
        return Err("Title is required".to_string());
    }

    // Use existing d-tag or generate new one
    let d_tag = existing_d_tag.unwrap_or_else(crate::utils::format::generate_unique_id);

    // Build tags
    let mut tags = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::custom("title"), vec![data.title.clone()]),
    ];

    // Description
    if !data.description.trim().is_empty() {
        tags.push(Tag::custom(TagKind::custom("summary"), vec![data.description.clone()]));
    }

    // Image
    if let Some(ref img) = data.image {
        if !img.trim().is_empty() {
            tags.push(Tag::custom(TagKind::custom("image"), vec![img.clone()]));
        }
    }

    // Products (as "a" tags with coordinates)
    for coord in &data.products {
        tags.push(Tag::custom(TagKind::a(), vec![coord.clone()]));
    }

    // Shipping options
    for shipping in &data.shipping_options {
        tags.push(Tag::custom(TagKind::custom("shipping_option"), vec![shipping.clone()]));
    }

    // Create event
    let builder = EventBuilder::new(Kind::Custom(KIND_COLLECTION), &data.description)
        .tags(tags);

    // Sign and publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish collection: {}", e))?;

    log::info!("Published collection: {} (event: {})", d_tag, output.id());

    // Refresh collections
    let _ = fetch_my_collections().await;

    Ok(d_tag)
}

/// Delete a collection
pub async fn delete_collection(d_tag: &str) -> Result<()> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Find the collection event ID
    let collection = MY_COLLECTIONS.read()
        .iter()
        .find(|c| c.d_tag == d_tag)
        .cloned()
        .ok_or("Collection not found")?;

    let event_id = EventId::parse(&collection.event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Create deletion event using NIP-9
    use nostr::nips::nip09::EventDeletionRequest;
    let deletion_request = EventDeletionRequest::new().id(event_id);
    let delete_builder = EventBuilder::delete(deletion_request);

    client.send_event_builder(delete_builder).await
        .map_err(|e| format!("Failed to delete collection: {}", e))?;

    // Remove from local state
    MY_COLLECTIONS.write().retain(|c| c.d_tag != d_tag);

    log::info!("Deleted collection: {}", d_tag);

    Ok(())
}

// =============================================================================
// Search
// =============================================================================

/// Search products by query using NIP-50, with fallback to local filtering
pub async fn search_products(query: &str, limit: usize) -> Result<Vec<Product>> {
    let client = nostr_client::get_client()
        .ok_or("Client not initialized")?;

    // Try NIP-50 relay search first
    let search_filter = Filter::new()
        .kind(Kind::Custom(KIND_PRODUCT))
        .search(query)
        .limit(limit);

    let search_result = client.fetch_events(search_filter, Duration::from_secs(5)).await;

    // If NIP-50 returns results, use them
    if let Ok(events) = search_result {
        if !events.is_empty() {
            log::debug!("NIP-50 search returned {} products for '{}'", events.len(), query);
            let products: Vec<Product> = events
                .iter()
                .filter_map(|e| parse_product(e).ok())
                .filter(|p| p.is_visible()) // Only show visible products
                .collect();
            return Ok(products);
        }
    }

    // Fallback: fetch recent products and filter locally
    log::debug!("NIP-50 search returned no results, falling back to local filter for '{}'", query);
    let all_products = fetch_products(200).await?;

    let query_lower = query.to_lowercase();
    let filtered: Vec<Product> = all_products.into_iter()
        .filter(|p| {
            p.is_visible() && (
                p.title.to_lowercase().contains(&query_lower) ||
                p.description.as_ref().map(|d| d.to_lowercase().contains(&query_lower)).unwrap_or(false) ||
                p.categories.iter().any(|c| c.to_lowercase().contains(&query_lower))
            )
        })
        .take(limit)
        .collect();

    Ok(filtered)
}

// =============================================================================
// Order Management
// =============================================================================

/// Fetch orders for the current user (as buyer)
pub async fn fetch_my_orders() -> Result<Vec<ShopOrder>> {
    // Orders are stored locally in BUYER_ORDERS after being created via checkout
    // In a real implementation, we would also fetch from NIP-17 gift wraps
    Ok(BUYER_ORDERS.read().clone())
}

/// Fetch orders for the current user (as seller)
pub async fn fetch_seller_orders() -> Result<Vec<ShopOrder>> {
    // Orders are stored locally in SELLER_ORDERS
    // In a real implementation, we would also fetch from NIP-17 gift wraps
    Ok(SELLER_ORDERS.read().clone())
}

/// Process an incoming order message and update local state
/// Uses OrderStatus::from_str() and ShippingStatus::from_str() for parsing
/// sender_pubkey is provided when message comes from gift wrap to identify buyer
pub async fn process_order_message(msg: &OrderMessageContent, sender_pubkey: Option<&str>) -> Result<()> {
    let order_id = &msg.order_id;

    // Track if we updated an order so we can persist it
    let updated_order: Option<ShopOrder>;

    match msg.get_type() {
        Some(OrderMessageType::OrderCreation) => {
            // New order received (seller perspective)
            log::info!("Received new order: {}", order_id);

            // Parse order from payload and add to seller orders
            if let Some(buyer) = sender_pubkey {
                if let Some(items_value) = msg.payload.get("items") {
                    if let Ok(items) = serde_json::from_value::<Vec<OrderItem>>(items_value.clone()) {
                        let shipping_address = msg.payload.get("shipping_address")
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        let shipping_option = msg.payload.get("shipping_option")
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        // Get total from payload
                        let total_sats = msg.payload.get("amount_sats")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        // Get current user as merchant
                        let merchant_pubkey = nostr_client::get_user_pubkey().await
                            .map(|pk| pk.to_hex())
                            .unwrap_or_default();

                        let order = ShopOrder {
                            order_id: order_id.clone(),
                            buyer_pubkey: buyer.to_string(),
                            merchant_pubkey,
                            items,
                            amount_sats: total_sats,
                            status: OrderStatus::Pending,
                            shipping_status: None,
                            shipping_option,
                            shipping_address,
                            tracking_number: None,
                            carrier: None,
                            email: None,
                            phone: None,
                            created_at: msg.timestamp,
                            updated_at: msg.timestamp,
                            paid_at: None,
                            shipped_at: None,
                            delivered_at: None,
                            note: None,
                            download_url: None,
                            license_key: None,
                        };

                        add_seller_order(order).await;
                        log::info!("Added order {} to seller orders", order_id);
                    }
                }
            }
            updated_order = None;
        }
        Some(OrderMessageType::PaymentRequest) => {
            // Payment info received
            log::info!("Received payment for order: {}", order_id);
            // Update order to confirmed status
            let mut orders = BUYER_ORDERS.write();
            updated_order = if let Some(order) = orders.iter_mut().find(|o| o.order_id == *order_id) {
                order.status = OrderStatus::Confirmed;
                order.paid_at = Some(msg.timestamp);
                order.updated_at = msg.timestamp;
                Some(order.clone())
            } else {
                None
            };
        }
        Some(OrderMessageType::StatusUpdate) => {
            // Order status changed (buyer perspective)
            let mut orders = BUYER_ORDERS.write();
            updated_order = if let Some(status_str) = msg.payload.get("status").and_then(|v| v.as_str()) {
                if let Some(new_status) = OrderStatus::from_str(status_str) {
                    log::info!("Order {} status updated to: {}", order_id, new_status);

                    // Update buyer's order with new status
                    if let Some(order) = orders.iter_mut().find(|o| o.order_id == *order_id) {
                        order.status = new_status;
                        order.updated_at = msg.timestamp;

                        // Check if order is still active
                        if !new_status.is_active() {
                            log::info!("Order {} is no longer active", order_id);
                        }
                        Some(order.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
        }
        Some(OrderMessageType::ShippingUpdate) => {
            // Shipping info received (buyer perspective)
            let tracking = msg.payload.get("tracking_number").and_then(|v| v.as_str());
            let carrier = msg.payload.get("carrier").and_then(|v| v.as_str());
            let status_str = msg.payload.get("status").and_then(|v| v.as_str());

            log::info!("Shipping update for order {}: tracking={:?}", order_id, tracking);

            let mut orders = BUYER_ORDERS.write();
            updated_order = if let Some(order) = orders.iter_mut().find(|o| o.order_id == *order_id) {
                if let Some(t) = tracking {
                    order.tracking_number = Some(t.to_string());
                }
                if let Some(c) = carrier {
                    order.carrier = Some(c.to_string());
                }
                if let Some(s) = status_str {
                    if let Some(ship_status) = ShippingStatus::from_str(s) {
                        order.shipping_status = Some(ship_status);
                        if ship_status == ShippingStatus::Shipped {
                            order.shipped_at = Some(msg.timestamp);
                        } else if ship_status == ShippingStatus::Delivered {
                            order.delivered_at = Some(msg.timestamp);
                        }
                    }
                }
                order.updated_at = msg.timestamp;
                Some(order.clone())
            } else {
                None
            };
        }
        Some(OrderMessageType::Message) => {
            // General message - could be used for buyer/seller chat
            if let Some(text) = msg.payload.get("text").and_then(|v| v.as_str()) {
                log::info!("Message for order {}: {}", order_id, text);
            }
            updated_order = None;
        }
        None => {
            log::warn!("Unknown order message type: {}", msg.message_type);
            updated_order = None;
        }
    }

    // Persist updated order to IndexedDB (lock is released, safe to await)
    if let Some(order) = updated_order {
        if let Err(e) = update_persisted_order(&order).await {
            log::warn!("Failed to persist order update to IndexedDB: {}", e);
        }
    }

    Ok(())
}

/// Listen for order updates via NIP-17 gift wrap messages
/// This would be called on app startup to process any pending messages
pub async fn listen_for_order_updates() -> Result<()> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Get current user's pubkey
    let signer = client.signer().await
        .map_err(|e| format!("No signer: {}", e))?;
    let my_pubkey = signer.get_public_key().await
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;

    // Subscribe to Kind 1059 (gift wrap) events addressed to us
    // These contain encrypted order messages
    let filter = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(my_pubkey)
        .limit(100);

    log::info!("Fetching order update messages...");

    let events = client.fetch_events(filter, Duration::from_secs(10)).await
        .map_err(|e| format!("Failed to fetch gift wraps: {}", e))?;

    log::info!("Found {} gift wrap events", events.len());

    for event in events.iter() {
        // Check if we've already processed this event to prevent duplicates
        let event_id = event.id.to_hex();
        if PROCESSED_ORDER_EVENTS.read().contains(&event_id) {
            log::debug!("Skipping already processed order event: {}", event_id);
            continue;
        }

        // Decrypt the gift wrap to get the sealed message
        match client.unwrap_gift_wrap(event).await {
            Ok(unwrapped) => {
                // The unwrapped event contains the actual order message
                // Check if it's an order-related message (Kind 16 or 17)
                let rumor = unwrapped.rumor;
                let kind_num = rumor.kind.as_u16();

                if kind_num == KIND_ORDER_MESSAGE || kind_num == KIND_PAYMENT_RECEIPT {
                    // Parse the order message content
                    // Extract sender pubkey from the rumor for order creation
                    let sender_pubkey = rumor.pubkey.to_hex();

                    match serde_json::from_str::<OrderMessageContent>(&rumor.content) {
                        Ok(msg) => {
                            if let Err(e) = process_order_message(&msg, Some(&sender_pubkey)).await {
                                log::error!("Failed to process order message: {}", e);
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to parse order message: {}", e);
                        }
                    }
                }

                // Mark event as processed after successful unwrap
                PROCESSED_ORDER_EVENTS.write().put(event_id, ());
            }
            Err(e) => {
                log::debug!("Failed to unwrap gift wrap: {}", e);
                // This is normal - not all gift wraps are for us or decryptable
            }
        }
    }

    Ok(())
}

/// Get count of active orders (pending, confirmed, processing)
pub fn get_active_order_count() -> usize {
    BUYER_ORDERS.read().iter()
        .filter(|o| o.status.is_active())
        .count()
}

/// Get count of seller's active orders
pub fn get_seller_active_order_count() -> usize {
    SELLER_ORDERS.read().iter()
        .filter(|o| o.status.is_active())
        .count()
}

/// Update order status (seller action)
pub async fn update_order_status(
    order_id: &str,
    new_status: OrderStatus,
    shipping_status: Option<ShippingStatus>,
    tracking_number: Option<String>,
    carrier: Option<String>,
) -> Result<()> {
    // Find the order
    let mut orders = SELLER_ORDERS.write();
    let order = orders.iter_mut()
        .find(|o| o.order_id == order_id)
        .ok_or("Order not found")?;

    // Update status
    order.status = new_status;
    order.updated_at = now_secs();

    // Handle shipping-specific updates
    if let Some(ref ship_status) = shipping_status {
        order.shipping_status = Some(*ship_status);
        if *ship_status == ShippingStatus::Shipped {
            order.shipped_at = Some(order.updated_at);
        } else if *ship_status == ShippingStatus::Delivered {
            order.delivered_at = Some(order.updated_at);
        }
    }
    if let Some(ref tracking) = tracking_number {
        order.tracking_number = Some(tracking.clone());
    }
    if let Some(ref c) = carrier {
        order.carrier = Some(c.clone());
    }

    let buyer_pubkey = order.buyer_pubkey.clone();
    let order_for_persist = order.clone(); // Clone for persistence

    // Send status update message to buyer
    let content = if shipping_status.is_some() || tracking_number.is_some() {
        // Use shipping update for shipping-related changes
        OrderMessageContent::new_shipping(
            order_id,
            carrier.as_deref().unwrap_or(""),
            tracking_number.as_deref().unwrap_or(""),
        )
    } else {
        // Use status update for order status changes
        OrderMessageContent::new_status(
            order_id,
            new_status.as_str(),
            None,
        )
    };

    drop(orders); // Release the lock before async call
    send_order_message(&buyer_pubkey, content).await?;

    // Persist updated order to IndexedDB
    if let Err(e) = update_persisted_order(&order_for_persist).await {
        log::warn!("Failed to persist seller order update to IndexedDB: {}", e);
    }

    Ok(())
}

/// Add an order to seller orders (called when receiving order from buyer)
pub async fn add_seller_order(order: ShopOrder) {
    // Persist to IndexedDB first (non-blocking - log errors but don't fail)
    if let Err(e) = persist_order(&order).await {
        log::warn!("Failed to persist seller order to IndexedDB: {}", e);
    }
    SELLER_ORDERS.write().push(order);
}

/// Add an order to buyer orders (called when creating order)
pub async fn add_buyer_order(order: ShopOrder) {
    // Persist to IndexedDB first (non-blocking - log errors but don't fail)
    if let Err(e) = persist_order(&order).await {
        log::warn!("Failed to persist buyer order to IndexedDB: {}", e);
    }
    BUYER_ORDERS.write().push(order);
}

// =============================================================================
// Product Management (Edit/Delete)
// =============================================================================

/// Delete a product by publishing a deletion event (Kind 5)
pub async fn delete_product(product_naddr: &str, d_tag: &str) -> Result<()> {
    use nostr_sdk::TagKind;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let signer = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let pubkey = signer.get_public_key().await
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;

    // Create the coordinate for the product (Kind 30402)
    let coordinate = format!("{}:{}:{}", KIND_PRODUCT, pubkey.to_hex(), d_tag);

    // Build deletion event (Kind 5)
    let tags = vec![
        Tag::custom(TagKind::custom("a"), vec![coordinate]),
    ];

    let event = EventBuilder::new(Kind::EventDeletion, "Product deleted")
        .tags(tags)
        .sign(&signer).await
        .map_err(|e| format!("Failed to sign deletion event: {}", e))?;

    // Publish to relays
    client.send_event(&event).await
        .map_err(|e| format!("Failed to send deletion event: {}", e))?;

    // Remove from local cache
    MY_PRODUCTS.write().retain(|p| p.naddr != product_naddr);
    PRODUCTS_CACHE.write().pop(&product_naddr.to_string());

    log::info!("Product deleted: {}", d_tag);
    Ok(())
}

/// Update a product by republishing with the same d-tag
pub async fn update_product(d_tag: &str, data: ProductFormData) -> Result<String> {
    use nostr_sdk::TagKind;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Validate required fields
    if data.title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    if data.price_amount <= 0.0 {
        return Err("Price must be greater than 0".to_string());
    }

    // Build tags (same d-tag to replace)
    let mut tags = vec![
        Tag::identifier(d_tag),
        Tag::custom(TagKind::custom("title"), vec![data.title.clone()]),
        Tag::custom(TagKind::custom("price"), vec![
            data.price_amount.to_string(),
            data.price_currency.clone(),
        ]),
    ];

    // Add summary/description
    if !data.description.is_empty() {
        tags.push(Tag::custom(TagKind::custom("summary"), vec![data.description.clone()]));
    }

    // Add images
    for img in data.images.iter() {
        if !img.is_empty() {
            tags.push(Tag::custom(TagKind::custom("image"), vec![img.clone()]));
        }
    }

    // Add categories/tags (use Tag::hashtag for consistency with publish_product)
    for cat in data.categories.iter() {
        if !cat.is_empty() {
            tags.push(Tag::hashtag(cat));
        }
    }

    // Add stock if set
    if let Some(stock) = data.stock {
        tags.push(Tag::custom(TagKind::custom("stock"), vec![stock.to_string()]));
    }

    // Add format
    let format = if data.is_digital { "digital" } else { "physical" };
    tags.push(Tag::custom(TagKind::custom("format"), vec![format.to_string()]));

    // Add specs (consistent with publish_product)
    for (key, value) in &data.specs {
        tags.push(Tag::custom(TagKind::custom("spec"), vec![key.clone(), value.clone()]));
    }

    // Add shipping regions for physical products (use "shipping" for consistency)
    if !data.is_digital {
        for region in data.shipping_regions.iter() {
            if !region.is_empty() {
                tags.push(Tag::custom(TagKind::custom("shipping"), vec![region.clone()]));
            }
        }
    }

    // Add condition if specified
    if let Some(ref condition) = data.condition {
        if !condition.is_empty() {
            tags.push(Tag::custom(TagKind::custom("condition"), vec![condition.clone()]));
        }
    }

    // Build and sign event
    let signer = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?;

    let event = EventBuilder::new(Kind::Custom(KIND_PRODUCT), data.description.clone())
        .tags(tags)
        .sign(&signer).await
        .map_err(|e| format!("Failed to sign product event: {}", e))?;

    // Publish
    client.send_event(&event).await
        .map_err(|e| format!("Failed to send product event: {}", e))?;

    log::info!("Product updated: {}", d_tag);
    Ok(d_tag.to_string())
}
