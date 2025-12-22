//! CartItemCard component - displays an item in the shopping cart

use dioxus::prelude::*;
use crate::utils::nip99::CartItem;
use super::{PriceDisplay, QuantitySelector};

#[derive(Props, Clone, PartialEq)]
pub struct CartItemCardProps {
    pub item: CartItem,
    pub on_quantity_change: EventHandler<(String, u32)>,
    pub on_remove: EventHandler<String>,
}

/// Cart item card with quantity controls
#[component]
pub fn CartItemCard(props: CartItemCardProps) -> Element {
    let item = &props.item;
    let product = &item.product;

    // Get first image URL
    let image_url = product.images.first().map(|img| img.url.clone());

    // Convert price to sats for display
    let price_sats = if product.price.currency.eq_ignore_ascii_case("sats") || product.price.currency.eq_ignore_ascii_case("sat") {
        product.price.amount as u64
    } else {
        0
    };

    let line_total = price_sats * item.quantity as u64;

    rsx! {
        div { class: "flex gap-4 p-4 bg-card border border-border rounded-lg",
            // Product image
            div { class: "w-20 h-20 flex-shrink-0 bg-muted rounded overflow-hidden",
                if let Some(img_url) = &image_url {
                    img {
                        src: "{img_url}",
                        class: "w-full h-full object-cover",
                        alt: "{product.title}"
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center text-2xl",
                        "📦"
                    }
                }
            }

            // Item details
            div { class: "flex-1 min-w-0",
                h3 { class: "font-medium line-clamp-2 mb-1",
                    "{product.title}"
                }

                // Unit price
                div { class: "text-sm text-muted-foreground mb-2",
                    if price_sats > 0 {
                        PriceDisplay { price_sats }
                    } else {
                        span { "{product.price.display()}" }
                    }
                    " each"
                }

                // Quantity selector
                div { class: "flex items-center justify-between",
                    QuantitySelector {
                        quantity: item.quantity,
                        max: product.stock.unwrap_or(99),
                        on_change: {
                            let naddr = product.naddr.clone();
                            let on_change = props.on_quantity_change;
                            move |qty| on_change.call((naddr.clone(), qty))
                        }
                    }

                    // Remove button
                    button {
                        class: "text-sm text-destructive hover:underline",
                        onclick: {
                            let naddr = product.naddr.clone();
                            let on_remove = props.on_remove;
                            move |_| on_remove.call(naddr.clone())
                        },
                        "Remove"
                    }
                }
            }

            // Line total
            div { class: "flex-shrink-0 text-right",
                if line_total > 0 {
                    PriceDisplay { price_sats: line_total }
                } else {
                    span { class: "font-medium", "{item.line_total():.2} {product.price.currency}" }
                }
            }
        }
    }
}
