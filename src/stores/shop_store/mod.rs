//! Shop Store
//!
//! Handles NIP-99 Marketplace events - products, collections, orders, and cart management.
//!
//! Organized into submodules:
//! - `cart` - Shopping cart operations (add, remove, update, totals)
//! - `filters` - Product filtering, sorting, and Nostr filter builders
//! - `orders` - Order lifecycle, gift-wrap messaging, DB persistence
#![allow(unused_imports)]

use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
type Result<T> = std::result::Result<T, String>;
use crate::stores::shop_database::ShopDatabase;
use crate::stores::nostr_client;
use crate::utils::format::truncate_pubkey;
use crate::utils::nip99::{
    now_secs, parse_collection, parse_product, parse_review, parse_shipping, CartItem, OrderItem,
    OrderMessageType, OrderStatus, Product, ProductCollection, ProductFormat, ProductReview,
    ProductType, ProductVisibility, ShippingOption, ShippingService, ShippingStatus, ShopOrder,
    KIND_COLLECTION,
    KIND_ORDER_MESSAGE, KIND_PAYMENT_RECEIPT, KIND_PRODUCT, KIND_REVIEW, KIND_SHIPPING,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

mod cart;
mod filters;
mod orders;

pub use cart::*;
pub use filters::*;
pub use orders::*;

const PRODUCT_CACHE_SIZE: usize = 500;
const SHIPPING_CACHE_SIZE: usize = 100;
const PROCESSED_EVENTS_CACHE_SIZE: usize = 1000;

/// Product cache (keyed by naddr string)
pub static PRODUCTS_CACHE: GlobalSignal<LruCache<String, Product>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PRODUCT_CACHE_SIZE).unwrap()));

/// Whether the shop store has been initialized
pub static SHOP_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Currently loading products
pub static LOADING_PRODUCTS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Shipping options cache (keyed by naddr string)
pub static SHIPPING_CACHE: GlobalSignal<LruCache<String, ShippingOption>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(SHIPPING_CACHE_SIZE).unwrap()));

/// Reviews cache (keyed by product coordinate -> Vec<Review>)
pub static REVIEWS_CACHE: GlobalSignal<HashMap<String, Vec<ProductReview>>> =
    GlobalSignal::new(HashMap::new);

/// Shopping cart items (local, not persisted to Nostr)
pub static CART_ITEMS: GlobalSignal<Vec<CartItem>> = GlobalSignal::new(Vec::new);

/// Cart total in sats (computed from items)
pub static CART_TOTAL_SATS: GlobalSignal<u64> = GlobalSignal::new(|| 0);

/// Orders where current user is the buyer
pub static BUYER_ORDERS: GlobalSignal<Vec<ShopOrder>> = GlobalSignal::new(Vec::new);

/// Orders where current user is the merchant/seller
pub static SELLER_ORDERS: GlobalSignal<Vec<ShopOrder>> = GlobalSignal::new(Vec::new);

/// IndexedDB database handle for order persistence
pub static SHOP_DB: GlobalSignal<Option<Arc<ShopDatabase>>> = GlobalSignal::new(|| None);

/// Processed gift wrap event IDs to prevent duplicate order message processing
/// This prevents the same order update from being applied multiple times
/// Uses LRU cache to prevent unbounded memory growth
pub static PROCESSED_ORDER_EVENTS: GlobalSignal<LruCache<String, ()>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PROCESSED_EVENTS_CACHE_SIZE).unwrap()));

/// Flag to track if orders have been loaded from DB for this session
/// Prevents skipping reload when BUYER_ORDERS/SELLER_ORDERS happen to be empty
static ORDERS_LOADED_FROM_DB: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Current user's products (for merchant dashboard)
pub static MY_PRODUCTS: GlobalSignal<Vec<Product>> = GlobalSignal::new(Vec::new);

/// Loading state for my products
pub static LOADING_MY_PRODUCTS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Current user's collections (Kind 30405)
pub static MY_COLLECTIONS: GlobalSignal<Vec<ProductCollection>> = GlobalSignal::new(Vec::new);

/// Loading state for collections
pub static LOADING_MY_COLLECTIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Get a product from cache by naddr
pub fn get_cached_product(naddr: &str) -> Option<Product> {
    PRODUCTS_CACHE.read().peek(naddr).cloned()
}

/// Cache a product
pub fn cache_product(product: Product) {
    let mut cache = PRODUCTS_CACHE.write();
    // Addressable event (kind 30402): newest `created_at` wins (replaceable semantics).
    // Guards against a stale DB-cached copy winning over a fresher relay copy.
    if let Some(existing) = cache.peek(&product.naddr) {
        if product.created_at <= existing.created_at {
            return;
        }
    }
    cache.put(product.naddr.clone(), product);
}

