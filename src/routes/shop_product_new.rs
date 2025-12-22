//! Shop Product New - Create a new product listing (NIP-99 Kind 30402)

use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::shop_store::{ProductFormData, publish_product};

/// Product creation form
#[component]
pub fn ShopProductNew() -> Element {
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut price = use_signal(String::new);
    let mut currency = use_signal(|| "sats".to_string());
    let mut image_url = use_signal(String::new);
    let mut is_digital = use_signal(|| false);
    let mut stock = use_signal(String::new);
    let mut categories = use_signal(String::new);
    let mut shipping_regions = use_signal(String::new);
    let mut condition = use_signal(|| "new".to_string());
    let mut publishing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);

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
                    h1 { class: "text-xl font-bold", "New Product" }
                }
            }

            // Form content
            div { class: "max-w-2xl mx-auto p-4",
                if *success.read() {
                    // Success message
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "✅" }
                        h2 { class: "text-xl font-semibold mb-2", "Product Listed!" }
                        p { class: "text-muted-foreground mb-6",
                            "Your product is now live on the marketplace."
                        }
                        div { class: "space-y-3",
                            Link {
                                to: Route::ShopMerchant {},
                                class: "block w-full py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                                "View My Products"
                            }
                            button {
                                class: "block w-full py-3 border border-border hover:bg-accent rounded-lg font-medium transition",
                                onclick: move |_| {
                                    // Reset form
                                    title.set(String::new());
                                    description.set(String::new());
                                    price.set(String::new());
                                    image_url.set(String::new());
                                    is_digital.set(false);
                                    stock.set(String::new());
                                    categories.set(String::new());
                                    shipping_regions.set(String::new());
                                    condition.set("new".to_string());
                                    success.set(false);
                                },
                                "Create Another Product"
                            }
                        }
                    }
                } else {
                    // Form
                    div { class: "space-y-6",
                        // Title
                        div {
                            label { class: "block text-sm font-medium mb-2", "Product Title *" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                placeholder: "Enter product title",
                                value: "{title}",
                                oninput: move |e| title.set(e.value())
                            }
                        }

                        // Description
                        div {
                            label { class: "block text-sm font-medium mb-2", "Description" }
                            textarea {
                                class: "w-full h-32 px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none",
                                placeholder: "Describe your product...",
                                value: "{description}",
                                oninput: move |e| description.set(e.value())
                            }
                        }

                        // Price
                        div { class: "grid grid-cols-2 gap-4",
                            div {
                                label { class: "block text-sm font-medium mb-2", "Price *" }
                                input {
                                    r#type: "number",
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                    placeholder: "0",
                                    min: "0",
                                    step: "any",
                                    value: "{price}",
                                    oninput: move |e| price.set(e.value())
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Currency" }
                                select {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                    value: "{currency}",
                                    onchange: move |e| currency.set(e.value()),
                                    option { value: "sats", "Sats" }
                                    option { value: "USD", "USD" }
                                    option { value: "EUR", "EUR" }
                                }
                            }
                        }

                        // Image URL
                        div {
                            label { class: "block text-sm font-medium mb-2", "Image URL" }
                            input {
                                r#type: "url",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                placeholder: "https://example.com/image.jpg",
                                value: "{image_url}",
                                oninput: move |e| image_url.set(e.value())
                            }
                            p { class: "text-xs text-muted-foreground mt-1",
                                "Enter a direct URL to your product image"
                            }
                        }

                        // Image preview
                        if !image_url.read().is_empty() {
                            div { class: "aspect-video bg-muted rounded-lg overflow-hidden",
                                img {
                                    src: "{image_url}",
                                    class: "w-full h-full object-contain"
                                }
                            }
                        }

                        // Product type
                        div {
                            label { class: "block text-sm font-medium mb-2", "Product Type" }
                            div { class: "flex gap-4",
                                button {
                                    r#type: "button",
                                    class: if !*is_digital.read() {
                                        "flex-1 py-3 px-4 bg-blue-500 text-white rounded-lg font-medium"
                                    } else {
                                        "flex-1 py-3 px-4 bg-muted text-muted-foreground rounded-lg font-medium hover:bg-accent transition"
                                    },
                                    onclick: move |_| is_digital.set(false),
                                    "📦 Physical"
                                }
                                button {
                                    r#type: "button",
                                    class: if *is_digital.read() {
                                        "flex-1 py-3 px-4 bg-blue-500 text-white rounded-lg font-medium"
                                    } else {
                                        "flex-1 py-3 px-4 bg-muted text-muted-foreground rounded-lg font-medium hover:bg-accent transition"
                                    },
                                    onclick: move |_| is_digital.set(true),
                                    "📥 Digital"
                                }
                            }
                        }

                        // Condition (for physical products)
                        if !*is_digital.read() {
                            div {
                                label { class: "block text-sm font-medium mb-2", "Condition" }
                                select {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                    value: "{condition}",
                                    onchange: move |e| condition.set(e.value()),
                                    option { value: "new", "New" }
                                    option { value: "like_new", "Like New" }
                                    option { value: "used", "Used" }
                                    option { value: "fair", "Fair" }
                                    option { value: "refurbished", "Refurbished" }
                                }
                            }
                        }

                        // Stock (optional)
                        div {
                            label { class: "block text-sm font-medium mb-2", "Stock Quantity (optional)" }
                            input {
                                r#type: "number",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                placeholder: "Leave empty for unlimited",
                                min: "0",
                                value: "{stock}",
                                oninput: move |e| stock.set(e.value())
                            }
                        }

                        // Categories
                        div {
                            label { class: "block text-sm font-medium mb-2", "Categories (comma-separated)" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                placeholder: "electronics, gadgets, accessories",
                                value: "{categories}",
                                oninput: move |e| categories.set(e.value())
                            }
                        }

                        // Shipping regions (for physical products)
                        if !*is_digital.read() {
                            div {
                                label { class: "block text-sm font-medium mb-2", "Ships to (comma-separated)" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                    placeholder: "Worldwide, US, EU",
                                    value: "{shipping_regions}",
                                    oninput: move |e| shipping_regions.set(e.value())
                                }
                            }
                        }

                        // Error message
                        if let Some(err) = error.read().as_ref() {
                            div { class: "bg-destructive/10 border border-destructive/50 text-destructive rounded-lg p-4",
                                "{err}"
                            }
                        }

                        // Submit button
                        button {
                            class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                            disabled: *publishing.read() || title.read().trim().is_empty() || price.read().trim().is_empty(),
                            onclick: move |_| {
                                publishing.set(true);
                                error.set(None);

                                let is_digital_val = *is_digital.read();
                                let form_data = ProductFormData {
                                    title: title.read().clone(),
                                    description: description.read().clone(),
                                    price_amount: price.read().parse().unwrap_or(0.0),
                                    price_currency: currency.read().clone(),
                                    images: if image_url.read().trim().is_empty() {
                                        vec![]
                                    } else {
                                        vec![image_url.read().clone()]
                                    },
                                    categories: categories.read()
                                        .split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect(),
                                    is_digital: is_digital_val,
                                    stock: stock.read().parse().ok(),
                                    specs: vec![],
                                    shipping_regions: shipping_regions.read()
                                        .split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect(),
                                    condition: if is_digital_val { None } else { Some(condition.read().clone()) },
                                };

                                spawn(async move {
                                    match publish_product(form_data).await {
                                        Ok(d_tag) => {
                                            log::info!("Product published: {}", d_tag);
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
                                "Publishing..."
                            } else {
                                "Publish Product"
                            }
                        }
                    }
                }
            }
        }
    }
}
