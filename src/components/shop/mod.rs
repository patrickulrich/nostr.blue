//! Shop/Marketplace components (NIP-99)
//!
//! Components for displaying products, cart, checkout, and orders

/// Default maximum quantity for items without explicit stock limit
pub const DEFAULT_MAX_QUANTITY: u32 = 99;

mod product_card;
mod product_grid;
mod price_display;
mod quantity_selector;
mod cart_item;
mod cart_summary;
mod order_card;
mod order_status_badge;
mod review_card;
mod review_form;
mod merchant_card;
mod image_carousel;
mod condition_badge;
mod category_selector;

pub use product_card::{ProductCard, ProductCardSkeleton};
pub use price_display::PriceDisplay;
pub use quantity_selector::QuantitySelector;
pub use cart_item::CartItemCard;
pub use cart_summary::CartSummary;
pub use order_status_badge::OrderStatusBadge;
pub use review_card::ReviewCard;
pub use review_form::ReviewForm;
pub use merchant_card::MerchantCard;
pub use image_carousel::ImageCarousel;
pub use condition_badge::ConditionBadge;
pub use category_selector::CategorySelector;

// Components available for future use (not currently wired into routes)
#[allow(unused_imports)]
pub use product_grid::ProductGrid;
#[allow(unused_imports)]
pub use order_card::OrderCard;