/// Parse and cache products from events
pub fn cache_product_events(events: &[NostrEvent]) {
    let mut cache = PRODUCTS_CACHE.write();
    for event in events {
        if let Ok(product) = parse_product(event) {
            if let Some(existing) = cache.peek(&product.naddr) {
                if product.created_at <= existing.created_at {
                    continue;
                }
            }
            cache.put(product.naddr.clone(), product);
        }
    }
}

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
    let mut shipping_options = Vec::new();
    for coord in coordinates {
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
    let reviews = cache.entry(review.product_coordinate.clone()).or_default();
    if !reviews.iter().any(|r| r.event_id == review.event_id) {
        reviews.push(review);
        reviews.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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

/// Fetch products with limit
pub async fn fetch_products(limit: usize) -> Result<Vec<Product>> {
    *LOADING_PRODUCTS.write() = true;
    // Gate on relay readiness so authenticated users query their NIP-65 pool, not the
    // bootstrap defaults. No-op for logged-out users.
    crate::stores::relay::wait_for_user_relays(Duration::from_secs(5), "shop::fetch_products")
        .await;
    let filter = products_filter(limit);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
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

/// Fetch ALL marketplace products for the browse page: DB-first (the accumulating SDK
/// cache) merged with a fresh no-limit relay pull, deduped by coordinate (newest
/// `created_at` wins).
///
/// Unlike `fetch_products`/`fetch_events_aggregated` (which return a stale DB-only partial
/// and silently background-sync), this returns the full merged set so the page can render
/// and paginate client-side. Breadth accumulates across sessions via the SDK database
/// (auto-stored on every relay EVENT). Anonymous-safe: `wait_for_user_relays` is a no-op
/// when no signer is attached, so logged-out users browse on the default pool immediately.
pub async fn fetch_all_products() -> Result<Vec<Product>> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    crate::stores::relay::wait_for_user_relays(Duration::from_secs(5), "shop::fetch_all_products")
        .await;
    let filter = Filter::new().kind(Kind::Custom(KIND_PRODUCT)); // no limit
    // 1. DB-first: the accumulated cache, painted instantly and grown by prior visits.
    let mut merged: HashMap<String, Product> = match client.database().query(filter.clone()).await {
        Ok(db_events) => db_events
            .iter()
            .filter_map(|e| parse_product(e).ok())
            .map(|p| (p.naddr.clone(), p))
            .collect(),
        Err(e) => {
            log::warn!("Shop DB query failed, starting from empty cache: {}", e);
            HashMap::new()
        }
    };
    // 2. Fresh relay pull (no limit). Bounded by EOSE + timeout; a partial result is fine
    //    because the DB already holds previously-fetched listings.
    crate::stores::relay::connection::ensure_relays_ready(&client).await;
    match client.fetch_events(filter, Duration::from_secs(30)).await {
        Ok(events) => {
            let events: Vec<NostrEvent> = events.into_iter().collect();
            log::info!("Shop browse: {} fresh relay events", events.len());
            cache_product_events(&events); // newest-wins into PRODUCTS_CACHE
            for event in &events {
                if let Ok(product) = parse_product(event) {
                    merged
                        .entry(product.naddr.clone())
                        .and_modify(|existing| {
                            if product.created_at > existing.created_at {
                                *existing = product.clone();
                            }
                        })
                        .or_insert(product);
                }
            }
        }
        Err(e) => log::warn!("Shop browse relay fetch failed (using cache): {}", e),
    }
    // 3. Return merged set, newest first.
    let mut all: Vec<Product> = merged.into_values().collect();
    all.sort_by_key(|p| std::cmp::Reverse(p.created_at));
    Ok(all)
}

/// Fetch products with pagination support
#[allow(dead_code)]
pub async fn fetch_products_paginated(limit: usize, until: Option<u64>) -> Result<Vec<Product>> {
    crate::stores::relay::wait_for_user_relays(Duration::from_secs(5), "shop::fetch_products_paginated")
        .await;
    let filter = products_filter_paginated(limit, until);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
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
    // Outbox routing: gossip-route to each merchant's NIP-65 write relays so a merchant
    // whose products live only on their own relays is reachable. (waits for user relays)
    let events =
        crate::stores::nostr_client::fetch_events_aggregated_outbox(filter, Duration::from_secs(10))
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
    if let Some(cached) = get_cached_product(naddr) {
        return Ok(Some(cached));
    }
    let nip19 = Nip19Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let coordinate = nip19.coordinate;
    let pk = coordinate.public_key;
    let identifier = coordinate.identifier.clone();
    let filter = product_filter_by_coordinate(pk, &identifier);
    // Outbox routing to the author's write relays (waits for user relays).
    let events =
        crate::stores::nostr_client::fetch_events_aggregated_outbox(filter, Duration::from_secs(10))
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
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
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
    crate::stores::relay::wait_for_user_relays(Duration::from_secs(5), "shop::fetch_product_reviews")
        .await;
    let filter = reviews_filter_for_product(product_coordinate, 50);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    cache_review_events(&events);
    Ok(get_product_reviews(product_coordinate))
}

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
    HANDLER_RECS_PUBLISHED.store(false, AtomicOrdering::SeqCst);
    *CART_TOTAL_SATS.write() = 0;
    *SHOP_INITIALIZED.write() = false;
}

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
    /// Shipping option references (spec `shipping_option` coordinates, e.g. "30406:<pubkey>:<d>").
    pub shipping_options: Vec<String>,
    /// Product condition: new, like_new, used, fair, refurbished
    pub condition: Option<String>,
    /// Original publish timestamp to preserve across edits (NIP-99 `published_at`).
    /// `None` on create → the builder stamps it with the current time.
    pub published_at: Option<u64>,
    /// Listing status (NIP-99 base spec): "active" or "sold". `None` = no tag emitted.
    pub status: Option<String>,
    /// Human-readable location (optional, market-spec `location` tag).
    pub location: Option<String>,
    /// Geohash for precise location (optional, market-spec `g` tag).
    pub geohash: Option<String>,
    /// Optional short summary distinct from the long `content` description (market-spec).
    /// When `None`, the summary tag mirrors the description (legacy behavior).
    pub summary_override: Option<String>,
    /// Product classification (market-spec `type` tag). Defaults to Simple.
    pub product_type: ProductType,
    /// Parent product coordinate for variations (market-spec `a` -> "30402:...").
    pub parent_product: Option<String>,
}

