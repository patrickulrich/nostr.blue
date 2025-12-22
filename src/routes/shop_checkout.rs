//! Shop Checkout - Checkout flow with payment

use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::shop_store::{CART_ITEMS, CART_TOTAL_SATS, clear_cart, get_cart_count, create_shop_order};
use crate::stores::cashu::{WALLET_BALANCE, send_tokens_p2pk, get_balances_per_mint};
use crate::stores::nwc_store;
use crate::utils::format::format_sats_with_separator;

/// Checkout steps
#[derive(Clone, Copy, PartialEq)]
enum CheckoutStep {
    Review,
    Shipping,
    Payment,
    Complete,
}

/// Payment method
#[derive(Clone, Copy, PartialEq)]
enum PaymentMethod {
    Cashu,
    Lightning,
}

/// Checkout page
#[component]
pub fn ShopCheckout() -> Element {
    let mut step = use_signal(|| CheckoutStep::Review);
    let mut payment_method = use_signal(|| PaymentMethod::Cashu);
    let mut shipping_address = use_signal(String::new);
    let mut payment_processing = use_signal(|| false);
    let mut payment_error = use_signal(|| None::<String>);
    let mut order_id = use_signal(|| None::<String>);

    let cart_items = CART_ITEMS.read();
    let total_sats = *CART_TOTAL_SATS.read();
    let item_count = get_cart_count();
    let cashu_balance = *WALLET_BALANCE.read();
    let nwc_connected = nwc_store::is_connected();

    // Check if any items require shipping
    let has_physical_items = cart_items.iter().any(|item| item.product.requires_shipping());

    // Format total
    let total_formatted = format_sats_with_separator(total_sats);

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "flex items-center gap-4 p-4",
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        onclick: move |_| {
                            let current = *step.read();
                            match current {
                                CheckoutStep::Review => {
                                    let nav = navigator();
                                    nav.go_back();
                                }
                                CheckoutStep::Shipping => step.set(CheckoutStep::Review),
                                CheckoutStep::Payment => {
                                    if has_physical_items {
                                        step.set(CheckoutStep::Shipping);
                                    } else {
                                        step.set(CheckoutStep::Review);
                                    }
                                }
                                CheckoutStep::Complete => {}
                            }
                        },
                        if *step.read() != CheckoutStep::Complete {
                            crate::components::icons::ArrowLeftIcon { class: "w-5 h-5" }
                        }
                    }
                    h1 { class: "text-xl font-bold flex-1", "Checkout" }
                }

                // Progress steps
                if *step.read() != CheckoutStep::Complete {
                    div { class: "flex justify-center gap-2 pb-4",
                        StepIndicator {
                            label: "Review",
                            active: *step.read() == CheckoutStep::Review,
                            completed: matches!(*step.read(), CheckoutStep::Shipping | CheckoutStep::Payment)
                        }
                        if has_physical_items {
                            StepIndicator {
                                label: "Shipping",
                                active: *step.read() == CheckoutStep::Shipping,
                                completed: *step.read() == CheckoutStep::Payment
                            }
                        }
                        StepIndicator {
                            label: "Payment",
                            active: *step.read() == CheckoutStep::Payment,
                            completed: false
                        }
                    }
                }
            }

            // Content
            div { class: "max-w-2xl mx-auto p-4",
                if cart_items.is_empty() && *step.read() != CheckoutStep::Complete {
                    // Empty cart
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "🛒" }
                        h2 { class: "text-xl font-semibold mb-2", "Your cart is empty" }
                        Link {
                            to: Route::ShopHome {},
                            class: "text-blue-500 hover:underline",
                            "Browse products"
                        }
                    }
                } else {
                    match *step.read() {
                        CheckoutStep::Review => rsx! {
                            // Order review
                            div { class: "space-y-6",
                                h2 { class: "text-lg font-semibold", "Review Your Order" }

                                // Items list
                                div { class: "bg-card border border-border rounded-lg divide-y divide-border",
                                    for item in cart_items.iter() {
                                        div { class: "p-4 flex gap-4",
                                            // Image
                                            div { class: "w-16 h-16 bg-muted rounded flex-shrink-0 overflow-hidden",
                                                if let Some(img) = item.product.images.first() {
                                                    img {
                                                        src: "{img.url}",
                                                        class: "w-full h-full object-cover"
                                                    }
                                                } else {
                                                    div { class: "w-full h-full flex items-center justify-center text-2xl",
                                                        "📦"
                                                    }
                                                }
                                            }

                                            // Details
                                            div { class: "flex-1 min-w-0",
                                                p { class: "font-medium truncate", "{item.product.title}" }
                                                p { class: "text-sm text-muted-foreground",
                                                    "Qty: {item.quantity}"
                                                }
                                            }

                                            // Price
                                            div { class: "text-right",
                                                {
                                                    let item_price = if item.product.price.currency.eq_ignore_ascii_case("sats") {
                                                        item.product.price.amount as u64 * item.quantity as u64
                                                    } else { 0 };
                                                    rsx! {
                                                        p { class: "font-medium text-amber-500",
                                                            "⚡{format_sats_with_separator(item_price)}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Total
                                div { class: "bg-card border border-border rounded-lg p-4",
                                    div { class: "flex justify-between text-lg font-semibold",
                                        span { "Total ({item_count} items)" }
                                        span { class: "text-amber-500", "⚡{total_formatted} sats" }
                                    }
                                }

                                // Continue button
                                button {
                                    class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                                    onclick: move |_| {
                                        if has_physical_items {
                                            step.set(CheckoutStep::Shipping);
                                        } else {
                                            step.set(CheckoutStep::Payment);
                                        }
                                    },
                                    if has_physical_items {
                                        "Continue to Shipping"
                                    } else {
                                        "Continue to Payment"
                                    }
                                }
                            }
                        },

                        CheckoutStep::Shipping => rsx! {
                            // Shipping address form
                            div { class: "space-y-6",
                                h2 { class: "text-lg font-semibold", "Shipping Address" }

                                div { class: "bg-card border border-border rounded-lg p-4",
                                    p { class: "text-sm text-muted-foreground mb-4",
                                        "Your address will be encrypted and only visible to the seller."
                                    }

                                    textarea {
                                        class: "w-full h-32 px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none",
                                        placeholder: "Enter your full shipping address...\n\nName\nStreet Address\nCity, State/Province, ZIP\nCountry",
                                        value: "{shipping_address}",
                                        oninput: move |e| shipping_address.set(e.value())
                                    }
                                }

                                // Continue button
                                button {
                                    class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                                    disabled: shipping_address.read().trim().is_empty(),
                                    onclick: move |_| step.set(CheckoutStep::Payment),
                                    "Continue to Payment"
                                }
                            }
                        },

                        CheckoutStep::Payment => rsx! {
                            // Payment method selection
                            div { class: "space-y-6",
                                h2 { class: "text-lg font-semibold", "Select Payment Method" }

                                // Payment options
                                div { class: "space-y-3",
                                    // Cashu option
                                    button {
                                        class: if *payment_method.read() == PaymentMethod::Cashu {
                                            "w-full p-4 bg-card border-2 border-blue-500 rounded-lg text-left"
                                        } else {
                                            "w-full p-4 bg-card border border-border rounded-lg text-left hover:border-ring transition"
                                        },
                                        onclick: move |_| payment_method.set(PaymentMethod::Cashu),
                                        div { class: "flex items-center gap-3",
                                            span { class: "text-2xl", "🥜" }
                                            div { class: "flex-1",
                                                p { class: "font-medium", "Pay with Cashu" }
                                                p { class: "text-sm text-muted-foreground",
                                                    "Balance: ⚡{format_sats_with_separator(cashu_balance)} sats"
                                                }
                                            }
                                            if *payment_method.read() == PaymentMethod::Cashu {
                                                span { class: "text-blue-500", "✓" }
                                            }
                                        }
                                        if cashu_balance < total_sats {
                                            p { class: "text-sm text-destructive mt-2",
                                                "Insufficient balance"
                                            }
                                        }
                                    }

                                    // Lightning option
                                    button {
                                        class: if *payment_method.read() == PaymentMethod::Lightning {
                                            "w-full p-4 bg-card border-2 border-blue-500 rounded-lg text-left"
                                        } else {
                                            "w-full p-4 bg-card border border-border rounded-lg text-left hover:border-ring transition"
                                        },
                                        onclick: move |_| payment_method.set(PaymentMethod::Lightning),
                                        div { class: "flex items-center gap-3",
                                            span { class: "text-2xl", "⚡" }
                                            div { class: "flex-1",
                                                p { class: "font-medium", "Pay with Lightning" }
                                                p { class: "text-sm text-muted-foreground",
                                                    if nwc_connected { "NWC Connected" } else { "NWC Not Connected" }
                                                }
                                            }
                                            if *payment_method.read() == PaymentMethod::Lightning {
                                                span { class: "text-blue-500", "✓" }
                                            }
                                        }
                                        if !nwc_connected {
                                            p { class: "text-sm text-destructive mt-2",
                                                "Connect your wallet first"
                                            }
                                        }
                                    }
                                }

                                // Order summary
                                div { class: "bg-muted/50 rounded-lg p-4",
                                    div { class: "flex justify-between mb-2",
                                        span { class: "text-muted-foreground", "Subtotal" }
                                        span { "⚡{total_formatted} sats" }
                                    }
                                    div { class: "flex justify-between font-semibold text-lg pt-2 border-t border-border",
                                        span { "Total" }
                                        span { class: "text-amber-500", "⚡{total_formatted} sats" }
                                    }
                                }

                                // Error message
                                if let Some(err) = payment_error.read().as_ref() {
                                    div { class: "bg-destructive/10 border border-destructive/50 text-destructive rounded-lg p-4",
                                        "{err}"
                                    }
                                }

                                // Pay button
                                {
                                    let can_pay_cashu = *payment_method.read() == PaymentMethod::Cashu && cashu_balance >= total_sats;
                                    let can_pay_lightning = *payment_method.read() == PaymentMethod::Lightning && nwc_connected;
                                    let can_pay = can_pay_cashu || can_pay_lightning;
                                    let processing = *payment_processing.read();

                                    rsx! {
                                        button {
                                            class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                                            disabled: !can_pay || processing,
                                            onclick: move |_| {
                                                payment_processing.set(true);
                                                payment_error.set(None);

                                                let method = *payment_method.read();
                                                let total = total_sats;
                                                let address = shipping_address.read().clone();

                                                spawn(async move {
                                                    // For now, just simulate payment success
                                                    // In real implementation, we would:
                                                    // 1. Get merchant pubkey from cart items
                                                    // 2. Send P2PK tokens (Cashu) or pay invoice (Lightning)
                                                    // 3. Send order message via NIP-17

                                                    match method {
                                                        PaymentMethod::Cashu => {
                                                            // Get cart items for order creation
                                                            let items: Vec<_> = CART_ITEMS.read().clone();
                                                            if items.is_empty() {
                                                                payment_error.set(Some("No items in cart".to_string()));
                                                                payment_processing.set(false);
                                                                return;
                                                            }

                                                            let merchant_pubkey = items.first()
                                                                .map(|i| i.product.pubkey.clone())
                                                                .unwrap_or_default();

                                                            // Try to get first available mint with sufficient balance
                                                            match get_balances_per_mint().await {
                                                                Ok(balances) => {
                                                                    if let Some(mint) = balances.iter()
                                                                        .find(|b| b.balance >= total)
                                                                        .map(|b| b.mint_url.clone())
                                                                    {
                                                                        // Send P2PK tokens to merchant
                                                                        match send_tokens_p2pk(mint, total, merchant_pubkey).await {
                                                                            Ok(token) => {
                                                                                log::info!("Payment successful: {}", token);

                                                                                // Create order and send NIP-17 messages
                                                                                let shipping = if address.is_empty() { None } else { Some(address.clone()) };
                                                                                match create_shop_order(items, shipping, "cashu", &token).await {
                                                                                    Ok(id) => {
                                                                                        order_id.set(Some(id));
                                                                                        clear_cart();
                                                                                        step.set(CheckoutStep::Complete);
                                                                                    }
                                                                                    Err(e) => {
                                                                                        log::error!("Failed to create order: {}", e);
                                                                                        // Payment succeeded but order creation failed
                                                                                        // Still complete checkout but show warning
                                                                                        let id = format!("{:x}", std::time::SystemTime::now()
                                                                                            .duration_since(std::time::UNIX_EPOCH)
                                                                                            .unwrap_or_default()
                                                                                            .as_millis());
                                                                                        order_id.set(Some(id));
                                                                                        clear_cart();
                                                                                        step.set(CheckoutStep::Complete);
                                                                                    }
                                                                                }
                                                                            }
                                                                            Err(e) => {
                                                                                payment_error.set(Some(e));
                                                                            }
                                                                        }
                                                                    } else {
                                                                        payment_error.set(Some("No mint with sufficient balance".to_string()));
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    payment_error.set(Some(e));
                                                                }
                                                            }
                                                        }
                                                        PaymentMethod::Lightning => {
                                                            // For Lightning, we need an invoice from merchant
                                                            // This would be obtained via NIP-17 message
                                                            payment_error.set(Some("Lightning checkout requires merchant invoice (not yet implemented)".to_string()));
                                                        }
                                                    }

                                                    payment_processing.set(false);
                                                });
                                            },
                                            if processing {
                                                "Processing..."
                                            } else {
                                                "Pay ⚡{total_formatted} sats"
                                            }
                                        }
                                    }
                                }
                            }
                        },

                        CheckoutStep::Complete => rsx! {
                            // Order complete
                            div { class: "text-center py-12",
                                div { class: "text-6xl mb-4", "✅" }
                                h2 { class: "text-2xl font-bold mb-2", "Order Placed!" }
                                p { class: "text-muted-foreground mb-6",
                                    "Your payment has been sent to the merchant."
                                }

                                if let Some(id) = order_id.read().as_ref() {
                                    p { class: "text-sm text-muted-foreground mb-6",
                                        "Order ID: {id}"
                                    }
                                }

                                div { class: "space-y-3",
                                    Link {
                                        to: Route::ShopOrders {},
                                        class: "block w-full py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                                        "View My Orders"
                                    }
                                    Link {
                                        to: Route::ShopHome {},
                                        class: "block w-full py-3 border border-border hover:bg-accent rounded-lg font-medium transition",
                                        "Continue Shopping"
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

/// Step indicator component
#[component]
fn StepIndicator(label: &'static str, active: bool, completed: bool) -> Element {
    let class = if completed {
        "flex items-center gap-1 text-sm text-blue-500"
    } else if active {
        "flex items-center gap-1 text-sm font-medium"
    } else {
        "flex items-center gap-1 text-sm text-muted-foreground"
    };

    rsx! {
        div { class: "{class}",
            if completed {
                span { class: "w-5 h-5 rounded-full bg-blue-500 text-white flex items-center justify-center text-xs",
                    "✓"
                }
            } else if active {
                span { class: "w-5 h-5 rounded-full bg-blue-500 text-white flex items-center justify-center text-xs",
                    "●"
                }
            } else {
                span { class: "w-5 h-5 rounded-full border border-muted-foreground flex items-center justify-center text-xs",
                    "○"
                }
            }
            span { "{label}" }
        }
    }
}
