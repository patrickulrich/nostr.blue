//! Shop Home - Browse marketplace products (NIP-99)

use dioxus::prelude::*;
use crate::routes::Route;
use crate::utils::nip99::Product;
use crate::stores::shop_store::{fetch_products, get_cart_count};
use crate::components::shop::{ProductCard, ProductCardSkeleton, CategorySelector};

/// Sort options for products
#[derive(Clone, Copy, PartialEq)]
pub enum ProductSortBy {
    Newest,
    PriceLow,
    PriceHigh,
}

impl ProductSortBy {
    fn label(&self) -> &'static str {
        match self {
            Self::Newest => "Newest",
            Self::PriceLow => "Price: Low to High",
            Self::PriceHigh => "Price: High to Low",
        }
    }
}

/// Shop browse page - displays product grid with filters
#[component]
pub fn ShopHome() -> Element {
    let mut products = use_signal(Vec::<Product>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Filter states
    let mut show_filters = use_signal(|| false);
    let mut min_price = use_signal(|| None::<u64>);
    let mut max_price = use_signal(|| None::<u64>);
    let mut category_filter = use_signal(Vec::<String>::new);
    let mut digital_only = use_signal(|| false);
    let mut physical_only = use_signal(|| false);
    let mut sort_by = use_signal(|| ProductSortBy::Newest);

    // Fetch products on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error.set(None);
            match fetch_products(50).await {
                Ok(p) => products.set(p),
                Err(e) => {
                    log::error!("Failed to fetch products: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    // Apply local filters to products
    let filtered_products = {
        let mut prods = products.read().clone();
        let min = *min_price.read();
        let max = *max_price.read();
        let cats = category_filter.read();
        let digital = *digital_only.read();
        let physical = *physical_only.read();
        let sort = *sort_by.read();

        prods.retain(|p| {
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

            // Category filter - match any selected category
            if !cats.is_empty() {
                let has_match = cats.iter().any(|selected_cat| {
                    let cat_lower = selected_cat.to_lowercase();
                    p.categories.iter().any(|c| c.to_lowercase().contains(&cat_lower))
                });
                if !has_match { return false; }
            }

            // Product type filter
            if digital && !p.format.is_digital() { return false; }
            if physical && p.format.is_digital() { return false; }

            true
        });

        // Sort
        match sort {
            ProductSortBy::Newest => {
                prods.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
            ProductSortBy::PriceLow => {
                prods.sort_by(|a, b| {
                    let price_a = if a.price.currency.eq_ignore_ascii_case("sats") { a.price.amount as u64 } else { 0 };
                    let price_b = if b.price.currency.eq_ignore_ascii_case("sats") { b.price.amount as u64 } else { 0 };
                    price_a.cmp(&price_b)
                });
            }
            ProductSortBy::PriceHigh => {
                prods.sort_by(|a, b| {
                    let price_a = if a.price.currency.eq_ignore_ascii_case("sats") { a.price.amount as u64 } else { 0 };
                    let price_b = if b.price.currency.eq_ignore_ascii_case("sats") { b.price.amount as u64 } else { 0 };
                    price_b.cmp(&price_a)
                });
            }
        }

        prods
    };

    let has_filters = min_price.read().is_some() || max_price.read().is_some()
        || !category_filter.read().is_empty() || *digital_only.read() || *physical_only.read();
    let cart_count = get_cart_count();

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "flex items-center gap-4 p-4",
                    h1 { class: "text-xl font-bold flex-1", "Marketplace" }

                    // Filter toggle
                    button {
                        class: if *show_filters.read() || has_filters {
                            "relative p-2 bg-blue-500 text-white rounded-full transition"
                        } else {
                            "p-2 hover:bg-accent rounded-full transition"
                        },
                        onclick: move |_| {
                            let current = *show_filters.read();
                            show_filters.set(!current);
                        },
                        crate::components::icons::FilterIcon { class: "w-5 h-5" }
                        if has_filters {
                            span { class: "absolute -top-1 -right-1 w-3 h-3 bg-amber-500 rounded-full" }
                        }
                    }

                    // Cart button with badge
                    Link {
                        to: Route::ShopCart {},
                        class: "relative p-2 hover:bg-accent rounded-full transition",
                        crate::components::icons::ShoppingCartIcon { class: "w-6 h-6" }
                        if cart_count > 0 {
                            span { class: "absolute -top-1 -right-1 bg-blue-500 text-white text-xs rounded-full w-5 h-5 flex items-center justify-center",
                                "{cart_count}"
                            }
                        }
                    }
                }

                // Tab navigation
                div { class: "flex border-b border-border",
                    Link {
                        to: Route::ShopHome {},
                        class: "flex-1 py-3 text-center font-medium border-b-2 border-blue-500 text-blue-500",
                        "Browse"
                    }
                    Link {
                        to: Route::ShopSearch { q: String::new() },
                        class: "flex-1 py-3 text-center text-muted-foreground hover:text-foreground transition",
                        "Search"
                    }
                    Link {
                        to: Route::ShopMerchant {},
                        class: "flex-1 py-3 text-center text-muted-foreground hover:text-foreground transition",
                        "My Shop"
                    }
                }

                // Filters panel
                if *show_filters.read() {
                    div { class: "px-4 py-4 border-t border-border space-y-4 bg-background",
                        // Sort
                        div {
                            label { class: "block text-sm font-medium mb-2", "Sort By" }
                            div { class: "flex gap-2 flex-wrap",
                                for option in [ProductSortBy::Newest, ProductSortBy::PriceLow, ProductSortBy::PriceHigh] {
                                    button {
                                        key: "{option.label()}",
                                        class: if *sort_by.read() == option {
                                            "px-3 py-1 text-sm bg-blue-500 text-white rounded-full"
                                        } else {
                                            "px-3 py-1 text-sm bg-muted hover:bg-accent rounded-full transition"
                                        },
                                        onclick: move |_| sort_by.set(option),
                                        "{option.label()}"
                                    }
                                }
                            }
                        }

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

                        // Category filter using CategorySelector
                        div {
                            label { class: "block text-sm font-medium mb-2", "Categories" }
                            CategorySelector {
                                selected: category_filter.read().clone(),
                                on_change: move |cats: Vec<String>| category_filter.set(cats),
                                multi_select: true
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
                        if has_filters {
                            button {
                                class: "w-full py-2 text-sm text-blue-500 hover:underline",
                                onclick: move |_| {
                                    min_price.set(None);
                                    max_price.set(None);
                                    category_filter.set(Vec::new());
                                    digital_only.set(false);
                                    physical_only.set(false);
                                },
                                "Clear all filters"
                            }
                        }
                    }
                }
            }

            // Content
            div { class: "p-4",
                if *loading.read() {
                    // Loading skeleton grid
                    div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                        for i in 0..8 {
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
                                // Trigger refetch
                                spawn(async move {
                                    loading.set(true);
                                    error.set(None);
                                    match fetch_products(50).await {
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
                    // Empty state (no products at all)
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "🛒" }
                        h2 { class: "text-xl font-semibold mb-2", "No Products Found" }
                        p { class: "text-muted-foreground mb-4",
                            "Be the first to list a product on the marketplace!"
                        }
                        Link {
                            to: Route::ShopProductNew {},
                            class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-full transition font-medium",
                            "List a Product"
                        }
                    }
                } else if filtered_products.is_empty() {
                    // Empty state (filtered)
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "🔍" }
                        h2 { class: "text-xl font-semibold mb-2", "No Matching Products" }
                        p { class: "text-muted-foreground mb-4",
                            "Try adjusting your filters"
                        }
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                            onclick: move |_| {
                                min_price.set(None);
                                max_price.set(None);
                                category_filter.set(Vec::new());
                                digital_only.set(false);
                                physical_only.set(false);
                            },
                            "Clear Filters"
                        }
                    }
                } else {
                    // Results count
                    div { class: "flex items-center justify-between mb-4",
                        p { class: "text-sm text-muted-foreground",
                            "{filtered_products.len()} products"
                        }
                    }

                    // Product grid
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
