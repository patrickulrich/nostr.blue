//! Shop Collection New - Create a new product collection (NIP-99 Kind 30405)
use crate::routes::Route;
use crate::stores::shop_store::{fetch_my_products, publish_collection, CollectionFormData};
use crate::utils::nip99::Product;
use dioxus::prelude::*;
/// Collection creation form
#[component]
pub fn ShopCollectionNew() -> Element {
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut image_url = use_signal(String::new);
    let mut selected_products = use_signal(Vec::<String>::new);
    let mut available_products = use_signal(Vec::<Product>::new);
    let mut loading_products = use_signal(|| true);
    let mut publishing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            loading_products.set(true);
            match fetch_my_products().await {
                Ok(products) => available_products.set(products),
                Err(e) => log::error!("Failed to fetch products: {}", e),
            }
            loading_products.set(false);
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
                    h1 { class: "text-xl font-bold", "New Collection" }
                }
            }
            div { class: "max-w-2xl mx-auto p-4",
                if *success.read() {
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "✅" }
                        h2 { class: "text-xl font-semibold mb-2", "Collection Created!" }
                        p { class: "text-muted-foreground mb-6", "Your collection is now live." }
                        div { class: "space-y-3",
                            Link {
                                to: Route::ShopMerchant {},
                                class: "block w-full py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                                "View My Collections"
                            }
                            button {
                                class: "block w-full py-3 border border-border hover:bg-accent rounded-lg font-medium transition",
                                onclick: move |_| {
                                    title.set(String::new());
                                    description.set(String::new());
                                    image_url.set(String::new());
                                    selected_products.set(Vec::new());
                                    success.set(false);
                                },
                                "Create Another Collection"
                            }
                        }
                    }
                } else {
                    div { class: "space-y-6",
                        div {
                            label { class: "block text-sm font-medium mb-2", "Collection Title *" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "e.g., Summer Sale, Best Sellers",
                                value: "{title}",
                                oninput: move |e| title.set(e.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Description" }
                            textarea {
                                class: "w-full h-24 px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none",
                                placeholder: "Describe this collection...",
                                value: "{description}",
                                oninput: move |e| description.set(e.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Cover Image URL" }
                            input {
                                r#type: "url",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "https://example.com/collection-image.jpg",
                                value: "{image_url}",
                                oninput: move |e| image_url.set(e.value()),
                            }
                        }
                        if !image_url.read().is_empty() {
                            div { class: "aspect-video bg-muted rounded-lg overflow-hidden max-w-xs",
                                img {
                                    src: "{image_url}",
                                    class: "w-full h-full object-cover",
                                }
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Select Products ({selected_products.read().len()} selected)"
                            }
                            if *loading_products.read() {
                                div { class: "text-muted-foreground text-sm", "Loading products..." }
                            } else if available_products.read().is_empty() {
                                div { class: "bg-muted/50 rounded-lg p-4 text-center",
                                    p { class: "text-muted-foreground mb-2", "No products available" }
                                    Link {
                                        to: Route::ShopProductNew {},
                                        class: "text-blue-500 hover:underline text-sm",
                                        "Create a product first"
                                    }
                                }
                            } else {
                                div { class: "border border-border rounded-lg max-h-64 overflow-y-auto",
                                    for product in available_products.read().iter() {
                                        {
                                            let coord = product.coordinate.clone();
                                            let is_selected = selected_products.read().contains(&coord);
                                            let product_title = product.title.clone();
                                            let product_img = product.images.first().map(|i| i.url.clone());
                                            let coord_for_click = coord.clone();
                                            rsx! {
                                                button {
                                                    key: "{coord}",
                                                    r#type: "button",
                                                    class: if is_selected { "w-full p-3 flex items-center gap-3 bg-blue-500/10 border-b border-border last:border-0 text-left" } else { "w-full p-3 flex items-center gap-3 hover:bg-accent border-b border-border last:border-0 text-left transition" },
                                                    onclick: move |_| {
                                                        let mut selected = selected_products.write();
                                                        if selected.contains(&coord_for_click) {
                                                            selected.retain(|c| c != &coord_for_click);
                                                        } else {
                                                            selected.push(coord_for_click.clone());
                                                        }
                                                    },
                                                    div { class: if is_selected { "w-5 h-5 rounded border-2 border-blue-500 bg-blue-500 flex items-center justify-center" } else { "w-5 h-5 rounded border-2 border-border" },
                                                        if is_selected {
                                                            span { class: "text-white text-xs", "✓" }
                                                        }
                                                    }
                                                    div { class: "w-10 h-10 bg-muted rounded overflow-hidden shrink-0",
                                                        if let Some(ref img) = product_img {
                                                            img { src: "{img}", class: "w-full h-full object-cover" }
                                                        } else {
                                                            div { class: "w-full h-full flex items-center justify-center text-sm", "📦" }
                                                        }
                                                    }
                                                    span { class: "flex-1 truncate", "{product_title}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(err) = error.read().as_ref() {
                            div { class: "bg-destructive/10 border border-destructive/50 text-destructive rounded-lg p-4",
                                "{err}"
                            }
                        }
                        button {
                            class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                            disabled: *publishing.read() || title.read().trim().is_empty()
                                || selected_products.read().is_empty(),
                            onclick: move |_| {
                                publishing.set(true);
                                error.set(None);
                                let form_data = CollectionFormData {
                                    title: title.read().clone(),
                                    description: description.read().clone(),
                                    image: if image_url.read().trim().is_empty() {
                                        None
                                    } else {
                                        Some(image_url.read().clone())
                                    },
                                    products: selected_products.read().clone(),
                                    shipping_options: vec![],
                                };
                                spawn(async move {
                                    match publish_collection(form_data, None).await {
                                        Ok(d_tag) => {
                                            log::info!("Collection published: {}", d_tag);
                                            success.set(true);
                                        }
                                        Err(e) => {
                                            error.set(Some(e));
                                        }
                                    }
                                    publishing.set(false);
                                });
                            },
                            if *publishing.read() {
                                "Creating..."
                            } else {
                                "Create Collection"
                            }
                        }
                        if selected_products.read().is_empty() && !available_products.read().is_empty() {
                            p { class: "text-sm text-muted-foreground text-center",
                                "Select at least one product to create a collection"
                            }
                        }
                    }
                }
            }
        }
    }
}
