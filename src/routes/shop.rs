//! Shop Home - Browse marketplace products (NIP-99)

use dioxus::prelude::*;
use crate::routes::Route;
use crate::utils::nip99::{Product, ProductFormat};
use crate::stores::shop_store::{
    fetch_products, get_cart_count,
    ShopFilterState, filter_products, sort_products, ProductSortBy,
};
use crate::stores::nostr_client::{fetch_contacts, get_user_pubkey};
use crate::components::shop::{ProductCard, ProductCardSkeleton, CategorySelector};

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

    // Web of Trust filter - only show products from followed users
    let mut wot_enabled = use_signal(|| false);
    let mut wot_contacts = use_signal(Vec::<String>::new);
    let mut wot_loading = use_signal(|| false);

    // Fetch guard to prevent redundant fetches on re-renders
    let mut has_fetched = use_signal(|| false);

    // Fetch products on mount (only once)
    use_effect(move || {
        // Skip if already fetched to prevent redundant requests
        if *has_fetched.peek() {
            return;
        }
        has_fetched.set(true);

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

    // Build filter state from UI inputs
    let filter_state = {
        let cats = category_filter.read();
        let digital = *digital_only.read();
        let physical = *physical_only.read();

        ShopFilterState {
            min_price_sats: *min_price.read(),
            max_price_sats: *max_price.read(),
            category: if cats.is_empty() { None } else { cats.first().cloned() },
            format: if digital {
                Some(ProductFormat::Digital)
            } else if physical {
                Some(ProductFormat::Physical)
            } else {
                None
            },
            ..Default::default()
        }
    };

    // Apply filters and sort using infrastructure
    let filtered_products = {
        let prods = products.read();
        let sort = *sort_by.read();

        let mut filtered = filter_products(&prods, &filter_state);

        // Apply Web of Trust filter if enabled
        if *wot_enabled.read() {
            let contacts = wot_contacts.read();
            if !contacts.is_empty() {
                filtered.retain(|p| contacts.contains(&p.pubkey));
            }
        }

        sort_products(&mut filtered, sort);
        filtered
    };

    let has_filters = !filter_state.is_empty() || *wot_enabled.read();
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
                                for option in [
                                    ProductSortBy::Newest,
                                    ProductSortBy::Oldest,
                                    ProductSortBy::PriceLow,
                                    ProductSortBy::PriceHigh,
                                    ProductSortBy::Rating,
                                    ProductSortBy::Title,
                                ] {
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

                        // Web of Trust filter
                        div {
                            label { class: "flex items-center justify-between cursor-pointer p-2 rounded-lg hover:bg-accent transition",
                                div { class: "flex items-center gap-2",
                                    // People icon
                                    svg {
                                        class: "w-5 h-5 text-blue-500",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M15 19.128a9.38 9.38 0 002.625.372 9.337 9.337 0 004.121-.952 4.125 4.125 0 00-7.533-2.493M15 19.128v-.003c0-1.113-.285-2.16-.786-3.07M15 19.128v.106A12.318 12.318 0 018.624 21c-2.331 0-4.512-.645-6.374-1.766l-.001-.109a6.375 6.375 0 0111.964-3.07M12 6.375a3.375 3.375 0 11-6.75 0 3.375 3.375 0 016.75 0zm8.25 2.25a2.625 2.625 0 11-5.25 0 2.625 2.625 0 015.25 0z"
                                        }
                                    }
                                    div {
                                        span { class: "text-sm font-medium", "Web of Trust" }
                                        p { class: "text-xs text-muted-foreground", "Only show products from people you follow" }
                                    }
                                }
                                // Toggle switch
                                div { class: "relative",
                                    input {
                                        r#type: "checkbox",
                                        class: "sr-only peer",
                                        checked: *wot_enabled.read(),
                                        onchange: move |e| {
                                            let enabled = e.checked();
                                            wot_enabled.set(enabled);

                                            if enabled {
                                                // Fetch contacts when enabling WoT filter
                                                wot_loading.set(true);
                                                spawn(async move {
                                                    if let Ok(pubkey) = get_user_pubkey().await {
                                                        match fetch_contacts(pubkey.to_hex()).await {
                                                            Ok(contacts) => {
                                                                log::info!("WoT filter: loaded {} contacts", contacts.len());
                                                                wot_contacts.set(contacts);
                                                            }
                                                            Err(e) => {
                                                                log::error!("Failed to fetch contacts for WoT: {}", e);
                                                            }
                                                        }
                                                    }
                                                    wot_loading.set(false);
                                                });
                                            }
                                        }
                                    }
                                    div { class: "w-11 h-6 bg-muted rounded-full peer peer-checked:bg-blue-500 transition-colors" }
                                    div { class: "absolute left-1 top-1 w-4 h-4 bg-white rounded-full transition-transform peer-checked:translate-x-5" }
                                }
                            }
                            if *wot_loading.read() {
                                p { class: "text-xs text-muted-foreground mt-1 ml-7", "Loading your follow list..." }
                            } else if *wot_enabled.read() && wot_contacts.read().is_empty() {
                                p { class: "text-xs text-yellow-600 dark:text-yellow-500 mt-1 ml-7", "You're not following anyone yet" }
                            } else if *wot_enabled.read() {
                                p { class: "text-xs text-muted-foreground mt-1 ml-7",
                                    "Filtering by {wot_contacts.read().len()} followed users"
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
                                    wot_enabled.set(false);
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
                                wot_enabled.set(false);
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
