//! Shop Shipping New - Create a shipping option (NIP-99 market-spec Kind 30406)
use crate::routes::Route;
use crate::stores::shop_store::{publish_shipping_option, ShippingOptionFormData};
use dioxus::prelude::*;
/// Shipping option creation form (Kind 30406).
#[component]
pub fn ShopShippingNew() -> Element {
    let mut title = use_signal(String::new);
    let mut base_price = use_signal(String::new);
    let mut currency = use_signal(|| "USD".to_string());
    let mut countries = use_signal(String::new);
    let mut service = use_signal(|| "standard".to_string());
    let mut carrier = use_signal(String::new);
    let mut regions = use_signal(String::new);
    let mut location = use_signal(String::new);
    let mut geohash = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut publishing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
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
                    h1 { class: "text-xl font-bold", "New Shipping Option" }
                }
            }
            div { class: "max-w-2xl mx-auto p-4",
                if *success.read() {
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "✅" }
                        h2 { class: "text-xl font-semibold mb-2", "Shipping Option Created!" }
                        p { class: "text-muted-foreground mb-6",
                            "You can now reference it from a product via its coordinate."
                        }
                        Link {
                            to: Route::ShopMerchant {},
                            class: "block w-full py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                            "Back to My Shop"
                        }
                    }
                } else {
                    div { class: "space-y-6",
                        div {
                            label { class: "block text-sm font-medium mb-2", "Title *" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "e.g., Standard US Shipping",
                                value: "{title}",
                                oninput: move |e| title.set(e.value()),
                            }
                        }
                        div { class: "grid grid-cols-2 gap-4",
                            div {
                                label { class: "block text-sm font-medium mb-2", "Base Price *" }
                                input {
                                    r#type: "number",
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                    placeholder: "0",
                                    min: "0",
                                    step: "any",
                                    value: "{base_price}",
                                    oninput: move |e| base_price.set(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Currency" }
                                select {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                    value: "{currency}",
                                    onchange: move |e| currency.set(e.value()),
                                    option { value: "USD", "USD" }
                                    option { value: "EUR", "EUR" }
                                    option { value: "sats", "Sats" }
                                }
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Countries * (comma-separated ISO codes, e.g. US, CA)"
                            }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "US",
                                value: "{countries}",
                                oninput: move |e| countries.set(e.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Service Type" }
                            select {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                value: "{service}",
                                onchange: move |e| service.set(e.value()),
                                option { value: "standard", "Standard" }
                                option { value: "express", "Express" }
                                option { value: "overnight", "Overnight" }
                                option { value: "pickup", "Pickup" }
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Carrier (optional)" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "UPS, USPS, FedEx...",
                                value: "{carrier}",
                                oninput: move |e| carrier.set(e.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Regions (optional, comma-separated ISO 3166-2, e.g. US-FL)"
                            }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                placeholder: "US-FL, US-NY",
                                value: "{regions}",
                                oninput: move |e| regions.set(e.value()),
                            }
                        }
                        div { class: "grid grid-cols-2 gap-4",
                            div {
                                label { class: "block text-sm font-medium mb-2", "Location (optional)" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                    placeholder: "123 Main St, Downtown, FL",
                                    value: "{location}",
                                    oninput: move |e| location.set(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Geohash (optional)" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                    placeholder: "dhwm9c4ws",
                                    value: "{geohash}",
                                    oninput: move |e| geohash.set(e.value()),
                                }
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Description (optional)" }
                            textarea {
                                class: "w-full h-20 px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none",
                                placeholder: "e.g., Downtown store pickup",
                                value: "{description}",
                                oninput: move |e| description.set(e.value()),
                            }
                        }
                        if let Some(err) = error.read().as_ref() {
                            div { class: "bg-destructive/10 border border-destructive/50 text-destructive rounded-lg p-4",
                                "{err}"
                            }
                        }
                        button {
                            class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                            disabled: *publishing.read()
                                || title.read().trim().is_empty()
                                || countries.read().trim().is_empty(),
                            onclick: move |_| {
                                publishing.set(true);
                                error.set(None);
                                let split_csv = |s: &String| {
                                    s.split(',')
                                        .map(|x| x.trim().to_string())
                                        .filter(|x| !x.is_empty())
                                        .collect::<Vec<String>>()
                                };
                                let form_data = ShippingOptionFormData {
                                    title: title.read().clone(),
                                    d_tag: None,
                                    base_price: base_price.read().parse().unwrap_or(0.0),
                                    currency: currency.read().clone(),
                                    countries: split_csv(&countries.read()),
                                    service: service.read().clone(),
                                    carrier: {
                                        let c = carrier.read().trim().to_string();
                                        if c.is_empty() { None } else { Some(c) }
                                    },
                                    regions: split_csv(&regions.read()),
                                    duration_min: None,
                                    duration_max: None,
                                    duration_unit: None,
                                    location: {
                                        let l = location.read().trim().to_string();
                                        if l.is_empty() { None } else { Some(l) }
                                    },
                                    geohash: {
                                        let g = geohash.read().trim().to_string();
                                        if g.is_empty() { None } else { Some(g) }
                                    },
                                    description: description.read().clone(),
                                };
                                spawn(async move {
                                    match publish_shipping_option(form_data).await {
                                        Ok(d_tag) => {
                                            log::info!("Shipping option published: {}", d_tag);
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
                                "Create Shipping Option"
                            }
                        }
                    }
                }
            }
        }
    }
}
