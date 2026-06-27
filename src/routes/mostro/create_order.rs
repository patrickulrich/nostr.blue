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
use crate::stores::mostro::nip78 as mostro_terms;
use crate::stores::mostro::trade_store::{Trade, TradeRole};
use crate::stores::mostro::{
    self, MostroKeyState, MOSTRO_KEYS, ensure_node_relays_connected,
    parse_node_pubkey, unwrap_mostro_response,
};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use mostro_core::order::{Kind as MostroKind, SmallOrder, Status as MostroStatus};
use nostr::prelude::*;
use nostr_relay_pool::relay::ReqExitPolicy;
use nostr_relay_pool::SubscribeAutoCloseOptions;
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
pub fn MostroCreateOrder() -> Element {
    let keys = MOSTRO_KEYS.read();
    let terms_accepted = *mostro_terms::P2P_TERMS_ACCEPTED.read();

    let mut order_kind = use_signal(|| OrderKind::Sell);
    // Phase 7.2: default to user's preferred fiat currency (was hardcoded USD).
    let mut fiat_code = use_signal(|| {
        crate::stores::ui::p2p_settings::default_fiat_or_usd()
    });
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

        // D2: client-side pending-order collision check. The daemon
        // refuses with `CantDo(PendingOrderExists)` if the user already
        // has a pending maker order; this preempts the round-trip with a
        // local warning so the user knows to cancel or wait first.
        // Best-effort — if the cache is stale, the daemon's check still
        // catches it authoritatively.
        let current_daemon_pk = mostro::try_get_node_config().map(|c| c.pubkey).unwrap_or_default();
        let has_pending = mostro::TRADES.read().iter().any(|t| {
            t.status == crate::stores::mostro::trade_store::TradeStatus::Pending
                && (t.daemon_pubkey.is_empty() || t.daemon_pubkey == current_daemon_pk)
                && t.role == crate::stores::mostro::trade_store::TradeRole::Maker
        });
        if has_pending {
            error.set(Some(
                "You already have a pending maker order on this daemon. The daemon will \
                 reject this creation with PendingOrderExists. Cancel or wait for the \
                 existing order to be taken first."
                    .to_string(),
            ));
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
            let identity_keys = k.identity_keys.clone();
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

            let client = match crate::stores::nostr_client::get_client() {
                Some(c) => c,
                None => {
                    error.set(Some("Nostr client not available".to_string()));
                    submitting.set(false);
                    return;
                }
            };

            // Step 1: Open the global notification channel FIRST so we don't
            // miss any events (matches mostro-cli pattern).
            let mut notifications = client.notifications();

            // Step 2: Subscribe to Mostro DMs for the trade pubkey with
            // auto-close after 1 event. Transport-aware: v1 daemons → kind
            // 1059, v2 daemons → kind 14 with `authors=[daemon]` pin. Using
            // subscribe_to (specific relays) ensures we listen on the same
            // relays the daemon publishes to.
            let urls: Vec<nostr::Url> = node
                .relays
                .iter()
                .filter_map(|u| nostr::Url::parse(u).ok())
                .collect();
            let sub_filter = mostro::active_trade_filter(&[trade_keys.public_key()]);
            let auto_close = SubscribeAutoCloseOptions::default()
                .exit_policy(ReqExitPolicy::WaitForEventsAfterEOSE(1))
                .timeout(Some(Duration::from_secs(20)));
            let sub_id = match client
                .subscribe_to(urls, sub_filter, Some(auto_close))
                .await
            {
                Ok(output) => output.val,
                Err(e) => {
                    error.set(Some(format!("Subscription failed: {e}")));
                    submitting.set(false);
                    return;
                }
            };

            // Step 3: Resolve the effective PoW (proactively fetches from
            // daemon if our cached value is 0), then send the message.
            let pow = mostro::resolve_effective_pow(&node, node_pk).await;
            if let Err(e) = mostro::send_mostro_message(
                &message,
                &identity_keys,
                &trade_keys,
                node_pk,
                &node.relays,
                pow,
            )
            .await
            {
                let _ = client.unsubscribe(&sub_id).await;
                error.set(Some(format!("Send failed: {e}")));
                submitting.set(false);
                return;
            }

            // Step 4: Wait up to 15 seconds for the daemon's NewOrder ACK.
            // Uses futures::select! to race the notification listener against a
            // timeout, matching the mostro-cli pattern.
            let trade_pk_for_filter = trade_keys.public_key();
            let listen_fut = async {
                loop {
                    match notifications.recv().await {
                        Ok(nostr_sdk::RelayPoolNotification::Event {
                            event, ..
                        }) => {
                            // Phase 2d: accept both Mostro DM transports. v1
                            // daemons reply with kind 1059; v2 daemons reply
                            // with kind 14 (authored by the daemon). Other
                            // kinds (e.g. NIP-17 peer chat on kind 14 from a
                            // non-daemon author) are skipped here — the
                            // `unwrap_mostro_response` path below also guards.
                            if event.kind != Kind::GiftWrap
                                && event.kind != Kind::PrivateDirectMessage
                            {
                                continue;
                            }
                            if !event
                                .tags
                                .public_keys()
                                .any(|pk| *pk == trade_pk_for_filter)
                            {
                                continue;
                            }
                            let unwrapped =
                                match unwrap_mostro_response(&event, &trade_keys).await {
                                    Ok(Some(u)) => u,
                                    _ => continue,
                                };
                            let action = unwrapped
                                .message
                                .inner_action()
                                .unwrap_or(mostro_core::prelude::Action::CantDo);
                            return Ok::<_, ()>(Some((action, unwrapped)));
                        }
                        Ok(_) => continue,
                        Err(_) => return Ok::<_, ()>(None),
                    }
                }
            };

            let timeout_fut = crate::platform::timer::sleep(Duration::from_secs(15));

            futures::pin_mut!(listen_fut, timeout_fut);
            let ack = futures::future::select(listen_fut, timeout_fut).await;

            // Unsubscribe — the auto-close should handle it, but be explicit.
            let _ = client.unsubscribe(&sub_id).await;

            let mut real_order_id: Option<String> = None;

            match ack {
                futures::future::Either::Left((result, _)) => match result {
                    Ok(Some((action, unwrapped))) => match action {
                        mostro_core::prelude::Action::NewOrder => {
                            if let Some(mostro_core::prelude::Payload::Order(ord)) =
                                &unwrapped.message.get_inner_message_kind().payload
                            {
                                if let Some(id) = ord.id {
                                    real_order_id = Some(id.to_string());
                                }
                            }
                        }
                        mostro_core::prelude::Action::CantDo => {
                            let msg =
                                if let Some(mostro_core::prelude::Payload::CantDo(reason)) =
                                    &unwrapped.message.get_inner_message_kind().payload
                                {
                                    reason
                                        .as_ref()
                                        .map(crate::stores::mostro::cant_do_message)
                                        .unwrap_or_else(|| "Unknown reason".to_string())
                                } else {
                                    "Order rejected".to_string()
                                };
                            let toast = consume_toast();
                            toast.error(
                                "Cannot proceed".to_string(),
                                ToastOptions::new()
                                    .description(msg)
                                    .duration(Duration::from_secs(5)),
                            );
                            submitting.set(false);
                            return;
                        }
                        _ => {}
                    },
                    _ => {
                        log::info!("Mostro listen ended without result");
                    }
                },
                futures::future::Either::Right(_) => {
                    log::info!(
                        "Mostro NewOrder ACK timed out after 15s, navigating with placeholder ID"
                    );

                    // PoW mismatch detection: the daemon silently discards
                    // messages with insufficient proof-of-work. Re-fetch the
                    // daemon's actual requirement and compare.  When the
                    // proactive fetch in Step 3 already set the correct PoW
                    // this is a no-op (actual_pow == sent_pow).
                    let sent_pow = pow;
                    let actual_pow = mostro::client::fetch_daemon_pow(
                        node_pk,
                        &node.relays,
                    )
                    .await
                    .unwrap_or(sent_pow);
                    if actual_pow > sent_pow {
                        log::warn!(
                            "PoW mismatch: daemon requires {actual_pow}, we sent {sent_pow}"
                        );
                        let toast = consume_toast();
                        toast.warning(
                            "Order may have been rejected".to_string(),
                            ToastOptions::new()
                                .description(format!(
                                    "Daemon requires PoW {actual_pow} but we used {sent_pow}. \
                                     The order was sent but the daemon may have silently discarded it. \
                                     Try again after the node config refreshes."
                                ))
                                .duration(Duration::from_secs(10)),
                        );
                        if let Some(mut cfg) = mostro::try_get_node_config() {
                            cfg.pow = actual_pow;
                            let _ = mostro::save_node_config(cfg).await;
                        }
                    }
                }
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

            let order_id_for_nav = real_order_id
                .clone()
                .unwrap_or_else(|| format!("maker-{trade_index_used}"));

            let mut trade = Trade::new_pending(
                order_id_for_nav.clone(),
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
            trade.my_trade_pubkey = Some(trade_pubkey_hex);
            trade.expires_at = expires_at;
            mostro::upsert_trade(trade);
            let _ = mostro::publish_trades().await;

            if real_order_id.is_some() {
                let toast = consume_toast();
                toast.info(
                    "Order confirmed".to_string(),
                    ToastOptions::new()
                        .description("The daemon accepted your order.".to_string())
                        .duration(Duration::from_secs(3)),
                );
            } else {
                let toast = consume_toast();
                toast.info(
                    "Order submitted".to_string(),
                    ToastOptions::new()
                        .description("Waiting for daemon confirmation...".to_string())
                        .duration(Duration::from_secs(3)),
                );
            }
            let _ = nav.push(Route::MostroTradeDetail {
                order_id: order_id_for_nav,
            });
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
                                let _ = nav.push(Route::MostroHome {});
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
                                // E1: range orders are incompatible with privacy mode —
                                // the daemon's child-order handler requires unique per-slice
                                // trade keys (see /home/patrick/mostro/src/app/release.rs:394-444),
                                // which privacy mode can't provide without leaking the
                                // maker's identity across slices.
                                div { class: "flex items-center gap-2",
                                    input {
                                        r#type: "checkbox",
                                        checked: *is_range.read(),
                                        disabled: *crate::stores::mostro::MOSTRO_PRIVACY_MODE.read(),
                                        onchange: move |e| {
                                            if !*crate::stores::mostro::MOSTRO_PRIVACY_MODE.read() {
                                                is_range.set(e.checked());
                                            }
                                        },
                                        class: "rounded border-border disabled:opacity-50",
                                        title: "Range orders require rotating trade keys; disable privacy mode to use them.",
                                    }
                                    label {
                                        class: if *crate::stores::mostro::MOSTRO_PRIVACY_MODE.read() {
                                            "text-sm text-muted-foreground/60"
                                        } else {
                                            "text-sm"
                                        },
                                        title: "Range orders require rotating trade keys; disable privacy mode to use them.",
                                        "Range order (min-max fiat amount)"
                                    }
                                }
                                if *crate::stores::mostro::MOSTRO_PRIVACY_MODE.read() {
                                    p { class: "text-xs text-muted-foreground/70 mt-1",
                                        "Range orders require rotating trade keys; disable privacy mode to use them."
                                    }
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
                                // E5: disabled while a session restore is in flight —
                                // taking/creating during a restore can race the
                                // trade-index counter (see take.rs::take_order).
                                button {
                                    class: "w-full px-4 py-3 bg-primary text-primary-foreground rounded-lg text-sm font-medium disabled:opacity-50",
                                    disabled: *submitting.read()
                                        || crate::stores::mostro::is_restore_in_progress(),
                                    onclick: on_submit,
                                    if *submitting.read() {
                                        span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin mr-2" }
                                        "Creating..."
                                    } else if crate::stores::mostro::is_restore_in_progress() {
                                        "Restore in progress…"
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
