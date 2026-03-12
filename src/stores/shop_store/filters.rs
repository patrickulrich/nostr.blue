//! Product filtering and sorting for the shop store
//!
//! Provides client-side filtering by category, price, format, visibility,
//! merchant, collection, stock, and search query. Also includes sort options
//! and Nostr filter builders for fetching products/reviews.

use super::*;

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
            } else if filters.min_price_sats.is_some() || filters.max_price_sats.is_some() {
                return false;
            }
            if let Some(category) = &filters.category {
                if !product
                    .categories
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(category))
                {
                    return false;
                }
            }
            if let Some(format) = &filters.format {
                if &product.format != format {
                    return false;
                }
            }
            if let Some(visibility) = &filters.visibility {
                if &product.visibility != visibility {
                    return false;
                }
            } else if !product.is_visible() {
                return false;
            }
            if let Some(pubkey) = &filters.merchant_pubkey {
                if &product.pubkey != pubkey {
                    return false;
                }
            }
            if let Some(collection) = &filters.collection {
                if !product.collections.contains(collection) {
                    return false;
                }
            }
            if filters.in_stock_only && !product.is_in_stock() {
                return false;
            }
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
            let a_sats = a.price.to_sats().unwrap_or(u64::MAX);
            let b_sats = b.price.to_sats().unwrap_or(u64::MAX);
            a_sats.cmp(&b_sats)
        }),
        ProductSortBy::PriceHigh => products.sort_by(|a, b| {
            let a_sats = a.price.to_sats().unwrap_or(0);
            let b_sats = b.price.to_sats().unwrap_or(0);
            b_sats.cmp(&a_sats)
        }),
        ProductSortBy::Rating => {
            products.sort_by_cached_key(|p| {
                let rating = get_product_average_rating(&p.coordinate).unwrap_or(0.0);
                std::cmp::Reverse(rating.to_bits())
            });
        }
        ProductSortBy::Title => {
            products.sort_by_cached_key(|p| p.title.to_lowercase());
        }
    }
}

/// Build filter for fetching products (limited)
pub fn products_filter(limit: usize) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_PRODUCT)).limit(limit)
}

/// Build filter for fetching products with pagination (cursor-based using `until`)
pub fn products_filter_paginated(limit: usize, until: Option<u64>) -> Filter {
    let mut filter = Filter::new().kind(Kind::Custom(KIND_PRODUCT)).limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    filter
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
    let d_prefix = format!("a:{}", product_coordinate);
    Filter::new()
        .kind(Kind::Custom(KIND_REVIEW))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::D), d_prefix)
        .limit(limit)
}