/// Build the tag set for a kind-30402 product event from form data.
///
/// Centralized so `publish_product` and `update_product` emit identical tags and
/// cannot drift from each other (or from `parse_product`). Emits the market-spec
/// `type`/`shipping_option`/`published_at` tags.
fn build_product_tags(data: &ProductFormData, d_tag: &str) -> Vec<Tag> {
    use nostr_sdk::TagKind;
    let format = if data.is_digital {
        ProductFormat::Digital
    } else {
        ProductFormat::Physical
    };
    let mut tags = vec![
        Tag::identifier(d_tag),
        Tag::custom(TagKind::custom("title"), vec![data.title.clone()]),
        // NIP-31 alt: human-readable description for clients that don't know kind 30402.
        Tag::custom(
            TagKind::custom("alt"),
            vec![format!("Product listing: {}", data.title)],
        ),
        Tag::custom(
            TagKind::custom("price"),
            vec![data.price_amount.to_string(), data.price_currency.clone()],
        ),
        // market-spec product classification: ["type", "<simple|variable|variation>", "<digital|physical>"]
        Tag::custom(
            TagKind::custom("type"),
            vec![
                data.product_type.as_str().to_string(),
                format.as_str().to_string(),
            ],
        ),
        // NIP-99 `published_at` (base spec). Preserve original on edit, else stamp now.
        Tag::custom(
            TagKind::custom("published_at"),
            vec![data
                .published_at
                .unwrap_or_else(crate::utils::nip99::now_secs)
                .to_string()],
        ),
    ];
    // Summary: a distinct short summary if provided, else mirror the description (legacy).
    let summary_text = data
        .summary_override
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| data.description.clone());
    if !summary_text.is_empty() {
        tags.push(Tag::custom(
            TagKind::custom("summary"),
            vec![summary_text],
        ));
    }
    // Variation -> parent product reference (market-spec `a` tag, kind 30402).
    if data.product_type == ProductType::Variation {
        if let Some(parent) = &data.parent_product {
            if !parent.is_empty() {
                tags.push(Tag::custom(TagKind::a(), vec![parent.clone()]));
            }
        }
    }
    for img_url in &data.images {
        if !img_url.is_empty() {
            tags.push(Tag::custom(
                TagKind::custom("image"),
                vec![img_url.clone()],
            ));
        }
    }
    for cat in &data.categories {
        if !cat.is_empty() {
            tags.push(Tag::hashtag(cat));
        }
    }
    if let Some(stock) = data.stock {
        tags.push(Tag::custom(
            TagKind::custom("stock"),
            vec![stock.to_string()],
        ));
    }
    for (key, value) in &data.specs {
        tags.push(Tag::custom(
            TagKind::custom("spec"),
            vec![key.clone(), value.clone()],
        ));
    }
    // Physical products reference shipping options by coordinate (market-spec `shipping_option`).
    if !data.is_digital {
        for opt in &data.shipping_options {
            if !opt.is_empty() {
                tags.push(Tag::custom(
                    TagKind::custom("shipping_option"),
                    vec![opt.clone()],
                ));
            }
        }
    }
    if let Some(ref condition) = data.condition {
        if !condition.is_empty() {
            tags.push(Tag::custom(
                TagKind::custom("condition"),
                vec![condition.clone()],
            ));
        }
    }
    if let Some(ref status) = data.status {
        if !status.is_empty() {
            // NIP-99 base spec: ["status", "active"|"sold"]
            tags.push(Tag::custom(
                TagKind::custom("status"),
                vec![status.clone()],
            ));
        }
    }
    if let Some(ref location) = data.location {
        if !location.is_empty() {
            tags.push(Tag::custom(
                TagKind::custom("location"),
                vec![location.clone()],
            ));
        }
    }
    if let Some(ref geohash) = data.geohash {
        if !geohash.is_empty() {
            tags.push(Tag::custom(TagKind::custom("g"), vec![geohash.clone()]));
        }
    }
    tags
}

