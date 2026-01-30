//! Shop/Marketplace components (NIP-99)
//!
//! Components for displaying products, cart, checkout, and orders

/// Default maximum quantity for items without explicit stock limit
pub const DEFAULT_MAX_QUANTITY: u32 = 99;

mod cart_item;
mod cart_summary;
mod category_selector;
mod condition_badge;
mod image_carousel;
mod merchant_card;
mod order_card;
mod order_status_badge;
mod price_display;
mod product_card;
mod product_grid;
mod quantity_selector;
mod review_card;
mod review_form;

pub use cart_item::CartItemCard;
pub use cart_summary::CartSummary;
pub use category_selector::CategorySelector;
pub use condition_badge::ConditionBadge;
pub use image_carousel::ImageCarousel;
pub use merchant_card::MerchantCard;
pub use order_status_badge::OrderStatusBadge;
pub use price_display::PriceDisplay;
pub use product_card::{ProductCard, ProductCardSkeleton};
pub use quantity_selector::QuantitySelector;
pub use review_card::ReviewCard;
pub use review_form::ReviewForm;

// Components available for future use (not currently wired into routes)
#[allow(unused_imports)]
pub use order_card::OrderCard;
#[allow(unused_imports)]
pub use product_grid::ProductGrid;
