//! Shop Merchant - Seller dashboard

use crate::components::shop::ProductCardSkeleton;
use crate::routes::Route;
use crate::stores::shop_store::{
    delete_collection, delete_product, fetch_my_collections, fetch_my_products,
    get_active_order_count, get_seller_active_order_count, get_shop_stats,
};
use crate::utils::nip99::{Product, ProductCollection};
use dioxus::prelude::*;

/// Merchant dashboard tabs
#[derive(Clone, Copy, PartialEq)]
enum MerchantTab {
    Products,
    Collections,
    Stats,
}

/// Merchant dashboard page
#[component]
pub fn ShopMerchant() -> Element {
    let mut active_tab = use_signal(|| MerchantTab::Products);
    let mut products = use_signal(Vec::<Product>::new);
    let mut collections = use_signal(Vec::<ProductCollection>::new);
    let mut loading = use_signal(|| true);
    let mut loading_collections = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut delete_error = use_signal(|| None::<String>);
    let mut deleting = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| None::<Product>);
    let mut confirm_delete_collection = use_signal(|| None::<ProductCollection>);

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

            // Fetch collections
            loading_collections.set(true);
            match fetch_my_collections().await {
                Ok(c) => collections.set(c),
                Err(e) => log::error!("Failed to fetch collections: {}", e),
            }
            loading_collections.set(false);
        });
    });

    let product_count = products.read().len();
    let collection_count = collections.read().len();

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
                    button {
                        class: if *active_tab.read() == MerchantTab::Products {
                            "flex-1 py-3 text-center font-medium border-b-2 border-blue-500 text-blue-500"
                        } else {
                            "flex-1 py-3 text-center text-muted-foreground hover:text-foreground transition"
                        },
                        onclick: move |_| active_tab.set(MerchantTab::Products),
                        "Products ({product_count})"
                    }
                    button {
                        class: if *active_tab.read() == MerchantTab::Collections {
                            "flex-1 py-3 text-center font-medium border-b-2 border-blue-500 text-blue-500"
                        } else {
                            "flex-1 py-3 text-center text-muted-foreground hover:text-foreground transition"
                        },
                        onclick: move |_| active_tab.set(MerchantTab::Collections),
                        "Collections ({collection_count})"
                    }
                    button {
                        class: if *active_tab.read() == MerchantTab::Stats {
                            "flex-1 py-3 text-center font-medium border-b-2 border-blue-500 text-blue-500"
                        } else {
                            "flex-1 py-3 text-center text-muted-foreground hover:text-foreground transition"
                        },
                        onclick: move |_| active_tab.set(MerchantTab::Stats),
                        "Stats"
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
                // Products Tab
                if *active_tab.read() == MerchantTab::Products {
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
                        // Delete error message
                        if let Some(err) = delete_error.read().as_ref() {
                            div { class: "bg-destructive/10 border border-destructive/50 text-destructive rounded-lg p-4 mb-4 flex items-center justify-between",
                                span { "{err}" }
                                button {
                                    class: "text-destructive hover:underline text-sm",
                                    onclick: move |_| delete_error.set(None),
                                    "Dismiss"
                                }
                            }
                        }

                        // Products grid with edit/delete options
                        div { class: "space-y-4",
                            for product in products.read().iter() {
                                div {
                                    key: "{product.naddr}",
                                    class: "bg-card border border-border rounded-lg overflow-hidden",

                                    // Product row
                                    div { class: "flex items-start gap-4 p-4",
                                        // Product image
                                        div { class: "w-20 h-20 bg-muted rounded-lg overflow-hidden shrink-0",
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
                                        div { class: "flex items-center gap-2 shrink-0",
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
                } // end Products Tab

                // Collections Tab
                if *active_tab.read() == MerchantTab::Collections {
                    if *loading_collections.read() {
                        div { class: "grid grid-cols-2 md:grid-cols-3 gap-4",
                            for i in 0..4 {
                                ProductCardSkeleton { key: "{i}" }
                            }
                        }
                    } else if collections.read().is_empty() {
                        div { class: "text-center py-12",
                            div { class: "text-6xl mb-4", "📁" }
                            h2 { class: "text-xl font-semibold mb-2", "No Collections" }
                            p { class: "text-muted-foreground mb-4",
                                "Group your products into collections for easier discovery"
                            }
                            Link {
                                to: Route::ShopCollectionNew {},
                                class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-full transition font-medium",
                                "Create Collection"
                            }
                        }
                    } else {
                        // Delete error message
                        if let Some(err) = delete_error.read().as_ref() {
                            div { class: "bg-destructive/10 border border-destructive/50 text-destructive rounded-lg p-4 mb-4 flex items-center justify-between",
                                span { "{err}" }
                                button {
                                    class: "text-destructive hover:underline text-sm",
                                    onclick: move |_| delete_error.set(None),
                                    "Dismiss"
                                }
                            }
                        }

                        // Collections list
                        div { class: "space-y-4",
                            for collection in collections.read().iter() {
                                div {
                                    key: "{collection.d_tag}",
                                    class: "bg-card border border-border rounded-lg overflow-hidden",

                                    div { class: "flex items-start gap-4 p-4",
                                        // Collection image
                                        div { class: "w-20 h-20 bg-muted rounded-lg overflow-hidden shrink-0",
                                            if let Some(ref img) = collection.image {
                                                img {
                                                    src: "{img}",
                                                    class: "w-full h-full object-cover",
                                                    loading: "lazy"
                                                }
                                            } else {
                                                div { class: "w-full h-full flex items-center justify-center text-2xl",
                                                    "📁"
                                                }
                                            }
                                        }

                                        // Collection info
                                        div { class: "flex-1 min-w-0",
                                            Link {
                                                to: Route::ShopCollection { naddr: collection.naddr.clone() },
                                                h3 { class: "font-medium truncate hover:text-blue-500 transition",
                                                    "{collection.title}"
                                                }
                                            }
                                            p { class: "text-sm text-muted-foreground",
                                                {
                                                    let count = collection.products.len();
                                                    if count == 1 {
                                                        format!("{} product", count)
                                                    } else {
                                                        format!("{} products", count)
                                                    }
                                                }
                                            }
                                            if let Some(ref desc) = collection.description {
                                                p { class: "text-sm text-muted-foreground truncate mt-1",
                                                    "{desc}"
                                                }
                                            }
                                        }

                                        // Action buttons
                                        div { class: "flex items-center gap-2 shrink-0",
                                            // Delete collection button
                                            {
                                                let collection_clone = collection.clone();
                                                let is_deleting = deleting.read().as_ref().map(|d| d == &collection.d_tag).unwrap_or(false);
                                                rsx! {
                                                    button {
                                                        class: "p-2 hover:bg-destructive/10 text-destructive rounded-full transition disabled:opacity-50",
                                                        title: "Delete Collection",
                                                        disabled: is_deleting,
                                                        onclick: move |e| {
                                                            e.stop_propagation();
                                                            confirm_delete_collection.set(Some(collection_clone.clone()));
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
                } // end Collections Tab

                // Stats Tab
                if *active_tab.read() == MerchantTab::Stats {
                    {
                        let stats = get_shop_stats();
                        rsx! {
                            div { class: "space-y-6",
                                // Stats header
                                div { class: "text-center mb-6",
                                    h2 { class: "text-2xl font-bold mb-2", "Marketplace Statistics" }
                                    p { class: "text-muted-foreground", "Overview of products in the marketplace" }
                                }

                                // Stats cards
                                div { class: "grid grid-cols-2 md:grid-cols-4 gap-4",
                                    // Total products
                                    div { class: "bg-card border border-border rounded-lg p-4 text-center",
                                        div { class: "text-3xl font-bold text-blue-500 mb-1", "{stats.total_products}" }
                                        p { class: "text-sm text-muted-foreground", "Total Products" }
                                    }

                                    // Digital products
                                    div { class: "bg-card border border-border rounded-lg p-4 text-center",
                                        div { class: "text-3xl font-bold text-purple-500 mb-1", "{stats.digital_products}" }
                                        p { class: "text-sm text-muted-foreground", "Digital" }
                                    }

                                    // Physical products
                                    div { class: "bg-card border border-border rounded-lg p-4 text-center",
                                        div { class: "text-3xl font-bold text-green-500 mb-1", "{stats.physical_products}" }
                                        p { class: "text-sm text-muted-foreground", "Physical" }
                                    }

                                    // Merchants
                                    div { class: "bg-card border border-border rounded-lg p-4 text-center",
                                        div { class: "text-3xl font-bold text-amber-500 mb-1", "{stats.merchants}" }
                                        p { class: "text-sm text-muted-foreground", "Merchants" }
                                    }
                                }

                                // Categories section
                                if !stats.categories.is_empty() {
                                    div { class: "bg-card border border-border rounded-lg p-4",
                                        h3 { class: "font-semibold mb-3", "Categories ({stats.categories.len()})" }
                                        div { class: "flex flex-wrap gap-2",
                                            for category in stats.categories.iter() {
                                                span {
                                                    key: "{category}",
                                                    class: "px-3 py-1 bg-muted rounded-full text-sm",
                                                    "{category}"
                                                }
                                            }
                                        }
                                    }
                                }

                                // My shop stats
                                div { class: "bg-card border border-border rounded-lg p-4",
                                    h3 { class: "font-semibold mb-3", "My Shop" }
                                    div { class: "grid grid-cols-2 gap-4 text-center",
                                        div {
                                            div { class: "text-2xl font-bold", "{product_count}" }
                                            p { class: "text-xs text-muted-foreground", "Products" }
                                        }
                                        div {
                                            div { class: "text-2xl font-bold", "{collection_count}" }
                                            p { class: "text-xs text-muted-foreground", "Collections" }
                                        }
                                    }
                                }

                                // Order stats
                                div { class: "bg-card border border-border rounded-lg p-4",
                                    h3 { class: "font-semibold mb-3", "Orders" }
                                    div { class: "grid grid-cols-2 gap-4 text-center",
                                        div {
                                            div { class: "text-2xl font-bold text-blue-500", "{get_active_order_count()}" }
                                            p { class: "text-xs text-muted-foreground", "My Purchases" }
                                        }
                                        div {
                                            div { class: "text-2xl font-bold text-green-500", "{get_seller_active_order_count()}" }
                                            p { class: "text-xs text-muted-foreground", "Sales to Fulfill" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } // end Stats Tab
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
                                                            delete_error.set(Some(format!("Failed to delete: {}", e)));
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

            // Delete collection confirmation modal
            if let Some(collection_to_delete) = confirm_delete_collection.read().as_ref() {
                div { class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
                    onclick: move |_| confirm_delete_collection.set(None),

                    div {
                        class: "bg-background rounded-lg max-w-sm w-full p-6",
                        onclick: move |e| e.stop_propagation(),

                        div { class: "text-center",
                            div { class: "w-16 h-16 mx-auto mb-4 bg-destructive/10 rounded-full flex items-center justify-center",
                                crate::components::icons::TrashIcon { class: "w-8 h-8 text-destructive" }
                            }
                            h2 { class: "text-xl font-semibold mb-2", "Delete Collection?" }
                            p { class: "text-muted-foreground mb-6",
                                "Are you sure you want to delete \"{collection_to_delete.title}\"? This will not delete the products in the collection."
                            }

                            div { class: "flex gap-3",
                                button {
                                    class: "flex-1 py-2 border border-border rounded-lg hover:bg-accent transition",
                                    onclick: move |_| confirm_delete_collection.set(None),
                                    "Cancel"
                                }
                                {
                                    let d_tag = collection_to_delete.d_tag.clone();
                                    rsx! {
                                        button {
                                            class: "flex-1 py-2 bg-destructive hover:bg-destructive/90 text-destructive-foreground rounded-lg transition disabled:opacity-50",
                                            disabled: deleting.read().is_some(),
                                            onclick: move |_| {
                                                let d_tag_clone = d_tag.clone();
                                                deleting.set(Some(d_tag_clone.clone()));
                                                spawn(async move {
                                                    match delete_collection(&d_tag_clone).await {
                                                        Ok(_) => {
                                                            // Refresh collections
                                                            if let Ok(c) = fetch_my_collections().await {
                                                                collections.set(c);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            log::error!("Failed to delete collection: {}", e);
                                                            delete_error.set(Some(format!("Failed to delete: {}", e)));
                                                        }
                                                    }
                                                    deleting.set(None);
                                                    confirm_delete_collection.set(None);
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