/// Publish a new product (Kind 30402)
pub async fn publish_product(data: ProductFormData) -> Result<String> {
    let _client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    if data.title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    if data.price_amount <= 0.0 {
        return Err("Price must be greater than 0".to_string());
    }
    let d_tag = crate::utils::format::generate_unique_id();
    let tags = build_product_tags(&data, &d_tag);
    let content = data.description.clone();
    let event_builder = EventBuilder::new(Kind::Custom(KIND_PRODUCT), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(event_builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        std::collections::HashMap::new(),
    ).await;
    // Advertise nostr.blue as a marketplace handler (NIP-89) on first listing.
    spawn(async {
        let _ = publish_marketplace_handler_recs_if_needed().await;
    });
    spawn(async {
        if let Err(e) = fetch_my_products().await {
            log::error!("Failed to refresh my products: {}", e);
        }
    });
    Ok(d_tag)
}

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
    let _client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    // market-spec: the review `d` MUST be the product reference "a:30402:<pubkey>:<d>".
    // `reviews_filter_for_product` matches this d-tag, so this makes our own reviews findable.
    let d_tag = format!("a:{}", product_coordinate);
    let mut tags = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::custom("a"), vec![product_coordinate.to_string()]),
        Tag::custom(
            TagKind::custom("alt"),
            vec!["Product review".to_string()],
        ),
        Tag::custom(
            TagKind::custom("rating"),
            vec![format!("{:.1}", overall_rating), "thumb".to_string()],
        ),
    ];
    if let Some(v) = value_rating {
        tags.push(Tag::custom(
            TagKind::custom("rating"),
            vec![format!("{:.1}", v), "value".to_string()],
        ));
    }
    if let Some(q) = quality_rating {
        tags.push(Tag::custom(
            TagKind::custom("rating"),
            vec![format!("{:.1}", q), "quality".to_string()],
        ));
    }
    if let Some(d) = delivery_rating {
        tags.push(Tag::custom(
            TagKind::custom("rating"),
            vec![format!("{:.1}", d), "delivery".to_string()],
        ));
    }
    if let Some(c) = communication_rating {
        tags.push(Tag::custom(
            TagKind::custom("rating"),
            vec![format!("{:.1}", c), "communication".to_string()],
        ));
    }
    let event_builder = EventBuilder::new(Kind::Custom(KIND_REVIEW), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(event_builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(d_tag)
}

/// Fetch a collection by naddr
pub async fn fetch_collection_by_naddr(naddr: &str) -> Result<Option<ProductCollection>> {
    if nostr_client::NOSTR_CLIENT.read().is_none() {
        return Err("Client not initialized".to_string());
    }
    let coordinate = nostr_sdk::nips::nip19::Nip19::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;
    let (pubkey, d_tag) = match &coordinate {
        nostr_sdk::nips::nip19::Nip19::Coordinate(c) => (c.public_key, c.identifier.clone()),
        _ => return Err("Not a valid collection address".to_string()),
    };
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_COLLECTION))
        .author(pubkey)
        .identifier(&d_tag)
        .limit(1);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
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
        let parts: Vec<&str> = coord.splitn(3, ':').collect();
        if parts.len() >= 3 {
            let pubkey = PublicKey::parse(parts[1]).ok();
            let d_tag = parts[2];
            if let Some(pk) = pubkey {
                let filter = Filter::new()
                    .kind(Kind::Custom(KIND_PRODUCT))
                    .author(pk)
                    .identifier(d_tag)
                    .limit(1);
                if let Ok(events) =
                    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await
                {
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
    pub products: Vec<String>,
    pub shipping_options: Vec<String>,
}

/// Data for creating/updating a shipping option (Kind 30406).
#[derive(Clone, Debug, Default)]
pub struct ShippingOptionFormData {
    pub title: String,
    /// `None` on create → a new d-tag is generated.
    pub d_tag: Option<String>,
    pub base_price: f64,
    pub currency: String,
    /// ISO 3166-1 alpha-2 country codes.
    pub countries: Vec<String>,
    /// Service type: "standard" | "express" | "overnight" | "pickup".
    pub service: String,
    pub carrier: Option<String>,
    /// ISO 3166-2 region codes.
    pub regions: Vec<String>,
    pub duration_min: Option<u32>,
    pub duration_max: Option<u32>,
    /// ISO 8601 duration unit: "H" | "D" | "W".
    pub duration_unit: Option<String>,
    pub location: Option<String>,
    pub geohash: Option<String>,
    pub description: String,
}

/// Create or update a shipping option (Kind 30406) per the market-spec.
pub async fn publish_shipping_option(data: ShippingOptionFormData) -> Result<String> {
    use nostr_sdk::TagKind;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    if data.title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    if data.countries.is_empty() {
        return Err("At least one country is required".to_string());
    }
    if ShippingService::from_str(&data.service).is_none() {
        return Err(format!(
            "Invalid service '{}': must be standard|express|overnight|pickup",
            data.service
        ));
    }
    let d_tag = data
        .d_tag
        .clone()
        .unwrap_or_else(crate::utils::format::generate_unique_id);
    let mut tags = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::custom("title"), vec![data.title.clone()]),
        Tag::custom(
            TagKind::custom("alt"),
            vec![format!("Shipping option: {}", data.title)],
        ),
        Tag::custom(
            TagKind::custom("price"),
            vec![data.base_price.to_string(), data.currency.clone()],
        ),
        Tag::custom(TagKind::custom("country"), data.countries.clone()),
        Tag::custom(TagKind::custom("service"), vec![data.service.clone()]),
    ];
    if let Some(carrier) = &data.carrier {
        if !carrier.is_empty() {
            tags.push(Tag::custom(
                TagKind::custom("carrier"),
                vec![carrier.clone()],
            ));
        }
    }
    if !data.regions.is_empty() {
        tags.push(Tag::custom(TagKind::custom("region"), data.regions.clone()));
    }
    if let (Some(min), Some(max), Some(unit)) =
        (data.duration_min, data.duration_max, &data.duration_unit)
    {
        tags.push(Tag::custom(
            TagKind::custom("duration"),
            vec![min.to_string(), max.to_string(), unit.clone()],
        ));
    }
    if let Some(loc) = &data.location {
        if !loc.is_empty() {
            tags.push(Tag::custom(TagKind::custom("location"), vec![loc.clone()]));
        }
    }
    if let Some(g) = &data.geohash {
        if !g.is_empty() {
            tags.push(Tag::custom(TagKind::custom("g"), vec![g.clone()]));
        }
    }
    let event = crate::stores::publish_queue::signing::sign_event_builder(
        EventBuilder::new(Kind::Custom(KIND_SHIPPING), data.description.clone()).tags(tags),
    )
    .await
    .map_err(|e| format!("Failed to sign shipping option: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        HashMap::new(),
    )
    .await;
    Ok(d_tag)
}

/// Fetch current user's collections
pub async fn fetch_my_collections() -> Result<Vec<ProductCollection>> {
    let pubkey =
        nostr_client::get_cached_pubkey().map_err(|e| format!("Not authenticated: {}", e))?;
    *LOADING_MY_COLLECTIONS.write() = true;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_COLLECTION))
        .author(pubkey)
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
    collections.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    *MY_COLLECTIONS.write() = collections.clone();
    *LOADING_MY_COLLECTIONS.write() = false;
    Ok(collections)
}

