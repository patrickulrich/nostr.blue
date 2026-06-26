//! Shop Product Detail - View a single product (NIP-99 Kind 30402)
use crate::components::shop::{
    ConditionBadge, ImageCarousel, MerchantCard, QuantitySelector, ReviewCard,
};
use crate::routes::Route;
use crate::stores::shop_store::{
    add_to_cart, fetch_product_by_naddr, fetch_product_reviews, fetch_shipping_options,
    mark_product_sold, CART_ITEMS,
};
use crate::utils::nip99::{Product, ProductReview, ShippingOption};
use dioxus::prelude::*;
/// Product detail page
#[component]
pub fn ShopProductViewer(naddr: String) -> Element {
    let mut product = use_signal(|| None::<Product>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut quantity = use_signal(|| 1u32);
    let mut added_to_cart = use_signal(|| false);
    let mut reviews = use_signal(Vec::<ProductReview>::new);
    let mut reviews_loading = use_signal(|| false);
    let mut shipping_opts = use_signal(Vec::<ShippingOption>::new);
    let mut shipping_loading = use_signal(|| false);
    let mut marking_sold = use_signal(|| false);
    let naddr_clone = naddr.clone();
    use_effect(move || {
        let naddr = naddr_clone.clone();
        spawn(async move {
            loading.set(true);
            error.set(None);
            match fetch_product_by_naddr(&naddr).await {
                Ok(Some(p)) => {
                    product.set(Some(p.clone()));
                    reviews_loading.set(true);
                    if let Ok(r) = fetch_product_reviews(&p.coordinate).await {
                        reviews.set(r);
                    }
                    reviews_loading.set(false);
                    if !p.format.is_digital() && !p.shipping_options.is_empty() {
                        shipping_loading.set(true);
                        if let Ok(opts) = fetch_shipping_options(&p.shipping_options).await {
                            shipping_opts.set(opts);
                        }
                        shipping_loading.set(false);
                    }
                }
                Ok(None) => error.set(Some("Product not found".to_string())),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });
    rsx! {
        div { class: "min-h-screen",
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
                    h1 { class: "text-xl font-bold flex-1", "Product" }
                    {
                        let cart_items = CART_ITEMS.read();
                        let cart_count = cart_items.len();
                        rsx! {
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
                    }
                }
            }
            div { class: "max-w-4xl mx-auto p-4",
                if *loading.read() {
                    div { class: "grid md:grid-cols-2 gap-8",
                        div { class: "aspect-square bg-muted rounded-lg animate-pulse" }
                        div { class: "space-y-4",
                            div { class: "h-8 bg-muted rounded w-3/4 animate-pulse" }
                            div { class: "h-6 bg-muted rounded w-1/3 animate-pulse" }
                            div { class: "h-24 bg-muted rounded animate-pulse" }
                            div { class: "h-12 bg-muted rounded animate-pulse" }
                        }
                    }
                } else if let Some(err) = error.read().as_ref().cloned() {
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "😢" }
                        h2 { class: "text-xl font-semibold mb-2", "Failed to load product" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                            onclick: move |_| {
                                let nav = navigator();
                                nav.go_back();
                            },
                            "Go Back"
                        }
                    }
                } else if let Some(prod) = product.read().clone() {
                    div { class: "grid md:grid-cols-2 gap-8",
                        div {
                            ImageCarousel {
                                images: prod.images.iter().map(|i| i.url.clone()).collect(),
                                alt: Some(prod.title.clone()),
                            }
                        }
                        div { class: "space-y-6",
                            h1 { class: "text-2xl md:text-3xl font-bold", "{prod.title}" }
                            div { class: "flex items-baseline gap-3",
                                {
                                    let price_sats = if prod.price.is_sats() { prod.price.amount as u64 } else { 0 };
                                    if price_sats > 0 {
                                        rsx! {
                                            span { class: "text-3xl font-bold text-amber-500", "⚡{price_sats}" }
                                            span { class: "text-muted-foreground", "sats" }
                                        }
                                    } else {
                                        rsx! {
                                            span { class: "text-3xl font-bold", "{prod.price.display()}" }
                                        }
                                    }
                                }
                            }
                            if let Some(stock) = prod.stock {
                                if stock == 0 {
                                    span { class: "inline-flex items-center px-3 py-1 rounded-full text-sm font-medium bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400",
                                        "Out of Stock"
                                    }
                                } else if stock < 5 {
                                    span { class: "inline-flex items-center px-3 py-1 rounded-full text-sm font-medium bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400",
                                        "Only {stock} left"
                                    }
                                } else {
                                    span { class: "inline-flex items-center px-3 py-1 rounded-full text-sm font-medium bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400",
                                        "In Stock"
                                    }
                                }
                            }
                            div { class: "flex gap-2 flex-wrap",
                                span { class: "inline-flex items-center px-3 py-1 rounded-full text-sm font-medium bg-muted",
                                    if prod.format.is_digital() {
                                        "📥 Digital Product"
                                    } else {
                                        "📦 Physical Product"
                                    }
                                }
                                if let Some(ref cond) = prod.condition {
                                    ConditionBadge { condition: cond.clone() }
                                }
                            }
                            if prod.stock != Some(0) {
                                div { class: "space-y-4 pt-4 border-t border-border",
                                    div { class: "flex items-center gap-4",
                                        span { class: "text-sm font-medium", "Quantity:" }
                                        QuantitySelector {
                                            quantity: *quantity.read(),
                                            max: prod.stock.unwrap_or(99),
                                            on_change: move |q: u32| quantity.set(q),
                                        }
                                    }
                                    {
                                        let prod_clone = prod.clone();
                                        let qty = *quantity.read();
                                        let cart_items = CART_ITEMS.read();
                                        let in_cart = cart_items.iter().any(|item| item.product.naddr == prod.naddr);
                                        let in_cart_qty = cart_items
                                            .iter()
                                            .find(|item| item.product.naddr == prod.naddr)
                                            .map(|item| item.quantity)
                                            .unwrap_or(0);
                                        rsx! {
                                            button {
                                                class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition flex items-center justify-center gap-2",
                                                onclick: move |_| {
                                                    add_to_cart(prod_clone.clone(), qty);
                                                    added_to_cart.set(true);
                                                },
                                                crate::components::icons::ShoppingCartIcon { class: "w-5 h-5" }
                                                if *added_to_cart.read() || in_cart {
                                                    "Add More to Cart"
                                                } else {
                                                    "Add to Cart"
                                                }
                                            }
                                            if in_cart {
                                                p { class: "text-sm text-center text-muted-foreground", "You have {in_cart_qty} in your cart" }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(desc) = &prod.description {
                                div { class: "pt-4 border-t border-border",
                                    h2 { class: "text-lg font-semibold mb-3", "Description" }
                                    p { class: "text-muted-foreground whitespace-pre-wrap",
                                        "{desc}"
                                    }
                                }
                            }
                            // Owner actions: mark as sold (NIP-99 `status: sold`).
                            {
                                let is_owner = crate::stores::auth_store::get_pubkey()
                                    .as_deref()
                                    .map(|pk| pk == prod.pubkey)
                                    .unwrap_or(false);
                                let sold = prod.is_sold();
                if is_owner && !sold {
                    let prod_for_sold = prod.clone();
                    rsx! {
                        div { class: "pt-4 border-t border-border",
                            button {
                                class: "w-full py-3 border border-amber-500 text-amber-600 dark:text-amber-400 hover:bg-amber-500/10 rounded-lg font-medium transition disabled:opacity-50",
                                disabled: *marking_sold.read(),
                                onclick: move |_| {
                                    marking_sold.set(true);
                                    let prod = prod_for_sold.clone();
                                    let naddr = naddr.clone();
                                    spawn(async move {
                                        match mark_product_sold(prod).await {
                                            Ok(_) => {
                                                if let Ok(Some(updated)) =
                                                    fetch_product_by_naddr(&naddr).await
                                                {
                                                    product.set(Some(updated));
                                                }
                                            }
                                            Err(e) => log::error!("Failed to mark sold: {}", e),
                                        }
                                        marking_sold.set(false);
                                    });
                                },
                                if *marking_sold.read() { "Marking..." } else { "Mark as Sold" }
                            }
                        }
                    }
                } else if sold {
                    rsx! {
                        div { class: "pt-4 border-t border-border",
                            span { class: "inline-flex items-center px-3 py-1 rounded-full text-sm font-medium bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-400",
                                "Sold"
                            }
                        }
                    }
                } else {
                    rsx! {}
                }
            }
                            if !prod.specs.is_empty() {
                                div { class: "pt-4 border-t border-border",
                                    h2 { class: "text-lg font-semibold mb-3", "Specifications" }
                                    dl { class: "grid grid-cols-2 gap-2 text-sm",
                                        for spec in prod.specs.iter() {
                                            dt { class: "text-muted-foreground", "{spec.key}" }
                                            dd { class: "font-medium", "{spec.value}" }
                                        }
                                    }
                                }
                            }
                            if prod.location.is_some() || prod.geohash.is_some() {
                                div { class: "pt-4 border-t border-border",
                                    h2 { class: "text-lg font-semibold mb-2", "Location" }
                                    if let Some(loc) = &prod.location {
                                        p { class: "text-muted-foreground text-sm", "{loc}" }
                                    }
                                    if let Some(g) = &prod.geohash {
                                        p { class: "text-muted-foreground text-xs",
                                            "Geohash: {g}"
                                        }
                                    }
                                }
                            }
                            if !prod.format.is_digital() && !prod.shipping_options.is_empty() {
                                div { class: "pt-4 border-t border-border",
                                    h2 { class: "text-lg font-semibold mb-3", "Shipping Options" }
                                    if *shipping_loading.read() {
                                        div { class: "text-muted-foreground text-sm",
                                            "Loading shipping options..."
                                        }
                                    } else if !shipping_opts.read().is_empty() {
                                        div { class: "space-y-3",
                                            for opt in shipping_opts.read().iter() {
                                                div { class: "bg-muted rounded-lg p-3",
                                                    div { class: "flex items-center justify-between",
                                                        div {
                                                            span { class: "font-medium",
                                                                "{opt.title}"
                                                            }
                                                            for country in opt.countries.iter() {
                                                                span { class: "ml-2 text-xs text-muted-foreground bg-background px-2 py-0.5 rounded",
                                                                    "{country}"
                                                                }
                                                            }
                                                        }
                                                        span { class: "text-amber-500 font-medium",
                                                            "{opt.display_price()}"
                                                        }
                                                    }
                                                    if let Some(duration) = opt.display_duration() {
                                                        p { class: "text-sm text-muted-foreground mt-1",
                                                            "Estimated delivery: {duration}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        div { class: "flex flex-wrap gap-2",
                                            for region in prod.shipping_options.iter() {
                                                span { class: "px-3 py-1 bg-muted rounded-full text-sm",
                                                    "{region}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if prod.has_digital_delivery() {
                                div { class: "pt-4 border-t border-border",
                                    div { class: "bg-blue-500/10 border border-blue-500/30 rounded-lg p-4",
                                        div { class: "flex items-center gap-2 mb-2",
                                            span { class: "text-lg", "📥" }
                                            h3 { class: "font-medium", "Digital Download" }
                                        }
                                        if let Some(info) = prod.get_download_info() {
                                            p { class: "text-sm text-muted-foreground",
                                                "{info}"
                                            }
                                        }
                                        p { class: "text-xs text-muted-foreground mt-2",
                                            "You'll receive download access after purchase"
                                        }
                                    }
                                }
                            }
                            if !prod.categories.is_empty() {
                                div { class: "pt-4 border-t border-border",
                                    h2 { class: "text-lg font-semibold mb-3", "Categories" }
                                    div { class: "flex flex-wrap gap-2",
                                        for cat in prod.categories.iter() {
                                            span { class: "px-3 py-1 bg-muted rounded-full text-sm",
                                                "{cat}"
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "pt-4 border-t border-border",
                                h2 { class: "text-lg font-semibold mb-3", "Seller" }
                                {
                                    let reviews_list = reviews.read();
                                    let review_count = reviews_list.len();
                                    let avg_rating = if review_count > 0 {
                                        let total: f64 = reviews_list.iter().map(|r| r.as_stars()).sum();
                                        Some((total / review_count as f64) as f32)
                                    } else {
                                        None
                                    };
                                    rsx! {
                                        MerchantCard {
                                            pubkey: prod.pubkey.clone(),
                                            review_count: Some(review_count),
                                            avg_rating,
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "mt-12 pt-8 border-t border-border col-span-full",
                        div { class: "flex items-center justify-between mb-6",
                            h2 { class: "text-xl font-bold", "Customer Reviews" }
                            span { class: "text-muted-foreground", "{reviews.read().len()} reviews" }
                        }
                        div { class: "space-y-4",
                            if *reviews_loading.read() {
                                div { class: "text-center py-8",
                                    p { class: "text-muted-foreground", "Loading reviews..." }
                                }
                            } else if reviews.read().is_empty() {
                                div { class: "text-center py-8 bg-muted/50 rounded-lg",
                                    p { class: "text-muted-foreground", "No reviews yet" }
                                }
                            } else {
                                for review in reviews.read().iter().cloned() {
                                    ReviewCard { key: "{review.event_id}", review }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "📦" }
                        h2 { class: "text-xl font-semibold mb-2", "Product not found" }
                        Link {
                            to: Route::ShopHome {},
                            class: "text-blue-500 hover:underline",
                            "Browse products"
                        }
                    }
                }
            }
        }
    }
}
