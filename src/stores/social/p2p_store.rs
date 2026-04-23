//! P2P Store
//! Handles NIP-69 P2P order events - caching, filtering, and state management
#![allow(dead_code)]
use crate::utils::nip69::{parse_p2p_order, Layer, Network, OrderStatus, OrderType, P2POrder};
use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::num::NonZeroUsize;
use std::time::Duration;
const ORDER_CACHE_SIZE: usize = 500;
/// P2P order cache (keyed by naddr string)
pub static P2P_ORDERS_CACHE: GlobalSignal<LruCache<String, P2POrder>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(ORDER_CACHE_SIZE).unwrap()));
/// Whether the P2P store has been initialized
pub static P2P_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Currently loading orders
pub static LOADING_ORDERS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Get an order from cache by naddr
pub fn get_cached_order(naddr: &str) -> Option<P2POrder> {
    P2P_ORDERS_CACHE.read().peek(naddr).cloned()
}
/// Get an order from cache by coordinate string
pub fn get_cached_order_by_coordinate(coordinate: &str) -> Option<P2POrder> {
    let cache = P2P_ORDERS_CACHE.read();
    cache
        .iter()
        .find(|(_, order)| order.coordinate == coordinate)
        .map(|(_, order)| order.clone())
}
/// Cache an order
pub fn cache_order(order: P2POrder) {
    P2P_ORDERS_CACHE.write().put(order.naddr.clone(), order);
}
/// Parse and cache orders from events
pub fn cache_order_events(events: &[NostrEvent]) {
    let mut cache = P2P_ORDERS_CACHE.write();
    for event in events {
        if let Ok(order) = parse_p2p_order(event) {
            cache.put(order.naddr.clone(), order);
        }
    }
}
/// Get all cached orders
pub fn get_all_cached_orders() -> Vec<P2POrder> {
    let cache = P2P_ORDERS_CACHE.read();
    cache.iter().map(|(_, order)| order.clone()).collect()
}
/// Get active orders (pending, not expired)
pub fn get_active_orders() -> Vec<P2POrder> {
    let cache = P2P_ORDERS_CACHE.read();
    cache
        .iter()
        .filter_map(|(_, order)| {
            if order.is_active() {
                Some(order.clone())
            } else {
                None
            }
        })
        .collect()
}
/// Filter state for client-side filtering
#[derive(Clone, Debug)]
pub struct P2PFilterState {
    pub order_type: Option<OrderType>,
    pub status: Option<OrderStatus>,
    pub currency: Option<String>,
    pub payment_method: Option<String>,
    pub layer: Option<Layer>,
    pub network: Option<Network>,
    pub platform: Option<String>,
    pub min_amount: Option<f64>,
    pub max_amount: Option<f64>,
    pub geohash_prefix: Option<String>,
}
impl Default for P2PFilterState {
    fn default() -> Self {
        Self {
            order_type: None,
            status: Some(OrderStatus::Pending),
            currency: None,
            payment_method: None,
            layer: None,
            network: None,
            platform: None,
            min_amount: None,
            max_amount: None,
            geohash_prefix: None,
        }
    }
}
impl P2PFilterState {
    /// Check if any filters are active
    pub fn is_empty(&self) -> bool {
        self.order_type.is_none()
            && self.status.is_none()
            && self.currency.is_none()
            && self.payment_method.is_none()
            && self.layer.is_none()
            && self.network.is_none()
            && self.platform.is_none()
            && self.min_amount.is_none()
            && self.max_amount.is_none()
            && self.geohash_prefix.is_none()
    }
    /// Clear all filters
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
/// Filter orders based on filter state
pub fn filter_orders(orders: &[P2POrder], filters: &P2PFilterState) -> Vec<P2POrder> {
    orders
        .iter()
        .filter(|order| {
            if let Some(ot) = &filters.order_type {
                if &order.order_type != ot {
                    return false;
                }
            }
            if let Some(status) = &filters.status {
                if &order.status != status {
                    return false;
                }
            }
            if let Some(currency) = &filters.currency {
                if &order.currency != currency {
                    return false;
                }
            }
            if let Some(pm) = &filters.payment_method {
                let pm_lower = pm.to_lowercase();
                if !order
                    .payment_methods
                    .iter()
                    .any(|m| m.to_lowercase().contains(&pm_lower))
                {
                    return false;
                }
            }
            if let Some(layer) = &filters.layer {
                if &order.layer != layer {
                    return false;
                }
            }
            if let Some(network) = &filters.network {
                if &order.network != network {
                    return false;
                }
            }
            if let Some(platform) = &filters.platform {
                match &order.platform {
                    Some(op) if op == platform => {}
                    _ => return false,
                }
            }
            if filters.min_amount.is_some() || filters.max_amount.is_some() {
                let filter_amount = filters.max_amount.or(filters.min_amount).unwrap_or(0.0);
                if filter_amount > 0.0 && !order.fiat_amount.overlaps(filter_amount, 0.1) {
                    return false;
                }
            }
            if let Some(prefix) = &filters.geohash_prefix {
                match &order.geohash {
                    Some(gh) if gh.starts_with(prefix) => {}
                    _ => return false,
                }
            }
            true
        })
        .cloned()
        .collect()
}
/// Sort orders by various criteria
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OrderSortBy {
    #[default]
    Newest,
    Oldest,
    PremiumLow,
    PremiumHigh,
    AmountLow,
    AmountHigh,
}
impl OrderSortBy {
    pub fn label(&self) -> &'static str {
        match self {
            OrderSortBy::Newest => "Newest First",
            OrderSortBy::Oldest => "Oldest First",
            OrderSortBy::PremiumLow => "Premium: Low to High",
            OrderSortBy::PremiumHigh => "Premium: High to Low",
            OrderSortBy::AmountLow => "Amount: Low to High",
            OrderSortBy::AmountHigh => "Amount: High to Low",
        }
    }
}
/// Sort orders
pub fn sort_orders(orders: &mut [P2POrder], sort_by: OrderSortBy) {
    match sort_by {
        OrderSortBy::Newest => orders.sort_by_key(|b| std::cmp::Reverse(b.created_at)),
        OrderSortBy::Oldest => orders.sort_by_key(|a| a.created_at),
        OrderSortBy::PremiumLow => orders.sort_by(|a, b| {
            let pa = a.premium.unwrap_or(0.0);
            let pb = b.premium.unwrap_or(0.0);
            pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
        }),
        OrderSortBy::PremiumHigh => orders.sort_by(|a, b| {
            let pa = a.premium.unwrap_or(0.0);
            let pb = b.premium.unwrap_or(0.0);
            pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
        }),
        OrderSortBy::AmountLow => orders.sort_by(|a, b| {
            let aa = match &a.fiat_amount {
                crate::utils::nip69::FiatAmount::Fixed(v) => *v,
                crate::utils::nip69::FiatAmount::Range { min, .. } => *min,
            };
            let ab = match &b.fiat_amount {
                crate::utils::nip69::FiatAmount::Fixed(v) => *v,
                crate::utils::nip69::FiatAmount::Range { min, .. } => *min,
            };
            aa.partial_cmp(&ab).unwrap_or(std::cmp::Ordering::Equal)
        }),
        OrderSortBy::AmountHigh => orders.sort_by(|a, b| {
            let aa = match &a.fiat_amount {
                crate::utils::nip69::FiatAmount::Fixed(v) => *v,
                crate::utils::nip69::FiatAmount::Range { max, .. } => *max,
            };
            let ab = match &b.fiat_amount {
                crate::utils::nip69::FiatAmount::Fixed(v) => *v,
                crate::utils::nip69::FiatAmount::Range { max, .. } => *max,
            };
            ab.partial_cmp(&aa).unwrap_or(std::cmp::Ordering::Equal)
        }),
    }
}
/// Build filter for fetching all P2P orders (with optional limit)
pub fn orders_filter(limit: usize) -> Filter {
    Filter::new().kind(Kind::PeerToPeerOrder).limit(limit)
}
/// Build filter for fetching ALL P2P orders without limit
/// Uses a 30-day lookback for performance
pub fn orders_filter_all() -> Filter {
    let now_secs = crate::platform::timestamp::now_secs();
    let thirty_days_ago = now_secs.saturating_sub(30 * 24 * 60 * 60);
    Filter::new()
        .kind(Kind::PeerToPeerOrder)
        .since(Timestamp::from(thirty_days_ago))
}
/// Build filter for fetching orders by author
pub fn orders_filter_by_author(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::PeerToPeerOrder)
        .author(pubkey)
        .limit(limit)
}
/// Build filter for fetching a specific order by coordinate
pub fn order_filter_by_coordinate(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::PeerToPeerOrder)
        .author(pubkey)
        .identifier(identifier)
}
/// Build filter for orders by order type (buy/sell)
pub fn orders_filter_by_type(order_type: OrderType, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::PeerToPeerOrder)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::K), order_type.as_str())
        .limit(limit)
}
/// Build filter for orders by currency
pub fn orders_filter_by_currency(currency: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::PeerToPeerOrder)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::F), currency)
        .limit(limit)
}
/// Build filter for orders by status
pub fn orders_filter_by_status(status: OrderStatus, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::PeerToPeerOrder)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::S), status.as_str())
        .limit(limit)
}
/// Build filter for orders by platform
pub fn orders_filter_by_platform(platform: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::PeerToPeerOrder)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Y), platform)
        .limit(limit)
}
/// Fetch P2P orders with filters (limited)
pub async fn fetch_orders(limit: usize) -> std::result::Result<Vec<P2POrder>, String> {
    *LOADING_ORDERS.write() = true;
    let filter = orders_filter(limit);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
    *LOADING_ORDERS.write() = false;
    match result {
        Ok(events) => {
            cache_order_events(&events);
            let orders: Vec<P2POrder> = events
                .iter()
                .filter_map(|e| parse_p2p_order(e).ok())
                .collect();
            Ok(orders)
        }
        Err(e) => Err(e),
    }
}
/// Fetch ALL P2P orders (full order book)
/// Uses 30-day lookback, no limit, auto-deduplicated by relay pool
pub async fn fetch_all_orders() -> std::result::Result<Vec<P2POrder>, String> {
    *LOADING_ORDERS.write() = true;
    let filter = orders_filter_all();
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(30)).await;
    *LOADING_ORDERS.write() = false;
    match result {
        Ok(events) => {
            cache_order_events(&events);
            let mut dedup = std::collections::HashMap::<String, P2POrder>::new();
            for order in events.iter().filter_map(|e| parse_p2p_order(e).ok()) {
                dedup
                    .entry(order.naddr.clone())
                    .and_modify(|existing| {
                        if order.created_at > existing.created_at {
                            *existing = order.clone();
                        }
                    })
                    .or_insert(order);
            }
            let orders: Vec<P2POrder> = dedup.into_values().collect();
            log::info!("Fetched {} P2P orders for full order book", orders.len());
            Ok(orders)
        }
        Err(e) => Err(e),
    }
}
/// Fetch orders by author
pub async fn fetch_orders_by_author(
    pubkey: &str,
    limit: usize,
) -> std::result::Result<Vec<P2POrder>, String> {
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = orders_filter_by_author(pk, limit);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    cache_order_events(&events);
    let orders: Vec<P2POrder> = events
        .iter()
        .filter_map(|e| parse_p2p_order(e).ok())
        .collect();
    Ok(orders)
}
/// Fetch a specific order by naddr
pub async fn fetch_order_by_naddr(naddr: &str) -> std::result::Result<Option<P2POrder>, String> {
    if let Some(cached) = get_cached_order(naddr) {
        return Ok(Some(cached));
    }
    let nip19 = Nip19Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let coordinate = nip19.coordinate;
    let pk = coordinate.public_key;
    let identifier = coordinate.identifier.clone();
    let filter = order_filter_by_coordinate(pk, &identifier);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    if let Some(event) = events.first() {
        if let Ok(order) = parse_p2p_order(event) {
            cache_order(order.clone());
            return Ok(Some(order));
        }
    }
    Ok(None)
}
/// Clear all caches (e.g., on logout)
pub fn clear_caches() {
    P2P_ORDERS_CACHE.write().clear();
    *P2P_INITIALIZED.write() = false;
}
/// Get order statistics
pub struct OrderStats {
    pub total_orders: usize,
    pub buy_orders: usize,
    pub sell_orders: usize,
    pub pending_orders: usize,
    pub currencies: Vec<String>,
    pub platforms: Vec<String>,
}
/// Calculate statistics from cached orders
pub fn get_order_stats() -> OrderStats {
    let cache = P2P_ORDERS_CACHE.read();
    let mut buy_count = 0;
    let mut sell_count = 0;
    let mut pending_count = 0;
    let mut currencies = std::collections::HashSet::new();
    let mut platforms = std::collections::HashSet::new();
    for (_, order) in cache.iter() {
        match order.order_type {
            OrderType::Buy => buy_count += 1,
            OrderType::Sell => sell_count += 1,
        }
        if order.status == OrderStatus::Pending {
            pending_count += 1;
        }
        currencies.insert(order.currency.clone());
        if let Some(p) = &order.platform {
            platforms.insert(p.clone());
        }
    }
    OrderStats {
        total_orders: cache.len(),
        buy_orders: buy_count,
        sell_orders: sell_count,
        pending_orders: pending_count,
        currencies: currencies.into_iter().collect(),
        platforms: platforms.into_iter().collect(),
    }
}
/// Derive implied market price from P2P orders (USD only)
///
/// Algorithm:
/// 1. Filter USD orders with explicit amount_sats > 0
/// 2. Calculate implied price: fiat / (sats / 100_000_000)
/// 3. Adjust for premium: market_price = order_price / (1 + premium/100)
/// 4. Return median of all implied prices (robust against outliers)
/// 5. Returns None if insufficient data
pub fn derive_market_price(orders: &[P2POrder]) -> Option<f64> {
    use crate::utils::nip69::FiatAmount;
    let mut implied_prices: Vec<f64> = orders
        .iter()
        .filter(|o| o.currency.eq_ignore_ascii_case("USD"))
        .filter(|o| o.amount_sats > 0)
        .filter(|o| o.is_active())
        .filter_map(|o| {
            let fiat = match &o.fiat_amount {
                FiatAmount::Fixed(amt) => *amt,
                FiatAmount::Range { min, max } => (*min + *max) / 2.0,
            };
            let btc = o.amount_sats as f64 / 100_000_000.0;
            if btc <= 0.0 || fiat <= 0.0 {
                return None;
            }
            let raw_price = fiat / btc;
            let premium = o.premium.unwrap_or(0.0);
            let market_price = raw_price / (1.0 + premium / 100.0);
            if (1_000.0..=1_000_000.0).contains(&market_price) {
                Some(market_price)
            } else {
                None
            }
        })
        .collect();
    if implied_prices.is_empty() {
        return None;
    }
    implied_prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = implied_prices.len() / 2;
    if implied_prices.len() % 2 == 0 && implied_prices.len() > 1 {
        Some((implied_prices[mid - 1] + implied_prices[mid]) / 2.0)
    } else {
        Some(implied_prices[mid])
    }
}