/// Create or update a collection (Kind 30405)
pub async fn publish_collection(
    data: CollectionFormData,
    existing_d_tag: Option<String>,
) -> Result<String> {
    use nostr_sdk::TagKind;
    let _client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    if data.title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    let d_tag = existing_d_tag.unwrap_or_else(crate::utils::format::generate_unique_id);
    let mut tags = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::custom("title"), vec![data.title.clone()]),
        Tag::custom(
            TagKind::custom("alt"),
            vec![format!("Product collection: {}", data.title)],
        ),
    ];
    if !data.description.trim().is_empty() {
        tags.push(Tag::custom(
            TagKind::custom("summary"),
            vec![data.description.clone()],
        ));
    }
    if let Some(ref img) = data.image {
        if !img.trim().is_empty() {
            tags.push(Tag::custom(TagKind::custom("image"), vec![img.clone()]));
        }
    }
    for coord in &data.products {
        tags.push(Tag::custom(TagKind::a(), vec![coord.clone()]));
    }
    for shipping in &data.shipping_options {
        tags.push(Tag::custom(
            TagKind::custom("shipping_option"),
            vec![shipping.clone()],
        ));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_COLLECTION), &data.description).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        std::collections::HashMap::new(),
    ).await;
    let _ = fetch_my_collections().await;
    Ok(d_tag)
}

