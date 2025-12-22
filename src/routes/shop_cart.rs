//! Shop Cart - Shopping cart view

use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::shop_store::{
    CART_ITEMS, CART_TOTAL_SATS,
    remove_from_cart, update_cart_quantity, clear_cart, get_cart_count,
};
use crate::components::shop::{CartItemCard, CartSummary};

/// Shopping cart page
#[component]
pub fn ShopCart() -> Element {
    let cart_items = CART_ITEMS.read();
    let total_sats = *CART_TOTAL_SATS.read();
    let item_count = get_cart_count();

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "flex items-center gap-4 p-4",
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        onclick: move |_| {
                            let nav = navigator();
                            nav.go_back();
                        },
                        crate::components::icons::ArrowLeftIcon { class: "w-5 h-5" }
                    }
                    h1 { class: "text-xl font-bold flex-1", "Shopping Cart" }

                    // Clear cart button (only if items exist)
                    if !cart_items.is_empty() {
                        button {
                            class: "text-sm text-destructive hover:underline",
                            onclick: move |_| clear_cart(),
                            "Clear All"
                        }
                    }
                }
            }

            // Cart content
            div { class: "max-w-3xl mx-auto p-4",
                if cart_items.is_empty() {
                    // Empty cart state
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "🛒" }
                        h2 { class: "text-xl font-semibold mb-2", "Your Cart is Empty" }
                        p { class: "text-muted-foreground mb-6",
                            "Add products to your cart to see them here"
                        }
                        Link {
                            to: Route::ShopHome {},
                            class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-full transition font-medium",
                            "Browse Products"
                        }
                    }
                } else {
                    div { class: "space-y-6",
                        // Cart items
                        div { class: "space-y-4",
                            for item in cart_items.iter() {
                                CartItemCard {
                                    key: "{item.product.naddr}",
                                    item: item.clone(),
                                    on_quantity_change: move |(naddr, qty): (String, u32)| {
                                        update_cart_quantity(&naddr, qty);
                                    },
                                    on_remove: move |naddr: String| {
                                        remove_from_cart(&naddr);
                                    }
                                }
                            }
                        }

                        // Summary
                        CartSummary {
                            total_sats,
                            item_count
                        }

                        // Checkout button
                        Link {
                            to: Route::ShopCheckout {},
                            class: "block w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg text-center font-medium transition",
                            "Proceed to Checkout"
                        }

                        // Continue shopping link
                        div { class: "text-center",
                            Link {
                                to: Route::ShopHome {},
                                class: "text-sm text-muted-foreground hover:text-foreground transition",
                                "← Continue Shopping"
                            }
                        }
                    }
                }
            }
        }
    }
}
