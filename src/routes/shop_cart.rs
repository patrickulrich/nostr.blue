//! Shop Cart - Shopping cart view with multi-merchant support

use crate::components::shop::{CartItemCard, CartSummary};
use crate::routes::Route;
use crate::stores::profiles::{self, Profile};
use crate::stores::shop_store::{
    clear_cart, get_cart_count, remove_from_cart, update_cart_quantity, CART_ITEMS, CART_TOTAL_SATS,
};
use crate::utils::format::{format_sats_with_separator, truncate_id};
use crate::utils::nip99::CartItem;
use dioxus::prelude::*;
use std::collections::HashMap;

/// Shopping cart page
#[component]
pub fn ShopCart() -> Element {
    let cart_items = CART_ITEMS.read();
    let total_sats = *CART_TOTAL_SATS.read();
    let item_count = get_cart_count();

    // Group items by merchant pubkey
    let merchant_groups: HashMap<String, Vec<CartItem>> = {
        let mut groups: HashMap<String, Vec<CartItem>> = HashMap::new();
        for item in cart_items.iter() {
            groups
                .entry(item.product.pubkey.clone())
                .or_default()
                .push(item.clone());
        }
        groups
    };

    // State for merchant profiles
    let mut merchant_profiles = use_signal(HashMap::<String, Profile>::new);
    let mut profiles_fetched = use_signal(|| false);

    // Fetch merchant profiles (in parallel, only once)
    let pubkeys: Vec<String> = merchant_groups.keys().cloned().collect();
    use_effect(move || {
        // Skip if already fetched or no merchants
        if *profiles_fetched.peek() || pubkeys.is_empty() {
            return;
        }
        profiles_fetched.set(true);

        let pks = pubkeys.clone();
        spawn(async move {
            // Fetch all profiles in parallel
            let fetch_futures = pks.iter().map(|pk| {
                let pk = pk.clone();
                async move {
                    profiles::fetch_profile(pk.clone())
                        .await
                        .ok()
                        .map(|p| (pk, p))
                }
            });

            let results = futures::future::join_all(fetch_futures).await;

            for result in results.into_iter().flatten() {
                merchant_profiles.write().insert(result.0, result.1);
            }
        });
    });

    // Check if multiple merchants (for display purposes)
    let has_multiple_merchants = merchant_groups.len() > 1;

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
                        // Multi-merchant notice
                        if has_multiple_merchants {
                            div { class: "bg-blue-500/10 border border-blue-500/30 rounded-lg p-4 text-sm",
                                div { class: "flex items-center gap-2",
                                    span { class: "text-blue-500", "ℹ️" }
                                    span {
                                        "Your cart contains items from "
                                        strong { "{merchant_groups.len()} merchants" }
                                        ". Separate orders will be placed with each."
                                    }
                                }
                            }
                        }

                        // Cart items grouped by merchant
                        for (merchant_pk, items) in merchant_groups.iter() {
                            MerchantCartGroup {
                                key: "{merchant_pk}",
                                merchant_pubkey: merchant_pk.clone(),
                                items: items.clone(),
                                profile: merchant_profiles.read().get(merchant_pk).cloned(),
                                show_header: has_multiple_merchants
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

/// A group of cart items from a single merchant
#[component]
fn MerchantCartGroup(
    merchant_pubkey: String,
    items: Vec<CartItem>,
    profile: Option<Profile>,
    show_header: bool,
) -> Element {
    // Calculate subtotal for this merchant
    let subtotal: u64 = items
        .iter()
        .map(|item| {
            let price = if item.product.price.currency.eq_ignore_ascii_case("sats") {
                item.product.price.amount as u64
            } else {
                0
            };
            price * item.quantity as u64
        })
        .sum();

    // Check if merchant has Lightning (lud16)
    let has_lightning = profile.as_ref().and_then(|p| p.lud16.as_ref()).is_some();

    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden",
            // Merchant header (only show if multiple merchants)
            if show_header {
                div { class: "bg-muted/50 p-4 border-b border-border",
                    div { class: "flex items-center gap-3",
                        // Avatar
                        div { class: "w-10 h-10 rounded-full bg-muted overflow-hidden shrink-0",
                            if let Some(ref p) = profile {
                                if let Some(ref picture) = p.picture {
                                    img {
                                        src: "{picture}",
                                        class: "w-full h-full object-cover"
                                    }
                                } else {
                                    div { class: "w-full h-full flex items-center justify-center text-xl",
                                        "🏪"
                                    }
                                }
                            } else {
                                div { class: "w-full h-full flex items-center justify-center text-xl",
                                    "🏪"
                                }
                            }
                        }

                        // Name
                        div { class: "flex-1 min-w-0",
                            p { class: "font-medium truncate",
                                if let Some(ref p) = profile {
                                    if let Some(ref name) = p.name {
                                        "{name}"
                                    } else {
                                        "{truncate_id(&merchant_pubkey, 8)}..."
                                    }
                                } else {
                                    "{truncate_id(&merchant_pubkey, 8)}..."
                                }
                            }
                            p { class: "text-xs text-muted-foreground",
                                {
                                    let count = items.len();
                                    if count > 1 { format!("{} items", count) } else { format!("{} item", count) }
                                }
                            }
                        }

                        // Subtotal
                        div { class: "text-right",
                            p { class: "font-medium text-amber-500",
                                "⚡{format_sats_with_separator(subtotal)}"
                            }
                            // Lightning warning
                            if !has_lightning {
                                p { class: "text-xs text-amber-600 flex items-center gap-1",
                                    "⚠️ No Lightning"
                                }
                            }
                        }
                    }
                }
            }

            // Items
            div { class: "divide-y divide-border",
                for item in items.iter() {
                    div { class: "p-4",
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
            }
        }
    }
}
