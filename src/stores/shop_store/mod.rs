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
use crate::stores::indexeddb_database::IndexedDbDatabase;
use crate::stores::nostr_client;
use crate::utils::format::truncate_pubkey;
use crate::utils::nip99::{
    now_secs, parse_collection, parse_product, parse_review, parse_shipping, CartItem, OrderItem,
    OrderMessageType, OrderStatus, Product, ProductCollection, ProductFormat, ProductReview,
    ProductVisibility, ShippingOption, ShippingStatus, ShopOrder, KIND_COLLECTION,
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
pub static SHOP_DB: GlobalSignal<Option<Arc<IndexedDbDatabase>>> = GlobalSignal::new(|| None);

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

/// Fetch products with limit
pub async fn fetch_products(limit: usize) -> Result<Vec<Product>> {
    *LOADING_PRODUCTS.write() = true;
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

/// Fetch products with pagination support
pub async fn fetch_products_paginated(limit: usize, until: Option<u64>) -> Result<Vec<Product>> {
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
    if let Some(cached) = get_cached_product(naddr) {
        return Ok(Some(cached));
    }
    let nip19 = Nip19Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
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
    pub shipping_regions: Vec<String>,
    /// Product condition: new, like_new, used, fair, refurbished
    pub condition: Option<String>,
}

/// Publish a new product (Kind 30402)
pub async fn publish_product(data: ProductFormData) -> Result<String> {
    use nostr_sdk::TagKind;
    let client = nostr_client::NOSTR_CLIENT
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
    let mut tags = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::custom("title"), vec![data.title.clone()]),
        Tag::custom(
            TagKind::custom("price"),
            vec![data.price_amount.to_string(), data.price_currency.clone()],
        ),
    ];
    if !data.description.is_empty() {
        tags.push(Tag::custom(
            TagKind::custom("summary"),
            vec![data.description.clone()],
        ));
    }
    for img_url in &data.images {
        tags.push(Tag::custom(TagKind::custom("image"), vec![img_url.clone()]));
    }
    for cat in &data.categories {
        tags.push(Tag::hashtag(cat));
    }
    let format = if data.is_digital {
        "digital"
    } else {
        "physical"
    };
    tags.push(Tag::custom(
        TagKind::custom("format"),
        vec![format.to_string()],
    ));
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
    if !data.is_digital {
        for region in &data.shipping_regions {
            tags.push(Tag::custom(
                TagKind::custom("shipping"),
                vec![region.clone()],
            ));
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
    let content = data.description.clone();
    let event_builder = EventBuilder::new(Kind::Custom(KIND_PRODUCT), content).tags(tags);
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(event_builder))
        .await
        .map_err(|e| format!("Failed to publish product: {}", e))?;
    log::info!(
        "Published product: {} (event: {:?})",
        data.title,
        output.val
    );
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
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let d_tag = crate::utils::format::generate_unique_id();
    let mut tags = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::custom("a"), vec![product_coordinate.to_string()]),
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
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(event_builder))
        .await
        .map_err(|e| format!("Failed to publish review: {}", e))?;
    log::info!(
        "Published review for {}: {:?}",
        product_coordinate,
        output.val
    );
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
    collections.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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
    let client = nostr_client::NOSTR_CLIENT
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
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish collection: {}", e))?;
    log::info!("Published collection: {} (event: {})", d_tag, output.id());
    let _ = fetch_my_collections().await;
    Ok(d_tag)
}

/// Delete a collection
pub async fn delete_collection(d_tag: &str) -> Result<()> {
    let client = nostr_client::NOSTR_CLIENT
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
    client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(delete_builder))
        .await
        .map_err(|e| format!("Failed to delete collection: {}", e))?;
    MY_COLLECTIONS.write().retain(|c| c.d_tag != d_tag);
    log::info!("Deleted collection: {}", d_tag);
    Ok(())
}

/// Search products by query using NIP-50, with fallback to local filtering
pub async fn search_products(query: &str, limit: usize) -> Result<Vec<Product>> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let search_filter = Filter::new()
        .kind(Kind::Custom(KIND_PRODUCT))
        .search(query)
        .limit(limit);
    let search_result = client
        .fetch_events(search_filter, Duration::from_secs(5))
        .await;
    if let Ok(events) = search_result {
        if !events.is_empty() {
            log::debug!(
                "NIP-50 search returned {} products for '{}'",
                events.len(),
                query
            );
            let products: Vec<Product> = events
                .iter()
                .filter_map(|e| parse_product(e).ok())
                .filter(|p| p.is_visible())
                .collect();
            return Ok(products);
        }
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
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let pubkey = signer
        .get_public_key()
        .await
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;
    let coordinate = format!("{}:{}:{}", KIND_PRODUCT, pubkey.to_hex(), d_tag);
    let tags = vec![Tag::custom(TagKind::custom("a"), vec![coordinate])];
    let event = EventBuilder::new(Kind::EventDeletion, "Product deleted")
        .tags(tags)
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign deletion event: {}", e))?;
    client
        .send_event(&event)
        .await
        .map_err(|e| format!("Failed to send deletion event: {}", e))?;
    MY_PRODUCTS.write().retain(|p| p.naddr != product_naddr);
    PRODUCTS_CACHE.write().pop(&product_naddr.to_string());
    log::info!("Product deleted: {}", d_tag);
    Ok(())
}

/// Update a product by republishing with the same d-tag
pub async fn update_product(d_tag: &str, data: ProductFormData) -> Result<String> {
    use nostr_sdk::TagKind;
    let client = nostr_client::NOSTR_CLIENT
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
    let mut tags = vec![
        Tag::identifier(d_tag),
        Tag::custom(TagKind::custom("title"), vec![data.title.clone()]),
        Tag::custom(
            TagKind::custom("price"),
            vec![data.price_amount.to_string(), data.price_currency.clone()],
        ),
    ];
    if !data.description.is_empty() {
        tags.push(Tag::custom(
            TagKind::custom("summary"),
            vec![data.description.clone()],
        ));
    }
    for img in data.images.iter() {
        if !img.is_empty() {
            tags.push(Tag::custom(TagKind::custom("image"), vec![img.clone()]));
        }
    }
    for cat in data.categories.iter() {
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
    let format = if data.is_digital {
        "digital"
    } else {
        "physical"
    };
    tags.push(Tag::custom(
        TagKind::custom("format"),
        vec![format.to_string()],
    ));
    for (key, value) in &data.specs {
        tags.push(Tag::custom(
            TagKind::custom("spec"),
            vec![key.clone(), value.clone()],
        ));
    }
    if !data.is_digital {
        for region in data.shipping_regions.iter() {
            if !region.is_empty() {
                tags.push(Tag::custom(
                    TagKind::custom("shipping"),
                    vec![region.clone()],
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
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let event = EventBuilder::new(Kind::Custom(KIND_PRODUCT), data.description.clone())
        .tags(tags)
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign product event: {}", e))?;
    client
        .send_event(&event)
        .await
        .map_err(|e| format!("Failed to send product event: {}", e))?;
    log::info!("Product updated: {}", d_tag);
    Ok(d_tag.to_string())
}