/// Delete a collection
pub async fn delete_collection(d_tag: &str) -> Result<()> {
    let _client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let collection = MY_COLLECTIONS
        .read()
        .iter()
        .find(|c| c.d_tag == d_tag)
        .cloned()
        .ok_or("Collection not found")?;
    let event_id =
        EventId::parse(&collection.event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    use nostr::nips::nip09::EventDeletionRequest;
    let deletion_request = EventDeletionRequest::new().id(event_id);
    let delete_builder = EventBuilder::delete(deletion_request);
    let event = crate::stores::publish_queue::signing::sign_event_builder(delete_builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        std::collections::HashMap::new(),
    ).await;
    MY_COLLECTIONS.write().retain(|c| c.d_tag != d_tag);
    Ok(())
}

/// Search products by query using NIP-50, with fallback to local filtering
pub async fn search_products(query: &str, limit: usize) -> Result<Vec<Product>> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let search_filter = Filter::new()
        .kind(Kind::Custom(KIND_PRODUCT))
        .search(query)
        .limit(limit);
    // NIP-50 search requires dedicated search relays (kind 10007); the default READ pool
    // does not serve `search` filters. Target the configured SEARCH relays explicitly.
    let search_relays: Vec<String> = {
        let configured = crate::stores::relay::SEARCH_RELAYS.read().clone();
        if configured.is_empty() {
            crate::stores::relay::nip65::DEFAULT_SEARCH_RELAYS
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            configured
        }
    };
    let search_events: Option<Vec<Product>> = if !search_relays.is_empty() {
        match client
            .fetch_events_from(search_relays.clone(), search_filter, Duration::from_secs(5))
            .await
        {
            Ok(events) if !events.is_empty() => {
                log::debug!(
                    "NIP-50 search returned {} products for '{}'",
                    events.len(),
                    query
                );
                Some(
                    events
                        .iter()
                        .filter_map(|e| parse_product(e).ok())
                        .filter(|p| p.is_visible())
                        .collect(),
                )
            }
            _ => None,
        }
    } else {
        None
    };
    if let Some(products) = search_events {
        return Ok(products);
    }
    log::debug!(
        "NIP-50 search returned no results, falling back to local filter for '{}'",
        query
    );
    let all_products = fetch_products(200).await?;
    let query_lower = query.to_lowercase();
    let filtered: Vec<Product> = all_products
        .into_iter()
        .filter(|p| {
            p.is_visible()
                && (p.title.to_lowercase().contains(&query_lower)
                    || p.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || p.categories
                        .iter()
                        .any(|c| c.to_lowercase().contains(&query_lower)))
        })
        .take(limit)
        .collect();
    Ok(filtered)
}

