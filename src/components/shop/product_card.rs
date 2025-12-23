//! ProductCard component - displays a product in a grid

use dioxus::prelude::*;
use crate::utils::nip99::Product;
use crate::routes::Route;
use crate::stores::shop_store::add_to_cart;
use super::{PriceDisplay, ConditionBadge};

#[derive(Props, Clone, PartialEq)]
pub struct ProductCardProps {
    pub product: Product,
    #[props(default = true)]
    pub show_add_to_cart: bool,
}

/// Product card for grid display
#[component]
pub fn ProductCard(props: ProductCardProps) -> Element {
    let product = &props.product;

    // Get first image URL
    let image_url = product.images.first().map(|img| img.url.clone());

    // Convert price to sats for display with safe bounds checking
    let price_sats = if product.price.is_sats() {
        let amount = product.price.amount;
        // Ensure the f64 value is valid and within u64 range
        if amount.is_finite() && amount >= 0.0 && amount <= u64::MAX as f64 {
            amount.round() as u64
        } else {
            0
        }
    } else {
        // For non-sats currencies, we'd need conversion - for now show 0
        0
    };

    rsx! {
        Link {
            to: Route::ShopProductDetail { naddr: product.naddr.clone() },
            class: "bg-card border border-border rounded-lg overflow-hidden hover:border-ring transition group",

            // Product image
            div { class: "aspect-square bg-muted relative overflow-hidden",
                if let Some(img_url) = &image_url {
                    img {
                        src: "{img_url}",
                        class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                        loading: "lazy",
                        alt: "{product.title}"
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center text-4xl text-muted-foreground",
                        "📦"
                    }
                }

                // Stock indicator
                if product.stock == Some(0) {
                    div { class: "absolute inset-0 bg-black/50 flex items-center justify-center",
                        span { class: "bg-destructive text-destructive-foreground px-3 py-1 rounded-full text-sm font-medium",
                            "Out of Stock"
                        }
                    }
                } else if let Some(stock) = product.stock {
                    if stock > 0 && stock <= 5 {
                        // Low stock warning badge
                        div { class: "absolute top-2 right-2",
                            span { class: "bg-yellow-500 text-yellow-950 px-2 py-0.5 rounded text-xs font-medium",
                                "Only {stock} left"
                            }
                        }
                    }
                }
            }

            // Product info
            div { class: "p-3 space-y-2",
                // Title
                h3 { class: "font-medium text-sm line-clamp-2 group-hover:text-blue-500 transition-colors",
                    "{product.title}"
                }

                // Price
                div { class: "flex items-center gap-2",
                    if price_sats > 0 {
                        PriceDisplay { price_sats }
                    } else {
                        span { class: "text-sm font-medium", "{product.price.display()}" }
                    }
                }

                // Format badge, condition badge, subscription badge, and cart button row
                div { class: "flex items-center justify-between gap-2",
                    div { class: "flex items-center gap-1 flex-wrap",
                        span { class: "inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-muted text-muted-foreground",
                            if product.format.is_digital() { "Digital" } else { "Physical" }
                        }
                        if let Some(ref cond) = product.condition {
                            ConditionBadge { condition: cond.clone() }
                        }
                        if product.price.is_subscription() {
                            span { class: "inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-400",
                                "Subscription"
                            }
                        }
                    }

                    // Quick add to cart button
                    if props.show_add_to_cart && product.stock != Some(0) {
                        {
                            let product_clone = product.clone();
                            rsx! {
                                button {
                                    class: "p-1.5 bg-blue-500 hover:bg-blue-600 text-white rounded-full transition opacity-0 group-hover:opacity-100",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        e.prevent_default();
                                        add_to_cart(product_clone.clone(), 1);
                                    },
                                    title: "Add to cart",
                                    // Plus icon
                                    svg {
                                        class: "w-4 h-4",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M12 4.5v15m7.5-7.5h-15"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Skeleton loader for product card
#[component]
pub fn ProductCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden animate-pulse",
            // Image placeholder
            div { class: "aspect-square bg-muted" }

            // Info placeholder
            div { class: "p-3 space-y-2",
                // Title placeholder
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-4 bg-muted rounded w-1/2" }

                // Price placeholder
                div { class: "h-5 bg-muted rounded w-1/3 mt-2" }
            }
        }
    }
}
