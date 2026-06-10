//! Create a new Mostro P2P order
//!
//! Full compose form: the user fills in order details, the client builds a
//! `SmallOrder`, wraps it in `Action::NewOrder`, and sends it to the daemon.
//! The daemon assigns an order ID, publishes the kind-38383 event to relays,
//! and ACKs back with `Action::NewOrder` containing the assigned ID.

use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::services::{btc_price, payments::yadio};
use crate::stores::auth_store;
use crate::stores::social::mostro::nip78 as mostro_terms;
use crate::stores::social::mostro::trade_store::{Trade, TradeRole};
use crate::stores::social::mostro::{
    self, MostroKeyState, MOSTRO_KEYS, ensure_node_relays_connected, PENDING_CREATE_SUB,
    parse_node_pubkey,
};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use mostro_core::order::{Kind as MostroKind, SmallOrder, Status as MostroStatus};
use nostr::prelude::*;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Debug)]
enum OrderKind {
    Buy,
    Sell,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ExpirationPreset {
    OneDay,
    ThreeDays,
    SevenDays,
}

#[component]
pub fn P2PCreateOrder() -> Element {
    let keys = MOSTRO_KEYS.read();
    let terms_accepted = *mostro_terms::P2P_TERMS_ACCEPTED.read();

    let mut order_kind = use_signal(|| OrderKind::Sell);
    let mut fiat_code = use_signal(|| "USD".to_string());
    let mut show_suggestions = use_signal(|| false);    let mut is_range = use_signal(|| false);
    let mut fiat_amount = use_signal(String::new);
    let mut range_min = use_signal(String::new);
    let mut range_max = use_signal(String::new);
    let mut sats_amount = use_signal(|| "0".to_string());
    let mut premium = use_signal(|| "0".to_string());
    let mut payment_method = use_signal(String::new);
    let mut buyer_invoice = use_signal(String::new);
    let mut expiration = use_signal(|| ExpirationPreset::ThreeDays);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let nav = navigator();

    use_future(move || async move {
        if yadio::rates_are_stale() {
            let _ = yadio::fetch_yadio_rates().await;
        }
    });

    let suggestions: Vec<String> = {
        let query = fiat_code.read().to_uppercase();
        let prices = btc_price::BTC_PRICES.read();
        let yadio = yadio::YADIO_RATES.read();
        let mut set = std::collections::BTreeSet::new();
        for k in prices.keys() {
            set.insert(k.clone());
        }
        for k in yadio.keys() {
            set.insert(k.clone());
        }
        if query.is_empty() {
            set.into_iter().collect()
        } else {
            set.into_iter().filter(|k| k.starts_with(&query)).collect()
        }
    };

    let sats_val: i64 = sats_amount.read().parse().unwrap_or(0);
    let prem_val: i64 = premium.read().parse().unwrap_or(0);
    let sats_locked = prem_val != 0;
    let prem_locked = sats_val != 0;
    let is_buy = *order_kind.read() == OrderKind::Buy;

    let on_submit = move |_| {
        error.set(None);
        let fc = fiat_code.read().trim().to_uppercase();
        let pm = payment_method.read().trim().to_string();
        if fc.is_empty() {
            error.set(Some("Fiat currency is required.".to_string()));
            return;
        }
        if !yadio::is_currency_supported(&fc) && !yadio::YADIO_RATES.read().is_empty() {
            error.set(Some(format!(
                "Unsupported currency: {fc}. Pick from the suggestions list."
            )));
            return;
        }
        if pm.is_empty() {
            error.set(Some("Payment method is required.".to_string()));
            return;
        }
        let sats: i64 = sats_amount.read().parse().unwrap_or(0);
        let prem: i64 = premium.read().parse().unwrap_or(0);
        if sats < 0 {
            error.set(Some("Sats amount cannot be negative.".to_string()));
            return;
        }
        if sats > 0 && prem != 0 {
            error.set(Some(
                "Cannot combine a fixed sats amount with a premium. Set one to 0.".to_string(),
            ));
            return;
        }

        let (fiat_amt, min_amt, max_amt) = if *is_range.read() {
            let min: i64 = range_min.read().parse().unwrap_or(0);
            let max: i64 = range_max.read().parse().unwrap_or(0);
            if min <= 0 || max <= 0 {
                error.set(Some("Range min and max must be positive.".to_string()));
                return;
            }
            if min >= max {
                error.set(Some("Range min must be less than max.".to_string()));
                return;
            }
            if sats != 0 {
                error.set(Some(
                    "Range orders must use market pricing (sats = 0).".to_string(),
                ));
                return;
            }
            (0, Some(min), Some(max))
        } else {
            let fa: i64 = fiat_amount.read().parse().unwrap_or(0);
            if fa <= 0 {
                error.set(Some("Fiat amount must be positive.".to_string()));
                return;
            }
            (fa, None, None)
        };

        let inv = buyer_invoice.read().trim().to_string();
        let buyer_inv = if is_buy && !inv.is_empty() {
            Some(inv)
        } else {
            None
        };

        let expires_at = match *expiration.read() {
            ExpirationPreset::OneDay => {
                Some(crate::platform::timestamp::now_secs() as i64 + 86400)
            }
            ExpirationPreset::ThreeDays => {
                Some(crate::platform::timestamp::now_secs() as i64 + 259200)
            }
            ExpirationPreset::SevenDays => {
                Some(crate::platform::timestamp::now_secs() as i64 + 604800)
            }
        };

        submitting.set(true);
        spawn(async move {
            let mut k = match mostro::try_get() {
                Some(k) => k,
                None => {
                    error.set(Some("Mostro keys not initialized.".to_string()));
                    submitting.set(false);
                    return;
                }
            };
            let node = match mostro::try_get_node_config() {
                Some(n) => n,
                None => {
                    error.set(Some(
                        "Mostro node not configured. Visit Settings → P2P.".to_string(),
                    ));
                    submitting.set(false);
                    return;
                }
            };
            let trade_keys = match k.next_protocol_trade_keys() {
                Ok(tk) => tk,
                Err(e) => {
                    error.set(Some(format!("Key derivation failed: {e}")));
                    submitting.set(false);
                    return;
                }
            };
            mostro::write_back_trade_index(k.trade_index);
            let trade_pubkey_hex = trade_keys.public_key().to_hex();
            let trade_index_used = k.trade_index.saturating_sub(1);
            let trade_index_opt = if k.privacy_mode {
                None
            } else {
                Some(trade_index_used)
            };

            let kind = match *order_kind.read() {
                OrderKind::Buy => MostroKind::Buy,
                OrderKind::Sell => MostroKind::Sell,
            };

            let order = SmallOrder::new(
                None,
                Some(kind),
                Some(MostroStatus::Pending),
                sats,
                fc.clone(),
                min_amt,
                max_amt,
                fiat_amt,
                pm.clone(),
                prem,
                None,
                None,
                buyer_inv,
                Some(0),
                expires_at,
            );

            let message = mostro::new_order(&k, order.clone(), trade_index_used);
            drop(k);

            let node_pk = match parse_node_pubkey(&node.pubkey) {
                Ok(p) => p,
                Err(e) => {
                    error.set(Some(e));
                    submitting.set(false);
                    return;
                }
            };

            ensure_node_relays_connected().await;

            // Subscribe to GiftWraps for the trade pubkey BEFORE sending
            // (matches mostro-cli pattern: subscribe → send → wait for response)
            {
                let client = match crate::stores::nostr_client::get_client() {
                    Some(c) => c,
                    None => {
                        error.set(Some("Nostr client not available".to_string()));
                        submitting.set(false);
                        return;
                    }
                };
                let urls: Vec<nostr::Url> = node
                    .relays
                    .iter()
                    .filter_map(|u| nostr::Url::parse(u).ok())
                    .collect();
                let sub_filter = Filter::new()
                    .kind(Kind::GiftWrap)
                    .custom_tags(
                        nostr_sdk::prelude::SingleLetterTag::lowercase(nostr_sdk::prelude::Alphabet::P),
                        [trade_keys.public_key().to_hex()],
                    )
                    .limit(0);
                if let Ok(output) = client.subscribe_to(urls, sub_filter, None).await {
                    *PENDING_CREATE_SUB.write() = Some((output.val, trade_keys.public_key()));
                }
            }

            if let Err(e) = mostro::send_mostro_message(
                &message,
                &mostro::try_get().unwrap().identity_keys,
                &trade_keys,
                node_pk,
                &node.relays,
                node.pow,
            )
            .await
            {
                error.set(Some(format!("Send failed: {e}")));
                submitting.set(false);
                return;
            }

            let fiat_display = if *is_range.read() {
                format!(
                    "{}-{}",
                    range_min.read().clone(),
                    range_max.read().clone()
                )
            } else {
                fiat_amount.read().clone()
            };

            let mut trade = Trade::new_pending(
                format!("maker-{trade_index_used}"),
                String::new(),
                String::new(),
                TradeRole::Maker,
                format!("{kind}"),
                fiat_display,
                fc,
                if sats > 0 { Some(sats) } else { None },
                prem as f64,
                pm.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                trade_index_opt,
            );
            trade.maker_trade_pubkey = Some(trade_pubkey_hex);
            mostro::upsert_trade(trade);
            let _ = mostro::publish_trades().await;

            let toast = consume_toast();
            toast.info(
                "Order created".to_string(),
                ToastOptions::new()
                    .description("Waiting for daemon confirmation.".to_string())
                    .duration(Duration::from_secs(3)),
            );
            let _ = nav.push(Route::P2PTradeDetail { order_id: format!("maker-{trade_index_used}") });
            submitting.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen p-4 max-w-2xl mx-auto",
            if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if !auth_store::is_authenticated() {
                div { class: "p-8 text-center",
                    h2 { class: "text-xl font-bold mb-2", "Sign in required" }
                    p { class: "text-muted-foreground",
                        "You need to be signed in to create a Mostro order."
                    }
                }
            } else if terms_accepted == Some(false) {
                crate::components::MostroTermsModal { on_accept: move |_| {} }
            } else {
                div { class: "space-y-4",
                    div { class: "flex items-center gap-3",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg",
                            onclick: move |_| {
                                let _ = nav.push(Route::P2PHome {});
                            },
                            crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                        }
                        h1 { class: "text-xl font-bold", "Create Order" }
                    }

                    match &*keys {
                        MostroKeyState::NotInitialized => rsx! {
                            div { class: "p-4 bg-card border border-border rounded-lg text-sm text-amber-600 dark:text-amber-400",
                                "Mostro keys are initializing. Please wait a moment and refresh."
                            }
                        },
                        MostroKeyState::Loading => rsx! {
                            div { class: "p-4 bg-card border border-border rounded-lg text-sm text-muted-foreground",
                                "Loading Mostro keys..."
                            }
                        },
                        MostroKeyState::Error(e) => rsx! {
                            div { class: "p-4 bg-card border border-border rounded-lg text-sm text-red-500",
                                "Error: {e}"
                            }
                        },
                        MostroKeyState::Ready(_) => rsx! {
                            div { class: "space-y-4",

                                // Order type toggle
                                div { class: "flex gap-2",
                                    button {
                                        class: if *order_kind.read() == OrderKind::Sell {
                                            "flex-1 py-2 text-sm font-medium rounded-lg bg-red-500/20 text-red-600 dark:text-red-400 border-2 border-red-500/40"
                                        } else {
                                            "flex-1 py-2 text-sm font-medium rounded-lg border border-border text-muted-foreground hover:text-foreground transition"
                                        },
                                        onclick: move |_| order_kind.set(OrderKind::Sell),
                                        "Sell Sats"
                                    }
                                    button {
                                        class: if *order_kind.read() == OrderKind::Buy {
                                            "flex-1 py-2 text-sm font-medium rounded-lg bg-green-500/20 text-green-600 dark:text-green-400 border-2 border-green-500/40"
                                        } else {
                                            "flex-1 py-2 text-sm font-medium rounded-lg border border-border text-muted-foreground hover:text-foreground transition"
                                        },
                                        onclick: move |_| order_kind.set(OrderKind::Buy),
                                        "Buy Sats"
                                    }
                                }

                                // Fiat currency with autocomplete
                                div { class: "relative",
                                    label { class: "text-xs text-muted-foreground", "Fiat Currency" }
                                    input {
                                        class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm font-mono uppercase",
                                        r#type: "text",
                                        placeholder: "USD",
                                        value: "{fiat_code}",
                                        maxlength: "3",
                                        oninput: move |e| {
                                            fiat_code.set(e.value().to_uppercase());
                                            show_suggestions.set(true);
                                        },
                                        onfocus: move |_| show_suggestions.set(true),
                                        onblur: move |_| {
                                            spawn(async move {
                                                crate::platform::timer::sleep(Duration::from_millis(200)).await;
                                                show_suggestions.set(false);
                                            });
                                        },
                                    }
                                    if *show_suggestions.read() && !suggestions.is_empty() {
                                        div { class: "absolute z-30 w-full mt-1 bg-card border border-border rounded-lg shadow-lg max-h-40 overflow-y-auto",
                                            for sug in &suggestions {
                                                button {
                                                    class: "w-full text-left px-3 py-1.5 text-sm hover:bg-accent font-mono",
                                                    onmousedown: {
                                                        let s = sug.clone();
                                                        move |_| {
                                                            fiat_code.set(s.clone());
                                                            show_suggestions.set(false);
                                                        }
                                                    },
                                                    "{sug}"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Range toggle
                                div { class: "flex items-center gap-2",
                                    input {
                                        r#type: "checkbox",
                                        checked: *is_range.read(),
                                        onchange: move |e| is_range.set(e.checked()),
                                        class: "rounded border-border",
                                    }
                                    label { class: "text-sm", "Range order (min-max fiat amount)" }
                                }

                                // Fiat amount (or range inputs)
                                if *is_range.read() {
                                    div { class: "grid grid-cols-2 gap-3",
                                        div {
                                            label { class: "text-xs text-muted-foreground", "Min Fiat" }
                                            input {
                                                class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm",
                                                r#type: "number",
                                                placeholder: "50",
                                                value: "{range_min}",
                                                oninput: move |e| range_min.set(e.value()),
                                            }
                                        }
                                        div {
                                            label { class: "text-xs text-muted-foreground", "Max Fiat" }
                                            input {
                                                class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm",
                                                r#type: "number",
                                                placeholder: "500",
                                                value: "{range_max}",
                                                oninput: move |e| range_max.set(e.value()),
                                            }
                                        }
                                    }
                                } else {
                                    div {
                                        label { class: "text-xs text-muted-foreground", "Fiat Amount" }
                                        input {
                                            class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm",
                                            r#type: "number",
                                            placeholder: "100",
                                            value: "{fiat_amount}",
                                            oninput: move |e| fiat_amount.set(e.value()),
                                        }
                                    }
                                }

                                // Sats amount
                                div {
                                    label { class: "text-xs text-muted-foreground",
                                        if sats_locked {
                                            "Sats Amount (locked — premium is set)"
                                        } else {
                                            "Sats Amount (0 = market rate)"
                                        }
                                    }
                                    input {
                                        class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm disabled:opacity-50",
                                        r#type: "number",
                                        placeholder: "0 = market rate",
                                        value: "{sats_amount}",
                                        disabled: sats_locked,
                                        oninput: move |e| sats_amount.set(e.value()),
                                    }
                                }

                                // Premium
                                div {
                                    label { class: "text-xs text-muted-foreground",
                                        if prem_locked {
                                            "Premium % (locked — sats amount is set)"
                                        } else {
                                            "Premium %"
                                        }
                                    }
                                    input {
                                        class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm disabled:opacity-50",
                                        r#type: "number",
                                        placeholder: "0",
                                        value: "{premium}",
                                        disabled: prem_locked,
                                        oninput: move |e| premium.set(e.value()),
                                    }
                                }

                                // Payment method
                                div {
                                    label { class: "text-xs text-muted-foreground", "Payment Method(s)" }
                                    input {
                                        class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm",
                                        r#type: "text",
                                        placeholder: "SEPA, Bank transfer",
                                        value: "{payment_method}",
                                        oninput: move |e| payment_method.set(e.value()),
                                    }
                                }

                                // Buyer invoice (buy only)
                                if is_buy {
                                    div {
                                        label { class: "text-xs text-muted-foreground", "Payout Invoice (optional)" }
                                        input {
                                            class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm font-mono",
                                            r#type: "text",
                                            placeholder: "lnbc...",
                                            value: "{buyer_invoice}",
                                            oninput: move |e| buyer_invoice.set(e.value()),
                                        }
                                    }
                                }

                                // Expiration
                                div {
                                    label { class: "text-xs text-muted-foreground", "Expiration" }
                                    div { class: "flex gap-2 mt-1",
                                        {
                                            let opts = [
                                                (ExpirationPreset::OneDay, "1 day", 1),
                                                (ExpirationPreset::ThreeDays, "3 days", 3),
                                                (ExpirationPreset::SevenDays, "7 days", 7),
                                            ];
                                            opts.into_iter().map(|(preset, label, _val)| {
                                                let active = *expiration.read() == preset;
                                                let cls = if active {
                                                    "flex-1 py-2 text-sm font-medium rounded-lg bg-primary text-primary-foreground".to_string()
                                                } else {
                                                    "flex-1 py-2 text-sm font-medium rounded-lg border border-border text-muted-foreground hover:text-foreground transition".to_string()
                                                };
                                                rsx! {
                                                    button {
                                                        key: "{label}",
                                                        class: "{cls}",
                                                        onclick: move |_| expiration.set(preset),
                                                        "{label}"
                                                    }
                                                }
                                            })
                                        }
                                    }
                                }

                                // Error display
                                if let Some(err) = error.read().as_ref() {
                                    div { class: "p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-600 dark:text-red-400",
                                        "{err}"
                                    }
                                }

                                // Submit
                                button {
                                    class: "w-full px-4 py-3 bg-primary text-primary-foreground rounded-lg text-sm font-medium disabled:opacity-50",
                                    disabled: *submitting.read(),
                                    onclick: on_submit,
                                    if *submitting.read() {
                                        span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin mr-2" }
                                        "Creating..."
                                    } else {
                                        "Create Order"
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}
