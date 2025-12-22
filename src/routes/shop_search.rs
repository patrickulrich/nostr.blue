//! Shop Search - Search marketplace products

use dioxus::prelude::*;
use crate::routes::Route;
use crate::utils::nip99::Product;
use crate::stores::shop_store::{search_products, CART_ITEMS};
use crate::components::shop::{ProductCard, ProductCardSkeleton};

/// Product search page
#[component]
pub fn ShopSearch(q: String) -> Element {
    let mut query = use_signal(|| q.clone());
    let mut products = use_signal(Vec::<Product>::new);
    let mut loading = use_signal(|| false);
    let mut has_searched = use_signal(|| !q.is_empty());

    // Filter states
    let mut min_price = use_signal(|| None::<u64>);
    let mut max_price = use_signal(|| None::<u64>);
    let mut category_filter = use_signal(String::new);
    let mut digital_only = use_signal(|| false);
    let mut physical_only = use_signal(|| false);
    let mut show_filters = use_signal(|| false);

    // Search on initial load if query provided
    let initial_query = q.clone();
    use_effect(move || {
        if !initial_query.is_empty() {
            let q = initial_query.clone();
            spawn(async move {
                loading.set(true);
                match search_products(&q, 50).await {
                    Ok(p) => products.set(p),
                    Err(e) => log::error!("Search failed: {}", e),
                }
                loading.set(false);
            });
        }
    });

    // Apply local filters to products
    let filtered_products = {
        let prods = products.read();
        let min = *min_price.read();
        let max = *max_price.read();
        let cat = category_filter.read();
        let digital = *digital_only.read();
        let physical = *physical_only.read();

        prods.iter().filter(|p| {
            // Visibility filter - only show visible products (on-sale, pre-order)
            if !p.is_visible() { return false; }

            // Price filter (assuming sats)
            let price_sats = if p.price.currency.eq_ignore_ascii_case("sats") || p.price.currency.eq_ignore_ascii_case("sat") {
                p.price.amount as u64
            } else {
                0
            };

            if let Some(min_p) = min {
                if price_sats < min_p { return false; }
            }
            if let Some(max_p) = max {
                if price_sats > max_p { return false; }
            }

            // Category filter
            if !cat.is_empty() {
                let cat_lower = cat.to_lowercase();
                if !p.categories.iter().any(|c| c.to_lowercase().contains(&cat_lower)) {
                    return false;
                }
            }

            // Product type filter
            if digital && !p.format.is_digital() { return false; }
            if physical && p.format.is_digital() { return false; }

            true
        }).cloned().collect::<Vec<_>>()
    };

    let cart_count = CART_ITEMS.read().len();

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
                    h1 { class: "text-xl font-bold flex-1", "Search" }

                    // Filter toggle
                    button {
                        class: if *show_filters.read() {
                            "p-2 bg-blue-500 text-white rounded-full transition"
                        } else {
                            "p-2 hover:bg-accent rounded-full transition"
                        },
                        onclick: move |_| {
                            let current = *show_filters.read();
                            show_filters.set(!current);
                        },
                        crate::components::icons::FilterIcon { class: "w-5 h-5" }
                    }

                    // Cart button
                    Link {
                        to: Route::ShopCart {},
                        class: "relative p-2 hover:bg-accent rounded-full transition",
                        crate::components::icons::ShoppingCartIcon { class: "w-5 h-5" }
                        if cart_count > 0 {
                            span { class: "absolute -top-1 -right-1 bg-blue-500 text-white text-xs rounded-full w-5 h-5 flex items-center justify-center",
                                "{cart_count}"
                            }
                        }
                    }
                }

                // Search input
                div { class: "px-4 pb-4",
                    form {
                        class: "relative",
                        onsubmit: move |evt| {
                            evt.prevent_default();
                            let q = query.read().clone();
                            if !q.is_empty() {
                                has_searched.set(true);
                                let query_val = q;
                                spawn(async move {
                                    loading.set(true);
                                    match search_products(&query_val, 50).await {
                                        Ok(p) => products.set(p),
                                        Err(e) => log::error!("Search failed: {}", e),
                                    }
                                    loading.set(false);
                                });
                            }
                        },
                        input {
                            class: "w-full pl-10 pr-4 py-3 bg-muted rounded-lg outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "Search products...",
                            r#type: "text",
                            value: "{query}",
                            oninput: move |e| query.set(e.value())
                        }
                        div { class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                            crate::components::icons::SearchIcon { class: "w-5 h-5" }
                        }
                        button {
                            r#type: "submit",
                            class: "absolute right-2 top-1/2 -translate-y-1/2 px-4 py-1 bg-blue-500 hover:bg-blue-600 text-white rounded-full text-sm font-medium transition",
                            "Search"
                        }
                    }
                }

                // Filters panel
                if *show_filters.read() {
                    div { class: "px-4 pb-4 border-t border-border pt-4 space-y-4",
                        // Price range
                        div {
                            label { class: "block text-sm font-medium mb-2", "Price Range (sats)" }
                            div { class: "flex items-center gap-2",
                                input {
                                    r#type: "number",
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm",
                                    placeholder: "Min",
                                    value: if let Some(v) = *min_price.read() { v.to_string() } else { String::new() },
                                    oninput: move |e| {
                                        min_price.set(e.value().parse().ok());
                                    }
                                }
                                span { class: "text-muted-foreground", "-" }
                                input {
                                    r#type: "number",
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm",
                                    placeholder: "Max",
                                    value: if let Some(v) = *max_price.read() { v.to_string() } else { String::new() },
                                    oninput: move |e| {
                                        max_price.set(e.value().parse().ok());
                                    }
                                }
                            }
                        }

                        // Category filter
                        div {
                            label { class: "block text-sm font-medium mb-2", "Category" }
                            input {
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-muted rounded-lg text-sm",
                                placeholder: "e.g., electronics, art",
                                value: "{category_filter}",
                                oninput: move |e| category_filter.set(e.value())
                            }
                        }

                        // Product type
                        div {
                            label { class: "block text-sm font-medium mb-2", "Product Type" }
                            div { class: "flex gap-4",
                                label { class: "flex items-center gap-2 cursor-pointer",
                                    input {
                                        r#type: "checkbox",
                                        class: "rounded",
                                        checked: *digital_only.read(),
                                        onchange: move |e| {
                                            digital_only.set(e.checked());
                                            if e.checked() { physical_only.set(false); }
                                        }
                                    }
                                    span { class: "text-sm", "Digital only" }
                                }
                                label { class: "flex items-center gap-2 cursor-pointer",
                                    input {
                                        r#type: "checkbox",
                                        class: "rounded",
                                        checked: *physical_only.read(),
                                        onchange: move |e| {
                                            physical_only.set(e.checked());
                                            if e.checked() { digital_only.set(false); }
                                        }
                                    }
                                    span { class: "text-sm", "Physical only" }
                                }
                            }
                        }

                        // Clear filters button
                        if min_price.read().is_some() || max_price.read().is_some() || !category_filter.read().is_empty() || *digital_only.read() || *physical_only.read() {
                            button {
                                class: "w-full py-2 text-sm text-blue-500 hover:underline",
                                onclick: move |_| {
                                    min_price.set(None);
                                    max_price.set(None);
                                    category_filter.set(String::new());
                                    digital_only.set(false);
                                    physical_only.set(false);
                                },
                                "Clear all filters"
                            }
                        }
                    }
                }
            }

            // Results
            div { class: "p-4",
                if *loading.read() {
                    // Loading skeleton
                    div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                        for i in 0..8 {
                            ProductCardSkeleton { key: "{i}" }
                        }
                    }
                } else if !*has_searched.read() {
                    // Initial state
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "🔍" }
                        h2 { class: "text-xl font-semibold mb-2", "Search Products" }
                        p { class: "text-muted-foreground",
                            "Enter a search term to find products"
                        }
                    }
                } else if filtered_products.is_empty() {
                    // No results
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "📭" }
                        h2 { class: "text-xl font-semibold mb-2", "No Results" }
                        p { class: "text-muted-foreground mb-4",
                            "No products found for \"{query}\""
                        }
                        if min_price.read().is_some() || max_price.read().is_some() || !category_filter.read().is_empty() || *digital_only.read() || *physical_only.read() {
                            p { class: "text-sm text-muted-foreground",
                                "Try removing some filters"
                            }
                        }
                    }
                } else {
                    // Results header
                    div { class: "flex items-center justify-between mb-4",
                        p { class: "text-muted-foreground text-sm",
                            "{filtered_products.len()} products found"
                        }
                    }

                    // Results grid
                    div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                        for product in filtered_products.iter() {
                            ProductCard {
                                key: "{product.naddr}",
                                product: product.clone(),
                                show_add_to_cart: true
                            }
                        }
                    }
                }
            }
        }
    }
}
