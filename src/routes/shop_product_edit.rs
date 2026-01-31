//! Shop Product Edit - Edit an existing product listing (NIP-99 Kind 30402)
use crate::routes::Route;
use crate::stores::shop_store::{fetch_product_by_naddr, update_product, ProductFormData};
use crate::utils::nip99::Product;
use dioxus::prelude::*;
/// Product edit form
#[component]
pub fn ShopProductEdit(naddr: String) -> Element {
    let mut product = use_signal(|| None::<Product>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
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
    let mut updating = use_signal(|| false);
    let mut update_error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let naddr_clone = naddr.clone();
    use_effect(move || {
        let naddr = naddr_clone.clone();
        spawn(async move {
            loading.set(true);
            error.set(None);
            match fetch_product_by_naddr(&naddr).await {
                Ok(Some(p)) => {
                    title.set(p.title.clone());
                    description.set(p.description.clone().unwrap_or_default());
                    price.set(p.price.amount.to_string());
                    currency.set(p.price.currency.clone());
                    is_digital.set(p.format.is_digital());
                    if let Some(s) = p.stock {
                        stock.set(s.to_string());
                    }
                    categories.set(p.categories.join(", "));
                    shipping_regions.set(p.shipping_options.join(", "));
                    if let Some(img) = p.images.first() {
                        image_url.set(img.url.clone());
                    }
                    if let Some(ref c) = p.condition {
                        condition.set(c.clone());
                    }
                    product.set(Some(p));
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
                    h1 { class: "text-xl font-bold", "Edit Product" }
                }
            }
            div { class: "max-w-2xl mx-auto p-4",
                if *loading.read() {
                    div { class: "space-y-6",
                        for i in 0..5 {
                            div {
                                key: "{i}",
                                class: "h-16 bg-muted rounded-lg animate-pulse",
                            }
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
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
                } else if *success.read() {
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "✅" }
                        h2 { class: "text-xl font-semibold mb-2", "Product Updated!" }
                        p { class: "text-muted-foreground mb-6", "Your changes have been saved." }
                        div { class: "space-y-3",
                            Link {
                                to: Route::ShopMerchant {},
                                class: "block w-full py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition text-center",
                                "Back to My Shop"
                            }
                        }
                    }
                } else if let Some(prod) = product.read().as_ref() {
                    div { class: "space-y-6",
                        div {
                            label { class: "block text-sm font-medium mb-2", "Product Title *" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "Enter product title",
                                value: "{title}",
                                oninput: move |e| title.set(e.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Description" }
                            textarea {
                                class: "w-full h-32 px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none",
                                placeholder: "Describe your product...",
                                value: "{description}",
                                oninput: move |e| description.set(e.value()),
                            }
                        }
                        div { class: "grid grid-cols-2 gap-4",
                            div {
                                label { class: "block text-sm font-medium mb-2", "Price *" }
                                input {
                                    r#type: "number",
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                    placeholder: "0",
                                    min: "0",
                                    step: "any",
                                    value: "{price}",
                                    oninput: move |e| price.set(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Currency" }
                                select {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                    value: "{currency}",
                                    onchange: move |e| currency.set(e.value()),
                                    option { value: "sats", "Sats" }
                                    option { value: "USD", "USD" }
                                    option { value: "EUR", "EUR" }
                                }
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Image URL" }
                            input {
                                r#type: "url",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "https://example.com/image.jpg",
                                value: "{image_url}",
                                oninput: move |e| image_url.set(e.value()),
                            }
                        }
                        if !image_url.read().is_empty() {
                            div { class: "aspect-video bg-muted rounded-lg overflow-hidden",
                                img {
                                    src: "{image_url}",
                                    class: "w-full h-full object-contain",
                                }
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Product Type" }
                            div { class: "flex gap-4",
                                button {
                                    r#type: "button",
                                    class: if !*is_digital.read() { "flex-1 py-3 px-4 bg-blue-500 text-white rounded-lg font-medium" } else { "flex-1 py-3 px-4 bg-muted text-muted-foreground rounded-lg font-medium hover:bg-accent transition" },
                                    onclick: move |_| is_digital.set(false),
                                    "Physical"
                                }
                                button {
                                    r#type: "button",
                                    class: if *is_digital.read() { "flex-1 py-3 px-4 bg-blue-500 text-white rounded-lg font-medium" } else { "flex-1 py-3 px-4 bg-muted text-muted-foreground rounded-lg font-medium hover:bg-accent transition" },
                                    onclick: move |_| is_digital.set(true),
                                    "Digital"
                                }
                            }
                        }
                        if !*is_digital.read() {
                            div {
                                label { class: "block text-sm font-medium mb-2", "Condition" }
                                select {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
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
                        div {
                            label { class: "block text-sm font-medium mb-2", "Stock Quantity (optional)" }
                            input {
                                r#type: "number",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "Leave empty for unlimited",
                                min: "0",
                                value: "{stock}",
                                oninput: move |e| stock.set(e.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Categories (comma-separated)"
                            }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "electronics, gadgets, accessories",
                                value: "{categories}",
                                oninput: move |e| categories.set(e.value()),
                            }
                        }
                        if !*is_digital.read() {
                            div {
                                label { class: "block text-sm font-medium mb-2",
                                    "Ships to (comma-separated)"
                                }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                    placeholder: "Worldwide, US, EU",
                                    value: "{shipping_regions}",
                                    oninput: move |e| shipping_regions.set(e.value()),
                                }
                            }
                        }
                        if let Some(err) = update_error.read().as_ref() {
                            div { class: "bg-destructive/10 border border-destructive/50 text-destructive rounded-lg p-4",
                                "{err}"
                            }
                        }
                        {
                            let d_tag = prod.d_tag.clone();
                            let price_valid = price.read().parse::<f64>().map(|p| p > 0.0).unwrap_or(false);
                            rsx! {
                                button {
                                    class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                                    disabled: *updating.read() || title.read().trim().is_empty() || !price_valid,
                                    onclick: move |_| {
                                        updating.set(true);
                                        update_error.set(None);
                                        let d_tag = d_tag.clone();
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
                                            categories: categories
                                                .read()
                                                .split(',')
                                                .map(|s| s.trim().to_string())
                                                .filter(|s| !s.is_empty())
                                                .collect(),
                                            is_digital: is_digital_val,
                                            stock: stock.read().parse().ok(),
                                            specs: vec![],
                                            shipping_regions: shipping_regions
                                                .read()
                                                .split(',')
                                                .map(|s| s.trim().to_string())
                                                .filter(|s| !s.is_empty())
                                                .collect(),
                                            condition: if is_digital_val { None } else { Some(condition.read().clone()) },
                                        };
                                        spawn(async move {
                                            match update_product(&d_tag, form_data).await {
                                                Ok(_) => {
                                                    success.set(true);
                                                }
                                                Err(e) => {
                                                    update_error.set(Some(e));
                                                }
                                            }
                                            updating.set(false);
                                        });
                                    },
                                    if *updating.read() {
                                        "Saving..."
                                    } else {
                                        "Save Changes"
                                    }
                                }
                            }
                        }
                        Link {
                            to: Route::ShopMerchant {},
                            class: "block w-full py-3 text-center border border-border hover:bg-accent rounded-lg font-medium transition",
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}
