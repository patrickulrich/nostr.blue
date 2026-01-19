//! Shop Checkout - Checkout flow with payment

use dioxus::prelude::*;
use std::collections::HashMap;
use crate::routes::Route;
use crate::stores::shop_store::{CART_ITEMS, CART_TOTAL_SATS, clear_cart, get_cart_count, create_shop_order, fetch_shipping_options, set_cart_item_shipping};
use crate::utils::nip99::{CartItem, ShippingOption};
use crate::stores::cashu::{WALLET_BALANCE, send_tokens_p2pk, get_balances_per_mint};
use crate::stores::nwc_store;
use crate::stores::profiles;
use crate::services::lnurl;
use crate::utils::format::{format_sats_with_separator, truncate_pubkey};

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

/// Per-merchant payment tracking for multi-merchant Lightning
#[derive(Clone, PartialEq)]
struct MerchantPayment {
    pubkey: String,
    name: String,
    lud16: String,
    amount_sats: u64,
    items: Vec<CartItem>,
    invoice: Option<String>,
    paid: bool,
    preimage: Option<String>,
}

/// Group cart items by merchant and calculate subtotals
fn group_items_by_merchant(items: &[CartItem]) -> Vec<(String, Vec<CartItem>, u64)> {
    let mut groups: HashMap<String, (Vec<CartItem>, u64)> = HashMap::new();

    for item in items {
        let pubkey = item.product.pubkey.clone();
        let item_sats = if item.product.price.currency.eq_ignore_ascii_case("sats") {
            item.product.price.amount as u64 * item.quantity as u64
        } else {
            0 // Non-sats pricing not fully supported
        };

        let entry = groups
            .entry(pubkey)
            .or_insert_with(|| (Vec::new(), 0));
        entry.0.push(item.clone());
        entry.1 += item_sats;
    }

    groups.into_iter()
        .map(|(pk, (items, total))| (pk, items, total))
        .collect()
}

