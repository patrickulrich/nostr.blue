use crate::platform;
use crate::services::routstr::{
    self, RoutstrBalance, RoutstrLightningInvoice, RoutstrRefundResult,
};
use crate::stores::ai_provider_store::{self, AiProviderState, PROVIDER_STATE_SAVE_EVENT};
use dioxus::prelude::*;
use qrcode::{render::svg, QrCode};

#[component]
pub fn RoutstrSettingsPanel(
    state: Signal<AiProviderState>,
    is_saving: Signal<bool>,
    save_error: Signal<Option<String>>,
    provider_state_ready: bool,
) -> Element {
    let mut api_key_input = use_signal(String::new);
    let mut key_saving = use_signal(|| false);
    let mut key_error = use_signal(|| None::<String>);
    let mut balance = use_signal(|| None::<RoutstrBalance>);
    let balance_loading = use_signal(|| false);
    let mut balance_error = use_signal(|| None::<String>);
    let mut topup_amount = use_signal(|| "1000".to_string());
    let mut topup_invoice = use_signal(|| None::<RoutstrLightningInvoice>);
    let mut topup_loading = use_signal(|| false);
    let mut topup_error = use_signal(|| None::<String>);
    let mut cashu_token_input = use_signal(String::new);
    let mut cashu_loading = use_signal(|| false);
    let mut cashu_error = use_signal(|| None::<String>);
    let mut cashu_success = use_signal(|| None::<String>);
    let mut refund_result = use_signal(|| None::<RoutstrRefundResult>);
    let mut refund_loading = use_signal(|| false);
    let mut refund_error = use_signal(|| None::<String>);
    let mut last_loaded_key = use_signal(|| None::<String>);
    let mut request_generation = use_signal(|| 0u32);
    let mut pending_save_snapshot = use_signal(|| None::<AiProviderState>);
    let mut pending_save_min_event_id = use_signal(|| 0u64);

    let current_api_key = state.read().routstr_api_key.clone();

    // Balance loader: re-fetch when api_key changes
    use_effect(move || {
        if !provider_state_ready {
            return;
        }
        let key = state.read().routstr_api_key.clone();
        if last_loaded_key.read().as_ref() == key.as_ref() {
            return;
        }
        last_loaded_key.set(key.clone());
        balance.set(None);
        balance_error.set(None);
        let generation = request_generation.peek().wrapping_add(1);
        request_generation.set(generation);

        let Some(key) = key else {
            return;
        };
        load_routstr_balance(
            balance,
            balance_loading,
            balance_error,
            key,
            Some((request_generation, generation)),
        );
    });

    // Save-completion listener (mirrors PPQ panel Effect B)
    use_effect(move || {
        let pending_snapshot = pending_save_snapshot.read().clone();
        let Some(pending_snapshot) = pending_snapshot else {
            return;
        };
        let Some(event) = PROVIDER_STATE_SAVE_EVENT.read().clone() else {
            return;
        };
        if event.event_id <= *pending_save_min_event_id.read()
            || event.snapshot != pending_snapshot
        {
            return;
        }
        pending_save_snapshot.set(None);
        pending_save_min_event_id.set(event.event_id);
        is_saving.set(false);
        match event.result {
            Ok(()) => save_error.set(None),
            Err(err) => save_error.set(Some(err)),
        }
    });

    // Invoice payment polling loop (auto-cancels on unmount)
    {
        use_future(move || async move {
            let mut attempts = 0u32;
            loop {
                crate::platform::timer::sleep_ms(5000).await;
                let invoice = topup_invoice.read().clone();
                let Some(invoice) = invoice else {
                    attempts = 0;
                    continue;
                };
                attempts += 1;
                if attempts > 60 {
                    topup_error
                        .set(Some("Payment polling timed out. Please check manually.".to_string()));
                    topup_invoice.set(None);
                    attempts = 0;
                    continue;
                }
                if let Ok(status) = routstr::get_lightning_invoice_status(&invoice.invoice_id).await {
                        if status.status.as_deref() == Some("paid") {
                            topup_invoice.set(None);
                            topup_error.set(None);
                            attempts = 0;
                            if let Some(key) = state.read().routstr_api_key.as_ref() {
                                if let Ok(result) = routstr::get_balance(key).await {
                                    balance.set(Some(result));
                                }
                            }
                        }
                        if status.status.as_deref() == Some("expired") {
                            topup_error
                                .set(Some("Invoice expired. Please try again.".to_string()));
                            topup_invoice.set(None);
                            attempts = 0;
                        }
                    }
            }
        });
    }

    let active_key = current_api_key.clone();
    let active_balance = balance.read().clone();
    let display_balance_sats = active_balance
        .as_ref()
        .and_then(|b| b.balance_msats.map(|m| m / 1000))
        .unwrap_or(0);
    let display_total_spent_sats = active_balance
        .as_ref()
        .and_then(|b| b.total_spent_msats.map(|m| m / 1000))
        .unwrap_or(0);
    let display_total_requests = active_balance
        .as_ref()
        .and_then(|b| b.total_requests)
        .unwrap_or(0);

    let topup_invoice_display = topup_invoice.read().clone();
    let topup_qr_svg = topup_invoice_display
        .as_ref()
        .and_then(|inv| inv.bolt11.as_ref())
        .map(|b| generate_invoice_qr_svg(b))
        .unwrap_or_default();
    let refund_display = refund_result.read().clone();

    rsx! {
        div { class: "rounded-xl border border-border bg-card p-6 space-y-6",
            div {
                h3 { class: "text-lg font-semibold text-foreground", "Routstr" }
                p { class: "mt-1 text-sm text-muted-foreground",
                    "Decentralized AI inference marketplace powered by Bitcoin. Pay per request with Lightning or Cashu."
                }
            }

            if active_key.is_none() {
                // API Key entry section
                div { class: "space-y-3",
                    label { class: "text-sm font-medium text-foreground", "API Key or Cashu Token" }
                    input {
                        r#type: "text",
                        class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                        placeholder: "sk-... or cashuA...",
                        value: "{api_key_input}",
                        oninput: move |e| {
                            api_key_input.set(e.value());
                            key_error.set(None);
                        },
                    }
                    if let Some(err) = key_error.read().as_ref() {
                        p { class: "text-xs text-red-600 dark:text-red-400", "{err}" }
                    }
                    button {
                        r#type: "button",
                        class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                        disabled: *key_saving.read() || api_key_input.read().trim().is_empty(),
                        onclick: move |_| {
                            let input = api_key_input.read().trim().to_string();
                            if input.is_empty() {
                                key_error.set(Some("Please enter an API key or Cashu token".to_string()));
                                return;
                            }
                            key_saving.set(true);
                            key_error.set(None);
                            let input_clone = input.clone();
                            spawn(async move {
                                let final_key = if input_clone.starts_with("cashu") {
                                    match routstr::create_key_from_cashu(&input_clone).await {
                                        Ok(key) => key,
                                        Err(err) => {
                                            key_error.set(Some(err));
                                            key_saving.set(false);
                                            return;
                                        }
                                    }
                                } else {
                                    input_clone
                                };
                                let mut next_state = state.read().clone();
                                next_state.routstr_api_key = Some(final_key);
                                persist_routstr_state(
                                    next_state,
                                    state,
                                    is_saving,
                                    save_error,
                                    pending_save_snapshot,
                                    pending_save_min_event_id,
                                );
                                api_key_input.set(String::new());
                                key_saving.set(false);
                            });
                        },
                        if *key_saving.read() { "Saving..." } else { "Save Key" }
                    }
                    a {
                        href: "https://api.routstr.com",
                        target: "_blank",
                        class: "block text-xs text-primary hover:underline",
                        "Create a key at routstr.com →"
                    }
                }
            } else {
                // Balance + Topup + Refund section
                div { class: "space-y-4",
                    // Balance card
                    div { class: "rounded-lg border border-border p-4",
                        div { class: "flex items-center justify-between",
                            div {
                                p { class: "text-sm text-muted-foreground", "Balance" }
                                p { class: "text-2xl font-bold text-foreground",
                                    "{display_balance_sats}"
                                    span { class: "ml-1 text-sm font-normal text-muted-foreground", "sats" }
                                }
                            }
                            button {
                                r#type: "button",
                                class: "rounded-lg border border-border px-3 py-1.5 text-xs transition hover:bg-accent disabled:opacity-60",
                                disabled: *balance_loading.read(),
                                onclick: move |_| {
                                    let key = state.read().routstr_api_key.clone().unwrap_or_default();
                                    load_routstr_balance(
                                        balance,
                                        balance_loading,
                                        balance_error,
                                        key,
                                        None,
                                    );
                                },
                                if *balance_loading.read() { "Loading..." } else { "Refresh" }
                            }
                        }
                        if display_total_requests > 0 || display_total_spent_sats > 0 {
                            div { class: "mt-2 flex gap-4 text-xs text-muted-foreground",
                                span { "{display_total_requests} requests" }
                                span { "{display_total_spent_sats} sats spent" }
                            }
                        }
                        if let Some(err) = balance_error.read().as_ref() {
                            p { class: "mt-2 text-xs text-red-600 dark:text-red-400", "{err}" }
                        }
                    }

                    // Top up via Lightning
                    div { class: "rounded-lg border border-border p-4 space-y-3",
                        p { class: "text-sm font-medium text-foreground", "Top Up via Lightning" }
                        div { class: "flex gap-2",
                            input {
                                r#type: "number",
                                class: "w-32 rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                                placeholder: "sats",
                                value: "{topup_amount}",
                                oninput: move |e| { topup_amount.set(e.value()); },
                            }
                            button {
                                r#type: "button",
                                class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                                disabled: *topup_loading.read(),
                                onclick: move |_| {
                                    let amount = match topup_amount.read().trim().parse::<u64>() {
                                        Ok(a) if a > 0 => a,
                                        _ => {
                                            topup_error.set(Some("Enter a valid amount in sats".to_string()));
                                            return;
                                        }
                                    };
                                    let api_key = state.read().routstr_api_key.clone().unwrap_or_default();
                                    topup_error.set(None);
                                    topup_loading.set(true);
                                    spawn(async move {
                                        match routstr::create_lightning_invoice(amount, "topup", Some(&api_key)).await {
                                            Ok(invoice) => {
                                                topup_invoice.set(Some(invoice));
                                            }
                                            Err(err) => {
                                                topup_error.set(Some(err));
                                            }
                                        }
                                        topup_loading.set(false);
                                    });
                                },
                                if *topup_loading.read() { "Creating..." } else { "Create Invoice" }
                            }
                        }
                        if let Some(err) = topup_error.read().as_ref() {
                            p { class: "text-xs text-red-600 dark:text-red-400", "{err}" }
                        }
                        if let Some(invoice) = topup_invoice_display.as_ref() {
                            if invoice.bolt11.is_some() {
                                div { class: "flex flex-col items-center gap-2 pt-2",
                                    div {
                                        class: "flex justify-center rounded-lg bg-white p-4",
                                        dangerous_inner_html: "{topup_qr_svg}",
                                    }
                                    p { class: "text-xs text-muted-foreground", "Pay the invoice to add funds" }
                                    div { class: "flex gap-2",
                                        button {
                                            r#type: "button",
                                            class: "rounded-lg border border-border px-3 py-1.5 text-xs transition hover:bg-accent",
                                            onclick: move |_| {
                                                let bolt11 = topup_invoice.read().as_ref().and_then(|inv| inv.bolt11.clone()).unwrap_or_default();
                                                spawn(async move {
                                                    let _ = platform::clipboard::copy_to_clipboard(&bolt11).await;
                                                });
                                            },
                                            "Copy"
                                        }
                                        button {
                                            r#type: "button",
                                            class: "rounded-lg border border-border px-3 py-1.5 text-xs transition hover:bg-accent",
                                            onclick: move |_| {
                                                let bolt11 = topup_invoice.read().as_ref().and_then(|inv| inv.bolt11.clone()).unwrap_or_default();
                                                spawn(async move {
                                                    let _ = platform::open_lightning_invoice(&bolt11).await;
                                                });
                                            },
                                            "Open Wallet"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Top up via Cashu
                    div { class: "rounded-lg border border-border p-4 space-y-3",
                        p { class: "text-sm font-medium text-foreground", "Top Up via Cashu Token" }
                        textarea {
                            class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary resize-y",
                            rows: "2",
                            placeholder: "cashuA...",
                            value: "{cashu_token_input}",
                            oninput: move |e| {
                                cashu_token_input.set(e.value());
                                cashu_error.set(None);
                                cashu_success.set(None);
                            },
                        }
                        if let Some(err) = cashu_error.read().as_ref() {
                            p { class: "text-xs text-red-600 dark:text-red-400", "{err}" }
                        }
                        if let Some(msg) = cashu_success.read().as_ref() {
                            p { class: "text-xs text-green-600 dark:text-green-400", "{msg}" }
                        }
                        button {
                            r#type: "button",
                            class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                            disabled: *cashu_loading.read() || cashu_token_input.read().trim().is_empty(),
                            onclick: move |_| {
                                let token = cashu_token_input.read().trim().to_string();
                                if token.is_empty() {
                                    return;
                                }
                                let api_key = state.read().routstr_api_key.clone().unwrap_or_default();
                                cashu_loading.set(true);
                                spawn(async move {
                                    match routstr::topup_with_cashu(&api_key, &token).await {
                                        Ok(result) => {
                                            let sats = result.msats.map(|m| m / 1000).unwrap_or(0);
                                            cashu_success.set(Some(format!("Added {} sats", sats)));
                                            cashu_error.set(None);
                                            cashu_token_input.set(String::new());
                                            // Refresh balance
                                            let key = state.read().routstr_api_key.clone().unwrap_or_default();
                                            load_routstr_balance(
                                                balance,
                                                balance_loading,
                                                balance_error,
                                                key,
                                                None,
                                            );
                                        }
                                        Err(err) => {
                                            cashu_error.set(Some(err));
                                        }
                                    }
                                    cashu_loading.set(false);
                                });
                            },
                            if *cashu_loading.read() { "Processing..." } else { "Top Up" }
                        }
                    }

                    // Refund / Withdraw
                    div { class: "rounded-lg border border-border p-4 space-y-3",
                        p { class: "text-sm font-medium text-foreground", "Withdraw" }
                        p { class: "text-xs text-muted-foreground",
                            "Withdraw your remaining balance as a Cashu token. The token can be claimed in any Cashu wallet."
                        }
                        button {
                            r#type: "button",
                            class: "rounded-lg border border-red-500/20 px-4 py-2 text-sm text-red-600 transition hover:bg-red-500/10 dark:text-red-400 disabled:opacity-60",
                            disabled: *refund_loading.read(),
                            onclick: move |_| {
                                let api_key = state.read().routstr_api_key.clone().unwrap_or_default();
                                refund_loading.set(true);
                                spawn(async move {
                                    match routstr::refund(&api_key).await {
                                        Ok(result) => {
                                            refund_result.set(Some(result));
                                            refund_error.set(None);
                                            // Refresh balance
                                            let key = state.read().routstr_api_key.clone().unwrap_or_default();
                                            load_routstr_balance(
                                                balance,
                                                balance_loading,
                                                balance_error,
                                                key,
                                                None,
                                            );
                                        }
                                        Err(err) => {
                                            refund_error.set(Some(err));
                                        }
                                    }
                                    refund_loading.set(false);
                                });
                            },
                            if *refund_loading.read() { "Processing..." } else { "Withdraw Remaining Balance" }
                        }
                        if let Some(err) = refund_error.read().as_ref() {
                            p { class: "text-xs text-red-600 dark:text-red-400", "{err}" }
                        }
                        if let Some(result) = refund_display.as_ref() {
                            if let Some(token) = result.token.as_ref() {
                                div { class: "rounded-lg bg-background border border-border p-3 space-y-2",
                                    p { class: "text-xs font-medium text-foreground", "Cashu Token" }
                                    p { class: "break-all text-xs text-muted-foreground", "{token}" }
                                    div { class: "flex items-center gap-2",
                                        button {
                                            r#type: "button",
                                            class: "rounded-lg border border-border px-3 py-1.5 text-xs transition hover:bg-accent",
                                            onclick: move |_| {
                                                let token = refund_result.read().as_ref().and_then(|r| r.token.clone()).unwrap_or_default();
                                                spawn(async move {
                                                    let _ = platform::clipboard::copy_to_clipboard(&token).await;
                                                });
                                            },
                                            "Copy Token"
                                        }
                                        if let Some(sats) = result.sats.or_else(|| result.msats.map(|m| m / 1000)) {
                                            span { class: "text-xs text-muted-foreground", "{sats} sats" }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Replace key
                    div { class: "pt-2",
                        button {
                            r#type: "button",
                            class: "text-xs text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| {
                                let mut next_state = state.read().clone();
                                next_state.routstr_api_key = None;
                                persist_routstr_state(
                                    next_state,
                                    state,
                                    is_saving,
                                    save_error,
                                    pending_save_snapshot,
                                    pending_save_min_event_id,
                                );
                                balance.set(None);
                                balance_error.set(None);
                                topup_invoice.set(None);
                                topup_error.set(None);
                                refund_result.set(None);
                                cashu_success.set(None);
                                cashu_error.set(None);
                            },
                            "Replace Key"
                        }
                    }
                }
            }
        }
    }
}

fn generate_invoice_qr_svg(invoice: &str) -> String {
    match QrCode::new(invoice.trim().to_uppercase()) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        Err(_) => "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"220\" height=\"220\" viewBox=\"0 0 220 220\"><rect width=\"220\" height=\"220\" fill=\"#ffffff\"/><text x=\"110\" y=\"110\" text-anchor=\"middle\" fill=\"#666666\" font-size=\"14\">QR Error</text></svg>".to_string(),
    }
}

fn load_routstr_balance(
    mut balance: Signal<Option<RoutstrBalance>>,
    mut balance_loading: Signal<bool>,
    mut balance_error: Signal<Option<String>>,
    api_key: String,
    request_generation: Option<(Signal<u32>, u32)>,
) {
    balance_loading.set(true);
    balance_error.set(None);
    spawn(async move {
        let current = || {
            request_generation
                .as_ref()
                .is_none_or(|(signal, generation)| *signal.peek() == *generation)
        };
        match routstr::get_balance(&api_key).await {
            Ok(result) => {
                if current() {
                    balance.set(Some(result));
                }
            }
            Err(err) => {
                if current() {
                    balance_error.set(Some(err));
                }
            }
        }
        if current() {
            balance_loading.set(false);
        }
    });
}

fn persist_routstr_state(
    next_state: AiProviderState,
    mut state: Signal<AiProviderState>,
    mut is_saving: Signal<bool>,
    mut save_error: Signal<Option<String>>,
    mut pending_save_snapshot: Signal<Option<AiProviderState>>,
    mut pending_save_min_event_id: Signal<u64>,
) {
    is_saving.set(true);
    save_error.set(None);
    if let Err(err) = ai_provider_store::cache_provider_state(&next_state) {
        save_error.set(Some(err));
        is_saving.set(false);
        return;
    }
    state.set(next_state.clone());
    pending_save_snapshot.set(Some(next_state.clone()));
    pending_save_min_event_id.set(
        PROVIDER_STATE_SAVE_EVENT
            .read()
            .as_ref()
            .map(|event| event.event_id)
            .unwrap_or(0),
    );
    if let Some(snapshot) = ai_provider_store::queue_provider_state_save(next_state) {
        ai_provider_store::process_queued_provider_state_saves(snapshot);
    }
}