/// Delete a product by publishing a deletion event (Kind 5)
pub async fn delete_product(product_naddr: &str, d_tag: &str) -> Result<()> {
    use nostr_sdk::TagKind;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let _signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;
    let coordinate = format!("{}:{}:{}", KIND_PRODUCT, pubkey.to_hex(), d_tag);
    // NIP-09: include both the addressable `a` coordinate and a `k` tag so relays/clients
    // know which kind is being deleted.
    let tags = vec![
        Tag::custom(TagKind::custom("a"), vec![coordinate]),
        Tag::custom(TagKind::k(), vec![KIND_PRODUCT.to_string()]),
    ];
    let event = crate::stores::publish_queue::signing::sign_event_builder(
        EventBuilder::new(Kind::EventDeletion, "Product deleted").tags(tags),
    )
    .await
    .map_err(|e| format!("Failed to sign deletion event: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        std::collections::HashMap::new(),
    ).await;
    MY_PRODUCTS.write().retain(|p| p.naddr != product_naddr);
    PRODUCTS_CACHE.write().pop(&product_naddr.to_string());
    Ok(())
}

/// Update a product by republishing with the same d-tag
pub async fn update_product(d_tag: &str, data: ProductFormData) -> Result<String> {
    let _client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    if data.title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    if data.price_amount <= 0.0 {
        return Err("Price must be greater than 0".to_string());
    }
    let tags = build_product_tags(&data, d_tag);
    let event = crate::stores::publish_queue::signing::sign_event_builder(
        EventBuilder::new(Kind::Custom(KIND_PRODUCT), data.description.clone()).tags(tags),
    )
    .await
    .map_err(|e| format!("Failed to sign product event: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(d_tag.to_string())
}

/// Mark a product as sold (NIP-99 `status: "sold"`) and decrement its stock by one.
///
/// Re-publishes the listing with the same `d` tag via the publish queue, preserving all
/// other fields and the original `published_at`. Safe to call from a component; the
/// publish queue itself is durable.
pub async fn mark_product_sold(product: Product) -> Result<()> {
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let decremented_stock = product.stock.map(|s| s.saturating_sub(1));
    let data = ProductFormData {
        title: product.title.clone(),
        description: product
            .description
            .clone()
            .or_else(|| product.summary.clone())
            .unwrap_or_default(),
        price_amount: product.price.amount,
        price_currency: product.price.currency.clone(),
        images: product.images.iter().map(|i| i.url.clone()).collect(),
        categories: product.categories.clone(),
        is_digital: product.format.is_digital(),
        stock: decremented_stock,
        specs: product
            .specs
            .iter()
            .map(|s| (s.key.clone(), s.value.clone()))
            .collect(),
        shipping_options: product.shipping_options.clone(),
        condition: product.condition.clone(),
        published_at: product.published_at.or(Some(product.created_at)),
        status: Some("sold".to_string()),
        location: product.location.clone(),
        geohash: product.geohash.clone(),
        summary_override: None,
        product_type: product.product_type,
        parent_product: product.parent_product.clone(),
    };
    update_product(&product.d_tag, data).await?;
    Ok(())
}

use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
static HANDLER_RECS_PUBLISHED: AtomicBool = AtomicBool::new(false);
const MARKETPLACE_HANDLER_DTAG: &str = "nostr-blue-marketplace";
const MARKETPLACE_WEB_BASE: &str = "https://nostr.blue";

/// Publish NIP-89 handler recommendations (kinds 31990 + 31989) advertising nostr.blue as
/// a marketplace client for kinds 30402/30405/30406, so other clients can discover and
/// route to it. Runs at most once per session (these are replaceable/addressable events,
/// so re-publishing is harmless but wasteful). No-op without a signer.
pub async fn publish_marketplace_handler_recs_if_needed() -> Result<()> {
    if HANDLER_RECS_PUBLISHED.load(AtomicOrdering::SeqCst) {
        return Ok(());
    }
    if !*nostr_client::HAS_SIGNER.read() {
        return Ok(());
    }
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;
    // Kind 31990: the handler description (addressable).
    let handler_tags = vec![
        Tag::identifier(MARKETPLACE_HANDLER_DTAG),
        Tag::custom(
            TagKind::custom("name"),
            vec!["nostr.blue Marketplace".to_string()],
        ),
        Tag::custom(TagKind::custom("alt"), vec!["Marketplace handler".to_string()]),
        Tag::custom(TagKind::k(), vec![KIND_PRODUCT.to_string()]),
        Tag::custom(TagKind::k(), vec![KIND_COLLECTION.to_string()]),
        Tag::custom(TagKind::k(), vec![KIND_SHIPPING.to_string()]),
        Tag::custom(
            TagKind::custom("web"),
            vec![
                format!("{}/marketplace/{{npub}}", MARKETPLACE_WEB_BASE),
                "npub".to_string(),
            ],
        ),
        Tag::custom(
            TagKind::custom("web"),
            vec![
                format!("{}/marketplace/product/{{naddr}}", MARKETPLACE_WEB_BASE),
                "naddr".to_string(),
            ],
        ),
    ];
    let handler_event =
        crate::stores::publish_queue::signing::sign_event_builder(
            EventBuilder::new(Kind::Custom(31990), "").tags(handler_tags),
        )
        .await?;
    crate::stores::publish_queue::enqueue(
        handler_event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        HashMap::new(),
    )
    .await;
    // Kind 31989: the recommendation pointing to our handler (replaceable, d = handled kind).
    let handler_coord = format!(
        "31990:{}:{}",
        pubkey.to_hex(),
        MARKETPLACE_HANDLER_DTAG
    );
    let rec_tags = vec![
        Tag::identifier(KIND_PRODUCT.to_string()),
        Tag::custom(TagKind::a(), vec![handler_coord]),
    ];
    let rec_event =
        crate::stores::publish_queue::signing::sign_event_builder(
            EventBuilder::new(Kind::Custom(31989), "").tags(rec_tags),
        )
        .await?;
    crate::stores::publish_queue::enqueue(
        rec_event,
        crate::stores::publish_queue::types::QueueEventType::Shop,
        None,
        HashMap::new(),
    )
    .await;
    HANDLER_RECS_PUBLISHED.store(true, AtomicOrdering::SeqCst);
    log::info!("Published NIP-89 marketplace handler recommendations");
    Ok(())
}

/// How a merchant prefers to be paid (market-spec `payment_preference` from kind 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MerchantPaymentPreference {
    /// Merchant handles payment requests manually (default).
    Manual,
    /// Merchant accepts Cashu ecash (ideally with a kind 10019 mint).
    Ecash,
    /// Merchant accepts Lightning via their kind-0 `lud16`.
    Lud16,
}

impl MerchantPaymentPreference {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "manual" => Some(Self::Manual),
            "ecash" | "cashu" => Some(Self::Ecash),
            "lud16" | "lightning" => Some(Self::Lud16),
            _ => None,
        }
    }
}