/// Checkout page
#[component]
pub fn ShopCheckout() -> Element {
    let mut step = use_signal(|| CheckoutStep::Review);
    let mut payment_method = use_signal(|| PaymentMethod::Lightning);
    let mut shipping_address = use_signal(String::new);
    let mut payment_processing = use_signal(|| false);
    let mut payment_error = use_signal(|| None::<String>);
    let mut order_id = use_signal(|| None::<String>);
    let mut order_warning = use_signal(|| None::<String>);

    // Lightning payment state
    let mut lightning_invoice = use_signal(|| None::<String>);
    let mut show_invoice_qr = use_signal(|| false);
    let mut merchant_lud16 = use_signal(|| None::<String>);
    let mut all_merchants_have_lightning = use_signal(|| false);
    let mut merchants_without_lightning = use_signal(Vec::<String>::new);

    // Multi-merchant Lightning payment state
    let mut merchant_payments = use_signal(Vec::<MerchantPayment>::new);
    let mut current_merchant_idx = use_signal(|| 0usize);
    let mut multi_merchant_mode = use_signal(|| false);

    let cart_items = CART_ITEMS.read();
    let total_sats = *CART_TOTAL_SATS.read();
    let item_count = get_cart_count();
    let cashu_balance = *WALLET_BALANCE.read();
    let nwc_connected = nwc_store::is_connected();

    // Get all unique merchant pubkeys from cart
    let merchant_pubkeys: Vec<String> = {
        let mut pks: Vec<String> = cart_items.iter()
            .map(|i| i.product.pubkey.clone())
            .collect();
        pks.sort();
        pks.dedup();
        pks
    };

    // Note: merchant pubkeys are now handled via merchant_payments for both single and multi-merchant

    // Fetch ALL merchant profiles to check for lud16 and build payment list
    let pks_for_effect = merchant_pubkeys.clone();
    let cart_items_for_effect: Vec<CartItem> = cart_items.clone();
    use_effect(move || {
        let pks = pks_for_effect.clone();
        let items = cart_items_for_effect.clone();
        if pks.is_empty() {
            return;
        }

        // Group items by merchant
        let grouped = group_items_by_merchant(&items);
        let is_multi = grouped.len() > 1;

        spawn(async move {
            let mut all_have_lud16 = true;
            let mut missing_lightning: Vec<String> = Vec::new();
            let mut first_lud16: Option<String> = None;
            let mut payments: Vec<MerchantPayment> = Vec::new();

            for (pk, merchant_items, amount) in grouped {
                let profile_result = profiles::fetch_profile(pk.clone()).await;

                if let Ok(profile) = profile_result {
                    let name = profile.name.clone()
                        .unwrap_or_else(|| truncate_pubkey(&pk));

                    if let Some(ref lud16) = profile.lud16 {
                        if first_lud16.is_none() {
                            first_lud16 = Some(lud16.clone());
                        }

                        // Add to multi-merchant payment list
                        payments.push(MerchantPayment {
                            pubkey: pk.clone(),
                            name: name.clone(),
                            lud16: lud16.clone(),
                            amount_sats: amount,
                            items: merchant_items,
                            invoice: None,
                            paid: false,
                            preimage: None,
                        });
                    } else {
                        all_have_lud16 = false;
                        missing_lightning.push(name);
                    }
                } else {
                    all_have_lud16 = false;
                    missing_lightning.push(truncate_pubkey(&pk));
                }
            }

            all_merchants_have_lightning.set(all_have_lud16);
            merchants_without_lightning.set(missing_lightning);
            merchant_payments.set(payments);
            multi_merchant_mode.set(is_multi);

            if let Some(lud16) = first_lud16 {
                merchant_lud16.set(Some(lud16));
            }
        });
    });

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
                                            div { class: "w-16 h-16 bg-muted rounded shrink-0 overflow-hidden",
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
                            // Shipping step with address and shipping options
                            ShippingStep {
                                cart_items: cart_items.clone(),
                                shipping_address,
                                on_address_change: move |addr: String| shipping_address.set(addr),
                                on_continue: move |_| step.set(CheckoutStep::Payment)
                            }
                        },

                        CheckoutStep::Payment => rsx! {
                            // Payment method selection
                            div { class: "space-y-6",
                                h2 { class: "text-lg font-semibold", "Select Payment Method" }

                                // Payment options
                                div { class: "space-y-3",
                                    // Note: Cashu payment option is disabled for marketplace until
                                    // multi-merchant payment splitting is properly implemented.
                                    // See: https://github.com/patrickulrich/nostr.blue/pull/108

                                    // Lightning option
                                    {
                                        let lightning_available = *all_merchants_have_lightning.read();
                                        let merchants_missing = merchants_without_lightning.read().clone();
                                        let is_selected = *payment_method.read() == PaymentMethod::Lightning;

                                        rsx! {
                                            button {
                                                class: if is_selected {
                                                    "w-full p-4 bg-card border-2 border-blue-500 rounded-lg text-left"
                                                } else if lightning_available {
                                                    "w-full p-4 bg-card border border-border rounded-lg text-left hover:border-ring transition"
                                                } else {
                                                    "w-full p-4 bg-card border border-border rounded-lg text-left opacity-60 cursor-not-allowed"
                                                },
                                                disabled: !lightning_available,
                                                onclick: move |_| {
                                                    if lightning_available {
                                                        payment_method.set(PaymentMethod::Lightning);
                                                    }
                                                },
                                                div { class: "flex items-center gap-3",
                                                    span { class: "text-2xl", "⚡" }
                                                    div { class: "flex-1",
                                                        p { class: "font-medium",
                                                            "Pay with Lightning"
                                                            if !lightning_available && !merchants_missing.is_empty() {
                                                                span { class: "text-xs text-destructive ml-2",
                                                                    "(Not available)"
                                                                }
                                                            }
                                                        }
                                                        if !lightning_available && !merchants_missing.is_empty() {
                                                            p { class: "text-sm text-amber-600",
                                                                "Missing Lightning: {merchants_missing.join(\", \")}"
                                                            }
                                                        } else {
                                                            p { class: "text-sm text-muted-foreground",
                                                                if nwc_connected {
                                                                    "NWC connected - auto-pay enabled"
                                                                } else {
                                                                    "Scan QR code to pay"
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if is_selected {
                                                        span { class: "text-blue-500", "✓" }
                                                    }
                                                }
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

                                // Multi-merchant breakdown (when Lightning selected and multiple merchants)
                                {
                                    let is_multi = *multi_merchant_mode.read();
                                    let method = *payment_method.read();
                                    let payments = merchant_payments.read().clone();

                                    if is_multi && method == PaymentMethod::Lightning && !payments.is_empty() {
                                        rsx! {
                                            div { class: "bg-amber-500/10 border border-amber-500/30 rounded-lg p-4",
                                                div { class: "flex items-center gap-2 mb-3",
                                                    span { class: "text-amber-500", "⚡" }
                                                    p { class: "font-medium text-sm", "Multiple Merchants" }
                                                }
                                                p { class: "text-xs text-muted-foreground mb-3",
                                                    "You'll pay each merchant separately with Lightning:"
                                                }
                                                div { class: "space-y-2",
                                                    for payment in payments.iter() {
                                                        div {
                                                            key: "{payment.pubkey}",
                                                            class: "flex justify-between text-sm",
                                                            span { class: "text-muted-foreground", "{payment.name}" }
                                                            span { class: "text-amber-500", "⚡{format_sats_with_separator(payment.amount_sats)}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        rsx! {}
                                    }
                                }

                                // Error message
                                if let Some(err) = payment_error.read().as_ref() {
                                    div { class: "bg-destructive/10 border border-destructive/50 text-destructive rounded-lg p-4",
                                        "{err}"
                                    }
                                }

                                // Lightning invoice QR display (multi-merchant or single)
                                if *show_invoice_qr.read() {
                                    {
                                        let is_multi = *multi_merchant_mode.read();
                                        let payments = merchant_payments.read().clone();
                                        let current_idx = *current_merchant_idx.read();

                                        if is_multi && !payments.is_empty() {
                                            // Multi-merchant payment UI
                                            rsx! {
                                                MultiMerchantPaymentDisplay {
                                                    payments: payments.clone(),
                                                    current_idx: current_idx,
                                                    on_payment_confirmed: move |preimage: String| {
                                                        let idx = *current_merchant_idx.read();
                                                        let mut updated = merchant_payments.read().clone();

                                                        if idx < updated.len() {
                                                            updated[idx].paid = true;
                                                            updated[idx].preimage = Some(preimage);
                                                            merchant_payments.set(updated.clone());
                                                        }

                                                        let all_paid = updated.iter().all(|p| p.paid);

                                                        if all_paid {
                                                            // All merchants paid - create order
                                                            let items: Vec<_> = CART_ITEMS.read().clone();
                                                            let address = shipping_address.read().clone();
                                                            let shipping = if address.is_empty() { None } else { Some(address.clone()) };
                                                            let all_preimages: Vec<String> = updated.iter()
                                                                .filter_map(|p| p.preimage.clone())
                                                                .collect();
                                                            let proof = all_preimages.join(",");

                                                            spawn(async move {
                                                                match create_shop_order(items, shipping, None, "lightning", &proof).await {
                                                                    Ok(id) => {
                                                                        order_id.set(Some(id));
                                                                        clear_cart();
                                                                        show_invoice_qr.set(false);
                                                                        lightning_invoice.set(None);
                                                                        merchant_payments.set(Vec::new());
                                                                        step.set(CheckoutStep::Complete);
                                                                    }
                                                                    Err(e) => {
                                                                        log::error!("Failed to create order: {}", e);
                                                                        let id = crate::utils::format::generate_unique_id();
                                                                        order_id.set(Some(id));
                                                                        order_warning.set(Some(format!(
                                                                            "All payments confirmed but order notification failed: {}",
                                                                            e
                                                                        )));
                                                                        clear_cart();
                                                                        show_invoice_qr.set(false);
                                                                        step.set(CheckoutStep::Complete);
                                                                    }
                                                                }
                                                                payment_processing.set(false);
                                                            });
                                                        } else {
                                                            // Move to next merchant
                                                            let next_idx = idx + 1;
                                                            if next_idx < updated.len() {
                                                                current_merchant_idx.set(next_idx);

                                                                // Get invoice for next merchant
                                                                let next = updated[next_idx].clone();
                                                                spawn(async move {
                                                                    match lnurl::get_invoice_from_lud16(&next.lud16, next.amount_sats, Some(&format!("Order: {}", next.name))).await {
                                                                        Ok(invoice) => {
                                                                            let mut upd = merchant_payments.read().clone();
                                                                            upd[next_idx].invoice = Some(invoice.clone());
                                                                            merchant_payments.set(upd);
                                                                            lightning_invoice.set(Some(invoice));
                                                                        }
                                                                        Err(e) => {
                                                                            payment_error.set(Some(format!("Failed to get invoice from {}: {}", next.name, e)));
                                                                            show_invoice_qr.set(false);
                                                                            payment_processing.set(false);
                                                                        }
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    },
                                                    on_cancel: move |_| {
                                                        show_invoice_qr.set(false);
                                                        lightning_invoice.set(None);
                                                        merchant_payments.set(Vec::new());
                                                        current_merchant_idx.set(0);
                                                        payment_processing.set(false);
                                                    }
                                                }
                                            }
                                        } else if let Some(invoice) = lightning_invoice.read().as_ref() {
                                            // Single merchant payment UI (original)
                                            rsx! {
                                                LightningInvoiceDisplay {
                                                    invoice: invoice.clone(),
                                                    amount_sats: total_sats,
                                                    on_paid: move |preimage: String| {
                                                        let items: Vec<_> = CART_ITEMS.read().clone();
                                                        let address = shipping_address.read().clone();
                                                        let shipping = if address.is_empty() { None } else { Some(address.clone()) };

                                                        spawn(async move {
                                                            match create_shop_order(items, shipping, None, "lightning", &preimage).await {
                                                                Ok(id) => {
                                                                    order_id.set(Some(id));
                                                                    clear_cart();
                                                                    show_invoice_qr.set(false);
                                                                    lightning_invoice.set(None);
                                                                    step.set(CheckoutStep::Complete);
                                                                }
                                                                Err(e) => {
                                                                    log::error!("Failed to create order: {}", e);
                                                                    let id = crate::utils::format::generate_unique_id();
                                                                    order_id.set(Some(id));
                                                                    order_warning.set(Some(format!(
                                                                        "Payment confirmed but order notification failed: {}. \
                                                                        Please contact merchant directly with your Order ID.",
                                                                        e
                                                                    )));
                                                                    clear_cart();
                                                                    show_invoice_qr.set(false);
                                                                    lightning_invoice.set(None);
                                                                    step.set(CheckoutStep::Complete);
                                                                }
                                                            }
                                                            payment_processing.set(false);
                                                        });
                                                    },
                                                    on_cancel: move |_| {
                                                        show_invoice_qr.set(false);
                                                        lightning_invoice.set(None);
                                                        payment_processing.set(false);
                                                    }
                                                }
                                            }
                                        } else {
                                            rsx! {}
                                        }
                                    }
                                }

                                // Pay button
                                if !*show_invoice_qr.read() {
                                    {
                                        let method = *payment_method.read();
                                        let lightning_ready = *all_merchants_have_lightning.read();
                                        let can_pay_cashu = method == PaymentMethod::Cashu && cashu_balance >= total_sats;
                                        let can_pay_lightning = method == PaymentMethod::Lightning && lightning_ready;
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
                                                    let items: Vec<_> = CART_ITEMS.read().clone();
                                                    let lud16 = merchant_lud16.read().clone();

                                                    spawn(async move {
                                                        match method {
                                                            PaymentMethod::Cashu => {
                                                                if items.is_empty() {
                                                                    payment_error.set(Some("No items in cart".to_string()));
                                                                    payment_processing.set(false);
                                                                    return;
                                                                }

                                                                let merchant_pubkey = items.first()
                                                                    .map(|i| i.product.pubkey.clone())
                                                                    .unwrap_or_default();

                                                                match get_balances_per_mint().await {
                                                                    Ok(balances) => {
                                                                        if let Some(mint) = balances.iter()
                                                                            .find(|b| b.balance >= total)
                                                                            .map(|b| b.mint_url.clone())
                                                                        {
                                                                            match send_tokens_p2pk(mint, total, merchant_pubkey).await {
                                                                                Ok(token) => {
                                                                                    log::info!("Payment successful: {}", token);
                                                                                    let shipping = if address.is_empty() { None } else { Some(address.clone()) };
                                                                                    match create_shop_order(items, shipping, None, "cashu", &token).await {
                                                                                        Ok(id) => {
                                                                                            order_id.set(Some(id));
                                                                                            clear_cart();
                                                                                            step.set(CheckoutStep::Complete);
                                                                                        }
                                                                                        Err(e) => {
                                                                                            log::error!("Failed to create order: {}", e);
                                                                                            let id = crate::utils::format::generate_unique_id();
                                                                                            order_id.set(Some(id));
                                                                                            order_warning.set(Some(format!(
                                                                                                "Payment sent but order notification failed: {}. \
                                                                                                Please contact merchant directly with your Order ID.",
                                                                                                e
                                                                                            )));
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
                                                                payment_processing.set(false);
                                                            }
                                                            PaymentMethod::Lightning => {
                                                                let payments = merchant_payments.read().clone();
                                                                let is_multi = *multi_merchant_mode.read();

                                                                if payments.is_empty() {
                                                                    payment_error.set(Some("No merchants with Lightning addresses".to_string()));
                                                                    payment_processing.set(false);
                                                                    return;
                                                                }

                                                                // Multi-merchant mode: pay each merchant sequentially
                                                                if is_multi {
                                                                    log::info!("Multi-merchant mode: {} merchants to pay", payments.len());
                                                                    current_merchant_idx.set(0);

                                                                    // Start with first merchant invoice
                                                                    let first = &payments[0];
                                                                    log::info!("Getting invoice from {} for {} sats", first.name, first.amount_sats);

                                                                    match lnurl::get_invoice_from_lud16(&first.lud16, first.amount_sats, Some(&format!("Order: {}", first.name))).await {
                                                                        Ok(invoice) => {
                                                                            // Update the merchant payment with invoice
                                                                            let mut updated = payments.clone();
                                                                            updated[0].invoice = Some(invoice.clone());
                                                                            merchant_payments.set(updated);

                                                                            // If NWC connected, try auto-pay
                                                                            if nwc_store::is_connected() {
                                                                                match nwc_store::pay_invoice(invoice.clone()).await {
                                                                                    Ok(response) => {
                                                                                        log::info!("Paid merchant 1/{}", payments.len());
                                                                                        let mut updated = merchant_payments.read().clone();
                                                                                        updated[0].paid = true;
                                                                                        updated[0].preimage = Some(response.preimage);
                                                                                        merchant_payments.set(updated.clone());

                                                                                        // Check if more merchants to pay
                                                                                        if payments.len() > 1 {
                                                                                            current_merchant_idx.set(1);
                                                                                            // Fetch invoice for next merchant and try auto-pay
                                                                                            let next = updated[1].clone();
                                                                                            spawn(async move {
                                                                                                match lnurl::get_invoice_from_lud16(&next.lud16, next.amount_sats, Some(&format!("Order: {}", next.name))).await {
                                                                                                    Ok(invoice) => {
                                                                                                        let mut upd = merchant_payments.read().clone();
                                                                                                        upd[1].invoice = Some(invoice.clone());
                                                                                                        merchant_payments.set(upd);
                                                                                                        lightning_invoice.set(Some(invoice.clone()));

                                                                                                        // Try NWC auto-pay for second merchant
                                                                                                        if nwc_store::is_connected() {
                                                                                                            match nwc_store::pay_invoice(invoice).await {
                                                                                                                Ok(resp) => {
                                                                                                                    log::info!("Paid merchant 2/{} via NWC", payments.len());
                                                                                                                    let mut upd = merchant_payments.read().clone();
                                                                                                                    upd[1].paid = true;
                                                                                                                    upd[1].preimage = Some(resp.preimage.clone());
                                                                                                                    merchant_payments.set(upd.clone());

                                                                                                                    // Check if all paid
                                                                                                                    let all_paid = upd.iter().all(|p| p.paid);
                                                                                                                    if all_paid {
                                                                                                                        // All merchants paid - create order
                                                                                                                        let items: Vec<_> = CART_ITEMS.read().clone();
                                                                                                                        let address = shipping_address.read().clone();
                                                                                                                        let shipping = if address.is_empty() { None } else { Some(address.clone()) };
                                                                                                                        let all_preimages: Vec<String> = upd.iter()
                                                                                                                            .filter_map(|p| p.preimage.clone())
                                                                                                                            .collect();
                                                                                                                        let proof = all_preimages.join(",");

                                                                                                                        match create_shop_order(items, shipping, None, "lightning", &proof).await {
                                                                                                                            Ok(id) => {
                                                                                                                                order_id.set(Some(id));
                                                                                                                                clear_cart();
                                                                                                                                show_invoice_qr.set(false);
                                                                                                                                lightning_invoice.set(None);
                                                                                                                                merchant_payments.set(Vec::new());
                                                                                                                                step.set(CheckoutStep::Complete);
                                                                                                                            }
                                                                                                                            Err(e) => {
                                                                                                                                log::error!("Failed to create order: {}", e);
                                                                                                                                let id = crate::utils::format::generate_unique_id();
                                                                                                                                order_id.set(Some(id));
                                                                                                                                order_warning.set(Some(format!(
                                                                                                                                    "All payments confirmed but order notification failed: {}",
                                                                                                                                    e
                                                                                                                                )));
                                                                                                                                clear_cart();
                                                                                                                                show_invoice_qr.set(false);
                                                                                                                                step.set(CheckoutStep::Complete);
                                                                                                                            }
                                                                                                                        }
                                                                                                                        payment_processing.set(false);
                                                                                                                    } else {
                                                                                                                        // More merchants - continue with next
                                                                                                                        current_merchant_idx.set(2);
                                                                                                                        show_invoice_qr.set(true);
                                                                                                                    }
                                                                                                                }
                                                                                                                Err(e) => {
                                                                                                                    log::warn!("NWC auto-pay failed for merchant 2: {}, showing QR", e);
                                                                                                                    show_invoice_qr.set(true);
                                                                                                                }
                                                                                                            }
                                                                                                        } else {
                                                                                                            // No NWC, show QR for manual payment
                                                                                                            show_invoice_qr.set(true);
                                                                                                        }
                                                                                                    }
                                                                                                    Err(e) => {
                                                                                                        payment_error.set(Some(format!("Failed to get invoice from {}: {}", next.name, e)));
                                                                                                        show_invoice_qr.set(false);
                                                                                                        payment_processing.set(false);
                                                                                                    }
                                                                                                }
                                                                                            });
                                                                                        } else {
                                                                                            // Single merchant paid, complete order
                                                                                            let all_preimages: Vec<String> = merchant_payments.read()
                                                                                                .iter()
                                                                                                .filter_map(|p| p.preimage.clone())
                                                                                                .collect();
                                                                                            let proof = all_preimages.join(",");
                                                                                            let shipping = if address.is_empty() { None } else { Some(address.clone()) };
                                                                                            match create_shop_order(items, shipping, None, "lightning", &proof).await {
                                                                                                Ok(id) => {
                                                                                                    order_id.set(Some(id));
                                                                                                    clear_cart();
                                                                                                    step.set(CheckoutStep::Complete);
                                                                                                }
                                                                                                Err(e) => {
                                                                                                    log::error!("Failed to create order: {}", e);
                                                                                                    let id = crate::utils::format::generate_unique_id();
                                                                                                    order_id.set(Some(id));
                                                                                                    order_warning.set(Some(format!(
                                                                                                        "Payment confirmed but order notification failed: {}",
                                                                                                        e
                                                                                                    )));
                                                                                                    clear_cart();
                                                                                                    step.set(CheckoutStep::Complete);
                                                                                                }
                                                                                            }
                                                                                            payment_processing.set(false);
                                                                                        }
                                                                                    }
                                                                                    Err(e) => {
                                                                                        log::warn!("NWC auto-pay failed: {}, showing QR", e);
                                                                                        lightning_invoice.set(Some(invoice));
                                                                                        show_invoice_qr.set(true);
                                                                                    }
                                                                                }
                                                                            } else {
                                                                                // No NWC, show QR for manual payment
                                                                                lightning_invoice.set(Some(invoice));
                                                                                show_invoice_qr.set(true);
                                                                            }
                                                                        }
                                                                        Err(e) => {
                                                                            payment_error.set(Some(format!("Failed to get invoice from {}: {}", first.name, e)));
                                                                            payment_processing.set(false);
                                                                        }
                                                                    }
                                                                } else {
                                                                    // Single merchant mode (original flow)
                                                                    let Some(lud16_addr) = lud16 else {
                                                                        payment_error.set(Some("Merchant has no Lightning address".to_string()));
                                                                        payment_processing.set(false);
                                                                        return;
                                                                    };

                                                                    log::info!("Requesting invoice from {}", lud16_addr);
                                                                    match lnurl::get_invoice_from_lud16(&lud16_addr, total, Some("Shop order")).await {
                                                                        Ok(invoice) => {
                                                                            log::info!("Got invoice: {}", invoice);

                                                                            if nwc_store::is_connected() {
                                                                                log::info!("NWC connected, attempting auto-pay...");
                                                                                match nwc_store::pay_invoice(invoice.clone()).await {
                                                                                    Ok(response) => {
                                                                                        log::info!("Payment successful!");
                                                                                        let preimage = response.preimage;
                                                                                        let shipping = if address.is_empty() { None } else { Some(address.clone()) };
                                                                                        match create_shop_order(items, shipping, None, "lightning", &preimage).await {
                                                                                            Ok(id) => {
                                                                                                order_id.set(Some(id));
                                                                                                clear_cart();
                                                                                                step.set(CheckoutStep::Complete);
                                                                                            }
                                                                                            Err(e) => {
                                                                                                log::error!("Failed to create order: {}", e);
                                                                                                let id = crate::utils::format::generate_unique_id();
                                                                                                order_id.set(Some(id));
                                                                                                order_warning.set(Some(format!(
                                                                                                    "Payment confirmed but order notification failed: {}. \
                                                                                                    Please contact merchant directly with your Order ID.",
                                                                                                    e
                                                                                                )));
                                                                                                clear_cart();
                                                                                                step.set(CheckoutStep::Complete);
                                                                                            }
                                                                                        }
                                                                                        payment_processing.set(false);
                                                                                    }
                                                                                    Err(e) => {
                                                                                        log::warn!("NWC auto-pay failed: {}, showing QR", e);
                                                                                        lightning_invoice.set(Some(invoice));
                                                                                        show_invoice_qr.set(true);
                                                                                    }
                                                                                }
                                                                            } else {
                                                                                lightning_invoice.set(Some(invoice));
                                                                                show_invoice_qr.set(true);
                                                                            }
                                                                        }
                                                                        Err(e) => {
                                                                            payment_error.set(Some(format!("Failed to get invoice: {}", e)));
                                                                            payment_processing.set(false);
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
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
                            }
                        },

                        CheckoutStep::Complete => rsx! {
                            // Order complete
                            div { class: "text-center py-12",
                                div { class: "text-6xl mb-4",
                                    if order_warning.read().is_some() { "⚠️" } else { "✅" }
                                }
                                h2 { class: "text-2xl font-bold mb-2",
                                    if order_warning.read().is_some() {
                                        "Payment Sent"
                                    } else {
                                        "Order Placed!"
                                    }
                                }
                                p { class: "text-muted-foreground mb-6",
                                    "Your payment has been sent to the merchant."
                                }

                                // Warning message if order creation failed
                                if let Some(warning) = order_warning.read().as_ref() {
                                    div { class: "bg-amber-500/10 border border-amber-500/50 text-amber-600 dark:text-amber-400 rounded-lg p-4 mb-6 text-left text-sm",
                                        "{warning}"
                                    }
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

/// Lightning invoice display component with QR code
#[component]
fn LightningInvoiceDisplay(
    invoice: String,
    amount_sats: u64,
    on_paid: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut copied = use_signal(|| false);
    let mut waiting_for_payment = use_signal(|| false);

    // Sanitize invoice to prevent XSS attacks in the QR code script
    let sanitized_invoice = crate::utils::validation::sanitize_lightning_invoice(&invoice);

    // Generate QR code data URL only if invoice is valid
    let qr_data = sanitized_invoice
        .as_ref()
        .map(|inv| format!("lightning:{}", inv))
        .unwrap_or_default();

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-6 space-y-4",
            // Header
            div { class: "text-center",
                h3 { class: "text-lg font-semibold mb-2", "Pay Lightning Invoice" }
                p { class: "text-muted-foreground text-sm",
                    "Scan QR code or copy invoice to your Lightning wallet"
                }
            }

            // QR Code
            div { class: "flex justify-center py-4",
                div {
                    class: "bg-white p-4 rounded-lg",
                    div {
                        class: "w-48 h-48 flex items-center justify-center border-2 border-dashed border-gray-300 rounded",
                        id: "qr-container",
                    }
                    script {
                        dangerous_inner_html: format!(r#"
                            (function() {{
                                const container = document.getElementById('qr-container');
                                if (container && typeof QRCode !== 'undefined') {{
                                    container.innerHTML = '';
                                    new QRCode(container, {{
                                        text: '{qr_data}',
                                        width: 180,
                                        height: 180,
                                        colorDark: '#000000',
                                        colorLight: '#ffffff',
                                        correctLevel: QRCode.CorrectLevel.M
                                    }});
                                }} else {{
                                    container.innerHTML = '<div class="text-xs text-center text-gray-500 p-4">QR Code<br/>(scan in wallet)</div>';
                                }}
                            }})();
                        "#)
                    }
                }
            }

            // Amount display
            div { class: "text-center",
                p { class: "text-2xl font-bold text-amber-500",
                    "⚡{crate::utils::format::format_sats_with_separator(amount_sats)} sats"
                }
            }

            // Invoice text (truncated)
            div { class: "bg-muted/50 rounded-lg p-3",
                p { class: "text-xs font-mono break-all text-muted-foreground",
                    {
                        let display_invoice = if invoice.len() > 60 {
                            format!("{}...{}", &invoice[..30], &invoice[invoice.len()-20..])
                        } else {
                            invoice.clone()
                        };
                        display_invoice
                    }
                }
            }

            // Copy button
            {
                let invoice_for_copy = invoice.clone();
                rsx! {
                    button {
                        class: "w-full py-3 border border-border hover:bg-accent rounded-lg font-medium transition flex items-center justify-center gap-2",
                        onclick: move |_| {
                            let inv = invoice_for_copy.clone();
                            spawn(async move {
                                if crate::utils::clipboard::copy_to_clipboard(&inv).await.is_ok() {
                                    copied.set(true);
                                    gloo_timers::future::TimeoutFuture::new(2000).await;
                                    copied.set(false);
                                }
                            });
                        },
                        if *copied.read() {
                            "✓ Copied!"
                        } else {
                            "📋 Copy Invoice"
                        }
                    }
                }
            }

            // Waiting state or confirmation button
            if *waiting_for_payment.read() {
                div { class: "text-center py-4",
                    div { class: "flex items-center justify-center gap-2 text-blue-500",
                        div { class: "animate-spin w-5 h-5 border-2 border-blue-500 border-t-transparent rounded-full" }
                        span { "Waiting for payment confirmation..." }
                    }
                    p { class: "text-xs text-muted-foreground mt-2",
                        "If you've completed the payment, click below to continue"
                    }
                }
            }

            // Manual confirmation button
            button {
                class: "w-full py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                onclick: move |_| {
                    waiting_for_payment.set(true);
                    // Brief delay for UX, then complete
                    spawn(async move {
                        gloo_timers::future::TimeoutFuture::new(1500).await;
                        on_paid.call("manual_payment".to_string());
                    });
                },
                if *waiting_for_payment.read() {
                    "Completing Order..."
                } else {
                    "I've Paid - Complete Order"
                }
            }

            // Cancel button
            button {
                class: "w-full py-2 text-muted-foreground hover:text-foreground text-sm transition",
                onclick: move |_| on_cancel.call(()),
                "Cancel"
            }

            // Verification info
            div { class: "bg-blue-500/10 border border-blue-500/30 rounded-lg p-3 text-sm",
                div { class: "flex items-start gap-2",
                    span { class: "text-blue-500 shrink-0", "ℹ️" }
                    div {
                        p { class: "text-blue-600 dark:text-blue-400",
                            "After payment, the merchant will receive your order and verify the payment before fulfilling it."
                        }
                    }
                }
            }
        }
    }
}

/// Multi-merchant payment display component
#[component]
fn MultiMerchantPaymentDisplay(
    payments: Vec<MerchantPayment>,
    current_idx: usize,
    on_payment_confirmed: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut copied = use_signal(|| false);

    let current = payments.get(current_idx);
    let paid_count = payments.iter().filter(|p| p.paid).count();
    let total_count = payments.len();

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-6 space-y-4",
            // Header with progress
            div { class: "text-center",
                h3 { class: "text-lg font-semibold mb-2",
                    "Pay Merchants ({paid_count}/{total_count})"
                }
                p { class: "text-muted-foreground text-sm",
                    "Your order includes items from multiple sellers"
                }
            }

            // Progress indicators
            div { class: "flex justify-center gap-2 py-2",
                for (idx, payment) in payments.iter().enumerate() {
                    div {
                        key: "{payment.pubkey}",
                        class: if payment.paid {
                            "w-8 h-8 rounded-full bg-green-500 text-white flex items-center justify-center text-sm"
                        } else if idx == current_idx {
                            "w-8 h-8 rounded-full bg-blue-500 text-white flex items-center justify-center text-sm animate-pulse"
                        } else {
                            "w-8 h-8 rounded-full bg-muted text-muted-foreground flex items-center justify-center text-sm"
                        },
                        if payment.paid { "✓" } else { "{idx + 1}" }
                    }
                }
            }

            // Merchant list with status
            div { class: "space-y-2 py-2",
                for (idx, payment) in payments.iter().enumerate() {
                    div {
                        key: "{payment.pubkey}-list",
                        class: if idx == current_idx && !payment.paid {
                            "flex items-center justify-between p-3 bg-blue-500/10 border border-blue-500/30 rounded-lg"
                        } else if payment.paid {
                            "flex items-center justify-between p-3 bg-green-500/10 border border-green-500/30 rounded-lg"
                        } else {
                            "flex items-center justify-between p-3 bg-muted/50 rounded-lg opacity-60"
                        },
                        div { class: "flex items-center gap-2",
                            if payment.paid {
                                span { class: "text-green-500", "✓" }
                            } else if idx == current_idx {
                                span { class: "text-blue-500", "⚡" }
                            } else {
                                span { class: "text-muted-foreground", "○" }
                            }
                            span { class: "font-medium", "{payment.name}" }
                        }
                        span { class: "text-amber-500 font-medium",
                            "⚡{format_sats_with_separator(payment.amount_sats)}"
                        }
                    }
                }
            }

            // Current invoice QR
            if let Some(payment) = current {
                if !payment.paid {
                    if let Some(ref invoice) = payment.invoice {
                        // QR Code (invoice is sanitized inline via validation function)
                        {
                            // Sanitize invoice to prevent XSS attacks in dangerous_inner_html
                            let qr_invoice = crate::utils::validation::sanitize_lightning_invoice(invoice)
                                .unwrap_or_else(|| "INVALID".to_string());
                            rsx! {
                                div { class: "flex justify-center py-4",
                                    div {
                                        class: "bg-white p-4 rounded-lg",
                                        div {
                                            class: "w-48 h-48 flex items-center justify-center border-2 border-dashed border-gray-300 rounded",
                                            id: "qr-container-multi",
                                        }
                                        script {
                                            dangerous_inner_html: format!(r#"
                                                (function() {{
                                                    const container = document.getElementById('qr-container-multi');
                                                    if (container && typeof QRCode !== 'undefined') {{
                                                        container.innerHTML = '';
                                                        new QRCode(container, {{
                                                            text: 'lightning:{}',
                                                            width: 180,
                                                            height: 180,
                                                            colorDark: '#000000',
                                                            colorLight: '#ffffff',
                                                            correctLevel: QRCode.CorrectLevel.M
                                                        }});
                                                    }} else {{
                                                        container.innerHTML = '<div class="text-xs text-center text-gray-500 p-4">QR Code<br/>(scan in wallet)</div>';
                                                    }}
                                                }})();
                                            "#, qr_invoice)
                                        }
                                    }
                                }
                            }
                        }

                        // Amount for current merchant
                        div { class: "text-center",
                            p { class: "text-sm text-muted-foreground mb-1", "Paying {payment.name}" }
                            p { class: "text-2xl font-bold text-amber-500",
                                "⚡{format_sats_with_separator(payment.amount_sats)} sats"
                            }
                        }

                        // Invoice text (truncated)
                        div { class: "bg-muted/50 rounded-lg p-3",
                            p { class: "text-xs font-mono break-all text-muted-foreground",
                                {
                                    let display_invoice = if invoice.len() > 60 {
                                        format!("{}...{}", &invoice[..30], &invoice[invoice.len()-20..])
                                    } else {
                                        invoice.clone()
                                    };
                                    display_invoice
                                }
                            }
                        }

                        // Copy button
                        {
                            let invoice_for_copy = invoice.clone();
                            rsx! {
                                button {
                                    class: "w-full py-3 border border-border hover:bg-accent rounded-lg font-medium transition flex items-center justify-center gap-2",
                                    onclick: move |_| {
                                        let inv = invoice_for_copy.clone();
                                        spawn(async move {
                                            if crate::utils::clipboard::copy_to_clipboard(&inv).await.is_ok() {
                                                copied.set(true);
                                                gloo_timers::future::TimeoutFuture::new(2000).await;
                                                copied.set(false);
                                            }
                                        });
                                    },
                                    if *copied.read() {
                                        "✓ Copied!"
                                    } else {
                                        "📋 Copy Invoice"
                                    }
                                }
                            }
                        }

                        // Confirm payment button
                        button {
                            class: "w-full py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                            onclick: move |_| {
                                on_payment_confirmed.call("manual_payment".to_string());
                            },
                            if current_idx + 1 < total_count {
                                "I've Paid - Next Merchant →"
                            } else {
                                "I've Paid - Complete Order"
                            }
                        }
                    } else {
                        // Loading invoice
                        div { class: "text-center py-8",
                            div { class: "animate-spin w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full mx-auto mb-4" }
                            p { class: "text-muted-foreground", "Getting invoice for {payment.name}..." }
                        }
                    }
                }
            }

            // Cancel button
            button {
                class: "w-full py-2 text-muted-foreground hover:text-foreground text-sm transition",
                onclick: move |_| on_cancel.call(()),
                "Cancel"
            }

            // Note
            p { class: "text-xs text-muted-foreground text-center",
                "Each merchant receives payment directly. \
                Manual confirmation trusts your honesty."
            }
        }
    }
}

/// Shipping step component with address and shipping options
#[component]
fn ShippingStep(
    cart_items: Vec<CartItem>,
    shipping_address: String,
    on_address_change: EventHandler<String>,
    on_continue: EventHandler<()>,
) -> Element {
    // Get physical items that need shipping selection
    let physical_items: Vec<CartItem> = cart_items.iter()
        .filter(|item| item.product.requires_shipping())
        .cloned()
        .collect();

    // Fetch shipping options for all physical products
    let mut shipping_options_map = use_signal(HashMap::<String, Vec<ShippingOption>>::new);
    let mut shipping_loading = use_signal(|| true);

    // Fetch shipping options on mount
    let items_for_effect = physical_items.clone();
    use_effect(move || {
        let items = items_for_effect.clone();
        spawn(async move {
            shipping_loading.set(true);
            let mut opts_map = HashMap::new();

            for item in items {
                if !item.product.shipping_options.is_empty() {
                    if let Ok(opts) = fetch_shipping_options(&item.product.shipping_options).await {
                        opts_map.insert(item.product.naddr.clone(), opts);
                    }
                }
            }

            shipping_options_map.set(opts_map);
            shipping_loading.set(false);
        });
    });

    // Calculate total shipping cost
    let shipping_total: u64 = physical_items.iter()
        .filter_map(|item| {
            item.selected_shipping.as_ref().and_then(|selected_naddr| {
                shipping_options_map.read()
                    .get(&item.product.naddr)
                    .and_then(|opts| opts.iter().find(|o| &o.naddr == selected_naddr))
                    .map(|opt| opt.base_price as u64)
            })
        })
        .sum();

    // Check if all physical items have shipping selected (or don't need it)
    let all_shipping_selected = physical_items.iter().all(|item| {
        let has_options = shipping_options_map.read()
            .get(&item.product.naddr)
            .map(|opts| !opts.is_empty())
            .unwrap_or(false);
        !has_options || item.selected_shipping.is_some()
    });

    let can_continue = !shipping_address.trim().is_empty() && all_shipping_selected;

    rsx! {
        div { class: "space-y-6",
            h2 { class: "text-lg font-semibold", "Shipping" }

            // Shipping options per item
            if !physical_items.is_empty() {
                div { class: "space-y-4",
                    h3 { class: "text-sm font-medium text-muted-foreground", "Select Shipping Options" }

                    if *shipping_loading.read() {
                        div { class: "text-center py-4",
                            div { class: "animate-spin w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full mx-auto mb-2" }
                            p { class: "text-sm text-muted-foreground", "Loading shipping options..." }
                        }
                    } else {
                        for item in physical_items.iter() {
                            {
                                let opts = shipping_options_map.read()
                                    .get(&item.product.naddr)
                                    .cloned()
                                    .unwrap_or_default();
                                let selected = item.selected_shipping.clone();

                                if !opts.is_empty() {
                                    rsx! {
                                        div {
                                            key: "{item.product.naddr}",
                                            class: "bg-card border border-border rounded-lg p-4",

                                            // Product info
                                            div { class: "flex items-center gap-3 mb-3",
                                                div { class: "w-12 h-12 bg-muted rounded shrink-0 overflow-hidden",
                                                    if let Some(img) = item.product.images.first() {
                                                        img {
                                                            src: "{img.url}",
                                                            class: "w-full h-full object-cover"
                                                        }
                                                    } else {
                                                        div { class: "w-full h-full flex items-center justify-center text-xl",
                                                            "📦"
                                                        }
                                                    }
                                                }
                                                div { class: "flex-1 min-w-0",
                                                    p { class: "font-medium truncate", "{item.product.title}" }
                                                    p { class: "text-sm text-muted-foreground", "Qty: {item.quantity}" }
                                                }
                                            }

                                            // Shipping options
                                            div { class: "space-y-2",
                                                for opt in opts.iter() {
                                                    {
                                                        let opt_naddr = opt.naddr.clone();
                                                        let item_naddr = item.product.naddr.clone();
                                                        let is_selected = selected.as_ref() == Some(&opt.naddr);
                                                        rsx! {
                                                            button {
                                                                key: "{opt.naddr}",
                                                                class: if is_selected {
                                                                    "w-full p-3 bg-blue-500/10 border-2 border-blue-500 rounded-lg text-left"
                                                                } else {
                                                                    "w-full p-3 bg-muted/50 border border-border rounded-lg text-left hover:border-ring transition"
                                                                },
                                                                onclick: move |_| {
                                                                    set_cart_item_shipping(&item_naddr, Some(opt_naddr.clone()));
                                                                },
                                                                div { class: "flex items-center justify-between",
                                                                    div {
                                                                        p { class: "font-medium text-sm", "{opt.title}" }
                                                                        if !opt.countries.is_empty() {
                                                                            p { class: "text-xs text-muted-foreground",
                                                                                "Ships to: {opt.countries.join(\", \")}"
                                                                            }
                                                                        }
                                                                        if let Some(duration) = opt.display_duration() {
                                                                            p { class: "text-xs text-muted-foreground",
                                                                                "Est. delivery: {duration}"
                                                                            }
                                                                        }
                                                                    }
                                                                    div { class: "text-right",
                                                                        span { class: "text-amber-500 font-medium",
                                                                            "{opt.display_price()}"
                                                                        }
                                                                        if is_selected {
                                                                            span { class: "ml-2 text-blue-500", "✓" }
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
                                } else {
                                    rsx! {}
                                }
                            }
                        }
                    }
                }
            }

            // Shipping address
            div { class: "bg-card border border-border rounded-lg p-4",
                h3 { class: "text-sm font-medium mb-3", "Delivery Address" }
                p { class: "text-xs text-muted-foreground mb-4",
                    "Your address will be encrypted and only visible to the seller."
                }

                textarea {
                    class: "w-full h-32 px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none",
                    placeholder: "Enter your full shipping address...\n\nName\nStreet Address\nCity, State/Province, ZIP\nCountry",
                    value: "{shipping_address}",
                    oninput: move |e| on_address_change.call(e.value())
                }
            }

            // Shipping cost summary
            if shipping_total > 0 {
                div { class: "bg-muted/50 rounded-lg p-4",
                    div { class: "flex justify-between",
                        span { class: "text-muted-foreground", "Shipping" }
                        span { class: "text-amber-500 font-medium", "⚡{format_sats_with_separator(shipping_total)} sats" }
                    }
                }
            }

            // Warning if shipping not selected
            if !all_shipping_selected && !*shipping_loading.read() {
                div { class: "bg-amber-500/10 border border-amber-500/30 rounded-lg p-3 text-sm text-amber-600 dark:text-amber-400",
                    "Please select a shipping option for all items before continuing."
                }
            }

            // Continue button
            button {
                class: "w-full py-4 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                disabled: !can_continue,
                onclick: move |_| on_continue.call(()),
                "Continue to Payment"
            }
        }
    }
}
