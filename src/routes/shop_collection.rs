//! Shop Collection - View a product collection (NIP-99 Kind 30405)

use dioxus::prelude::*;
use crate::utils::nip99::{Product, ProductCollection};
use crate::stores::shop_store::{fetch_collection_by_naddr, fetch_collection_products};
use crate::components::shop::{ProductCard, ProductCardSkeleton};
use crate::routes::Route;

/// Collection detail page
#[component]
pub fn ShopCollection(naddr: String) -> Element {
    let mut collection = use_signal(|| None::<ProductCollection>);
    let mut products = use_signal(Vec::<Product>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Fetch collection on mount
    let naddr_clone = naddr.clone();
    use_effect(move || {
        let naddr = naddr_clone.clone();
        spawn(async move {
            loading.set(true);
            error.set(None);

            match fetch_collection_by_naddr(&naddr).await {
                Ok(Some(c)) => {
                    // Fetch products in collection
                    match fetch_collection_products(&c).await {
                        Ok(p) => products.set(p),
                        Err(e) => log::error!("Failed to fetch collection products: {}", e),
                    }
                    collection.set(Some(c));
                }
                Ok(None) => error.set(Some("Collection not found".to_string())),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

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
                    h1 { class: "text-xl font-bold flex-1",
                        if let Some(c) = collection.read().as_ref() {
                            "{c.title}"
                        } else {
                            "Collection"
                        }
                    }
                }
            }

            // Content
            div { class: "max-w-6xl mx-auto p-4",
                if *loading.read() {
                    // Loading skeleton
                    div { class: "space-y-6",
                        // Header skeleton
                        div { class: "h-8 bg-muted rounded w-1/3 animate-pulse" }
                        div { class: "h-4 bg-muted rounded w-2/3 animate-pulse" }

                        // Products skeleton
                        div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4 mt-8",
                            for i in 0..8 {
                                ProductCardSkeleton { key: "{i}" }
                            }
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "😢" }
                        h2 { class: "text-xl font-semibold mb-2", "Failed to load collection" }
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
                } else if let Some(col) = collection.read().as_ref() {
                    // Collection details
                    div { class: "space-y-6",
                        // Collection header
                        div { class: "space-y-2",
                            h1 { class: "text-2xl md:text-3xl font-bold", "{col.title}" }

                            if let Some(desc) = &col.description {
                                p { class: "text-muted-foreground", "{desc}" }
                            }

                            // Metadata
                            div { class: "flex items-center gap-4 text-sm text-muted-foreground",
                                span { "{products.read().len()} products" }
                                {
                                    let pubkey_short = if col.pubkey.len() > 8 {
                                        format!("by {}...", &col.pubkey[..8])
                                    } else {
                                        format!("by {}", &col.pubkey)
                                    };
                                    rsx! { span { "{pubkey_short}" } }
                                }
                            }
                        }

                        // Image if available
                        if let Some(image) = &col.image {
                            div { class: "aspect-[3/1] bg-muted rounded-lg overflow-hidden",
                                img {
                                    src: "{image}",
                                    class: "w-full h-full object-cover",
                                    loading: "lazy"
                                }
                            }
                        }

                        // Products grid
                        if products.read().is_empty() {
                            div { class: "text-center py-12 bg-muted/50 rounded-lg",
                                div { class: "text-6xl mb-4", "📦" }
                                h2 { class: "text-lg font-semibold mb-2", "No products yet" }
                                p { class: "text-muted-foreground",
                                    "This collection doesn't have any products."
                                }
                            }
                        } else {
                            div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                                for product in products.read().iter() {
                                    ProductCard {
                                        key: "{product.naddr}",
                                        product: product.clone(),
                                        show_add_to_cart: true
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Collection not found
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "📚" }
                        h2 { class: "text-xl font-semibold mb-2", "Collection not found" }
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