/// Resolve a merchant's payment preference from their cached kind-0 metadata.
///
/// Uses the explicit `payment_preference` field when set, else infers `lud16` when a
/// Lightning address is present, else falls back to `manual` (the spec default).
pub fn resolve_merchant_payment(merchant_pubkey: &str) -> MerchantPaymentPreference {
    if let Some(profile) = crate::stores::profiles::get_cached_profile(merchant_pubkey) {
        if let Some(pref) = profile.payment_preference() {
            if let Some(mode) = MerchantPaymentPreference::from_str(&pref) {
                return mode;
            }
        }
        if profile
            .lud16
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        {
            return MerchantPaymentPreference::Lud16;
        }
    }
    MerchantPaymentPreference::Manual
}

/// Send ecash to a merchant, locked to their pubkey (NUT-11 P2PK), for the ecash payment
/// mode (N-C). Returns a Cashu token string to embed in the order message.
///
/// Cashu is web + desktop only (not available on `mobile`/`playstore` builds); callers
/// must feature-gate any UI that offers this path.
#[cfg(feature = "cashu")]
#[allow(dead_code)]
pub async fn send_ecash_to_merchant(
    mint_url: String,
    amount_sats: u64,
    merchant_pubkey: String,
) -> Result<String> {
    crate::stores::cashu::send::send_tokens_p2pk(mint_url, amount_sats, merchant_pubkey, None, false)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::nip99::{parse_product, ProductFormat, ProductType};

    /// B1/B2 round-trip guard: `build_product_tags` must emit tags that `parse_product`
    /// reads back identically. A physical product with a shipping option and a published_at
    /// must survive the round-trip (previously `format`/`shipping` tags were orphaned).
    #[test]
    fn test_build_product_tags_round_trip_physical() {
        let data = ProductFormData {
            title: "Test Widget".to_string(),
            description: "A widget".to_string(),
            price_amount: 50.0,
            price_currency: "USD".to_string(),
            images: vec!["https://example.com/widget.png".to_string()],
            categories: vec!["electronics".to_string(), "gadgets".to_string()],
            is_digital: false,
            stock: Some(7),
            specs: vec![("color".to_string(), "red".to_string())],
            shipping_options: vec!["30406:merchant:us-standard".to_string()],
            condition: Some("new".to_string()),
            published_at: Some(1_700_000_000),
            status: None,
            location: None,
            geohash: None,
            summary_override: None,
            product_type: ProductType::Simple,
            parent_product: None,
        };
        let tags = build_product_tags(&data, "test-widget");
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(KIND_PRODUCT), data.description.clone())
            .tags(tags)
            .sign_with_keys(&keys)
            .expect("sign");
        let product = parse_product(&event).expect("parse product");
        assert_eq!(product.title, "Test Widget");
        assert_eq!(product.d_tag, "test-widget");
        assert_eq!(product.price.amount, 50.0);
        assert_eq!(product.product_type, ProductType::Simple);
        assert_eq!(product.format, ProductFormat::Physical); // B1 fixed
        assert_eq!(product.stock, Some(7));
        assert_eq!(product.published_at, Some(1_700_000_000)); // N-A
        assert_eq!(
            product.shipping_options,
            vec!["30406:merchant:us-standard".to_string()]
        ); // B2 fixed
        assert_eq!(product.categories, vec!["electronics", "gadgets"]);
        assert_eq!(product.condition.as_deref(), Some("new"));
        assert_eq!(product.specs.len(), 1);
        assert_eq!(product.images.len(), 1);
    }

    /// A digital product round-trips with format Digital and no shipping_option tags.
    #[test]
    fn test_build_product_tags_round_trip_digital() {
        let data = ProductFormData {
            title: "E-Book".to_string(),
            description: String::new(),
            price_amount: 1000.0,
            price_currency: "sats".to_string(),
            is_digital: true,
            published_at: Some(1_700_000_000),
            ..Default::default()
        };
        let tags = build_product_tags(&data, "ebook");
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(KIND_PRODUCT), "")
            .tags(tags)
            .sign_with_keys(&keys)
            .expect("sign");
        let product = parse_product(&event).expect("parse product");
        assert_eq!(product.format, ProductFormat::Digital);
        assert!(product.shipping_options.is_empty());
        assert!(product.summary.is_none()); // empty description -> no summary tag
        assert!(product.description.is_none());
    }
}
