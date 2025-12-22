//! Shop Merchant - Seller dashboard

use dioxus::prelude::*;
use crate::routes::Route;
use crate::utils::nip99::Product;
use crate::stores::shop_store::{fetch_my_products, delete_product};
use crate::components::shop::ProductCardSkeleton;

/// Merchant dashboard page
#[component]
pub fn ShopMerchant() -> Element {
    let mut products = use_signal(Vec::<Product>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut deleting = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| None::<Product>);

    // Fetch my products on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error.set(None);
            match fetch_my_products().await {
                Ok(p) => products.set(p),
                Err(e) => {
                    log::error!("Failed to fetch my products: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    let product_count = products.read().len();

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
                    h1 { class: "text-xl font-bold flex-1", "My Shop" }
                    Link {
                        to: Route::ShopProductNew {},
                        class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-full text-sm font-medium transition",
                        "+ New Product"
                    }
                }

                // Tab navigation
                div { class: "flex border-b border-border",
                    Link {
                        to: Route::ShopMerchant {},
                        class: "flex-1 py-3 text-center font-medium border-b-2 border-blue-500 text-blue-500",
                        "Products ({product_count})"
                    }
                    Link {
                        to: Route::ShopMerchantOrders {},
                        class: "flex-1 py-3 text-center text-muted-foreground hover:text-foreground transition",
                        "Orders"
                    }
                }
            }

            // Content
            div { class: "p-4",
                if *loading.read() {
                    // Loading skeleton
                    div { class: "grid grid-cols-2 md:grid-cols-3 gap-4",
                        for i in 0..6 {
                            ProductCardSkeleton { key: "{i}" }
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "😢" }
                        h2 { class: "text-xl font-semibold mb-2", "Failed to load products" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                            onclick: move |_| {
                                spawn(async move {
                                    loading.set(true);
                                    error.set(None);
                                    match fetch_my_products().await {
                                        Ok(p) => products.set(p),
                                        Err(e) => error.set(Some(e)),
                                    }
                                    loading.set(false);
                                });
                            },
                            "Try Again"
                        }
                    }
                } else if products.read().is_empty() {
                    // Empty state
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "🏪" }
                        h2 { class: "text-xl font-semibold mb-2", "Start Selling" }
                        p { class: "text-muted-foreground mb-4",
                            "Create your first product listing"
                        }
                        Link {
                            to: Route::ShopProductNew {},
                            class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-full transition font-medium",
                            "Create Product"
                        }
                    }
                } else {
                    // Products grid with edit/delete options
                    div { class: "space-y-4",
                        for product in products.read().iter() {
                            div {
                                key: "{product.naddr}",
                                class: "bg-card border border-border rounded-lg overflow-hidden",

                                // Product row
                                div { class: "flex items-start gap-4 p-4",
                                    // Product image
                                    div { class: "w-20 h-20 bg-muted rounded-lg overflow-hidden flex-shrink-0",
                                        if let Some(img) = product.images.first() {
                                            img {
                                                src: "{img.url}",
                                                class: "w-full h-full object-cover",
                                                loading: "lazy"
                                            }
                                        } else {
                                            div { class: "w-full h-full flex items-center justify-center text-2xl",
                                                "📦"
                                            }
                                        }
                                    }

                                    // Product info
                                    div { class: "flex-1 min-w-0",
                                        Link {
                                            to: Route::ShopProductDetail { naddr: product.naddr.clone() },
                                            h3 { class: "font-medium truncate hover:text-blue-500 transition",
                                                "{product.title}"
                                            }
                                        }
                                        {
                                            let price_sats = if product.price.currency.eq_ignore_ascii_case("sats") || product.price.currency.eq_ignore_ascii_case("sat") {
                                                product.price.amount as u64
                                            } else {
                                                0
                                            };
                                            if price_sats > 0 {
                                                rsx! {
                                                    p { class: "text-amber-500 font-medium",
                                                        "⚡{price_sats} sats"
                                                    }
                                                }
                                            } else {
                                                rsx! {
                                                    p { class: "font-medium",
                                                        "{product.price.display()}"
                                                    }
                                                }
                                            }
                                        }
                                        div { class: "flex items-center gap-2 mt-1",
                                            span { class: "text-xs px-2 py-0.5 bg-muted rounded",
                                                if product.format.is_digital() { "Digital" } else { "Physical" }
                                            }
                                            if let Some(stock) = product.stock {
                                                span { class: if stock == 0 {
                                                    "text-xs px-2 py-0.5 bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400 rounded"
                                                } else if stock < 5 {
                                                    "text-xs px-2 py-0.5 bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400 rounded"
                                                } else {
                                                    "text-xs px-2 py-0.5 bg-muted rounded"
                                                },
                                                    "Stock: {stock}"
                                                }
                                            }
                                        }
                                    }

                                    // Action buttons
                                    div { class: "flex items-center gap-2 flex-shrink-0",
                                        // Edit button
                                        Link {
                                            to: Route::ShopProductEdit { naddr: product.naddr.clone() },
                                            class: "p-2 hover:bg-accent rounded-full transition",
                                            title: "Edit",
                                            crate::components::icons::EditIcon { class: "w-5 h-5" }
                                        }

                                        // Delete button
                                        {
                                            let product_clone = product.clone();
                                            let is_deleting = deleting.read().as_ref().map(|d| d == &product.naddr).unwrap_or(false);
                                            rsx! {
                                                button {
                                                    class: "p-2 hover:bg-destructive/10 text-destructive rounded-full transition disabled:opacity-50",
                                                    title: "Delete",
                                                    disabled: is_deleting,
                                                    onclick: move |e| {
                                                        e.stop_propagation();
                                                        confirm_delete.set(Some(product_clone.clone()));
                                                    },
                                                    if is_deleting {
                                                        span { class: "w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                                    } else {
                                                        crate::components::icons::TrashIcon { class: "w-5 h-5" }
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

            // Delete confirmation modal
            if let Some(product_to_delete) = confirm_delete.read().as_ref() {
                div { class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
                    onclick: move |_| confirm_delete.set(None),

                    div {
                        class: "bg-background rounded-lg max-w-sm w-full p-6",
                        onclick: move |e| e.stop_propagation(),

                        div { class: "text-center",
                            div { class: "w-16 h-16 mx-auto mb-4 bg-destructive/10 rounded-full flex items-center justify-center",
                                crate::components::icons::TrashIcon { class: "w-8 h-8 text-destructive" }
                            }
                            h2 { class: "text-xl font-semibold mb-2", "Delete Product?" }
                            p { class: "text-muted-foreground mb-6",
                                "Are you sure you want to delete \"{product_to_delete.title}\"? This action cannot be undone."
                            }

                            div { class: "flex gap-3",
                                button {
                                    class: "flex-1 py-2 border border-border rounded-lg hover:bg-accent transition",
                                    onclick: move |_| confirm_delete.set(None),
                                    "Cancel"
                                }
                                {
                                    let naddr = product_to_delete.naddr.clone();
                                    let d_tag = product_to_delete.d_tag.clone();
                                    rsx! {
                                        button {
                                            class: "flex-1 py-2 bg-destructive hover:bg-destructive/90 text-destructive-foreground rounded-lg transition disabled:opacity-50",
                                            disabled: deleting.read().is_some(),
                                            onclick: move |_| {
                                                let naddr_clone = naddr.clone();
                                                let d_tag_clone = d_tag.clone();
                                                deleting.set(Some(naddr_clone.clone()));
                                                spawn(async move {
                                                    match delete_product(&naddr_clone, &d_tag_clone).await {
                                                        Ok(_) => {
                                                            // Refresh products
                                                            if let Ok(p) = fetch_my_products().await {
                                                                products.set(p);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            log::error!("Failed to delete product: {}", e);
                                                        }
                                                    }
                                                    deleting.set(None);
                                                    confirm_delete.set(None);
                                                });
                                            },
                                            "Delete"
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
