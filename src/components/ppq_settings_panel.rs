use crate::services::ppq::{
    self, PpqApiKey, PpqApiKeyInput, PpqBalance, PpqNwcAutoTopup, PpqTopupInvoice,
};
use crate::stores::ai_provider_store::{self, AiProviderState, PpqAccountState};
use crate::stores::nwc_store;
use dioxus::prelude::*;

#[component]
pub fn PpqSettingsPanel(
    state: Signal<AiProviderState>,
    is_saving: Signal<bool>,
    save_error: Signal<Option<String>>,
    provider_state_ready: bool,
) -> Element {
    let mut account_action_loading = use_signal(|| false);
    let mut balance = use_signal(|| None::<PpqBalance>);
    let balance_loading = use_signal(|| false);
    let balance_error = use_signal(|| None::<String>);
    let mut topup_amount = use_signal(|| "10".to_string());
    let mut topup_currency = use_signal(|| "USD".to_string());
    let mut topup_invoice = use_signal(|| None::<PpqTopupInvoice>);
    let mut topup_loading = use_signal(|| false);
    let mut topup_error = use_signal(|| None::<String>);
    let mut nwc = use_signal(|| None::<PpqNwcAutoTopup>);
    let mut nwc_loading = use_signal(|| false);
    let mut nwc_error = use_signal(|| None::<String>);
    let mut nwc_url = use_signal(String::new);
    let mut nwc_threshold = use_signal(|| "10".to_string());
    let mut nwc_topup_amount = use_signal(|| "10".to_string());
    let mut api_keys = use_signal(Vec::<PpqApiKey>::new);
    let keys_loading = use_signal(|| false);
    let mut keys_error = use_signal(|| None::<String>);
    let mut editing_key_id = use_signal(|| None::<String>);
    let mut key_name = use_signal(String::new);
    let mut key_usage_limit = use_signal(String::new);
    let mut key_reset_period = use_signal(String::new);
    let mut key_expire_at = use_signal(String::new);
    let mut key_form_loading = use_signal(|| false);
    let mut key_form_error = use_signal(|| None::<String>);
    let mut last_loaded_credit_id = use_signal(|| None::<String>);

    use_effect(move || {
        if !provider_state_ready {
            return;
        }

        let current_credit_id = state
            .read()
            .ppq_account
            .as_ref()
            .map(|account| account.credit_id.clone());
        if last_loaded_credit_id.read().as_ref() == current_credit_id.as_ref() {
            return;
        }

        last_loaded_credit_id.set(current_credit_id.clone());
        balance.set(None);
        api_keys.set(Vec::new());
        nwc.set(None);
        topup_invoice.set(None);
        topup_error.set(None);

        let Some(credit_id) = current_credit_id else {
            return;
        };

        load_balance(balance, balance_loading, balance_error, credit_id.clone());
        load_api_keys(api_keys, keys_loading, keys_error, credit_id.clone());
        load_nwc(nwc, nwc_loading, nwc_error, credit_id);
    });

    let ppq_account = state.read().ppq_account.clone();
    let active_key_id = ppq_account
        .as_ref()
        .and_then(|account| account.active_api_key_id.clone());
    let api_keys_list = api_keys.read().clone();
    let account_credit_id = ppq_account
        .as_ref()
        .map(|account| account.credit_id.clone())
        .unwrap_or_default();
    let account_api_key = ppq_account
        .as_ref()
        .map(|account| account.api_key.clone())
        .unwrap_or_default();
    let main_nwc_status = nwc_store::NWC_STATUS.read().clone();
    let main_nwc_uri = nwc_store::current_nwc_uri();
    let refresh_balance_credit_id = account_credit_id.clone();
    let refresh_keys_credit_id = account_credit_id.clone();
    let refresh_nwc_credit_id = account_credit_id.clone();
    let create_topup_api_key = account_api_key.clone();
    let create_topup_credit_id = account_credit_id.clone();
    let refresh_topup_api_key = account_api_key.clone();
    let refresh_topup_credit_id = account_credit_id.clone();
    let connect_nwc_credit_id = account_credit_id.clone();
    let disconnect_nwc_credit_id = account_credit_id.clone();
    let key_form_credit_id = account_credit_id.clone();

    rsx! {
        div { class: "rounded-xl border border-border bg-card p-6 space-y-6",
            div {
                h3 { class: "text-base font-semibold text-foreground", "Built-In PPQ Account" }
                p { class: "mt-1 text-sm text-muted-foreground",
                    "PPQ is the default built-in AI provider. Create or manage the local PPQ account used for chat, funding, auto-topup, and managed API keys."
                }
            }

            if let Some(account) = ppq_account.clone() {
                div { class: "grid gap-4 md:grid-cols-2",
                    InfoCard {
                        title: "Credit ID".to_string(),
                        value: account.credit_id.clone(),
                        subtle: active_key_id
                            .as_ref()
                            .map(|id| format!("Active chat key: {}", id))
                            .unwrap_or_else(|| "Active chat key: account default".to_string()),
                    }
                    InfoCard {
                        title: "Balance".to_string(),
                        value: balance
                            .read()
                            .as_ref()
                            .and_then(|item| item.amount.map(|amount| format!("{:.4} {}", amount, item.currency)))
                            .unwrap_or_else(|| "Unknown".to_string()),
                        subtle: if *balance_loading.read() {
                            "Refreshing balance...".to_string()
                        } else {
                            "Refresh to query PPQ credits/balance".to_string()
                        },
                    }
                }

                div { class: "flex flex-wrap gap-3",
                    button {
                        class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                        disabled: *account_action_loading.read() || *is_saving.read(),
                        onclick: move |_| {
                            load_balance(
                                balance,
                                balance_loading,
                                balance_error,
                                refresh_balance_credit_id.clone(),
                            );
                        },
                        "Refresh Balance"
                    }
                    button {
                        class: "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent disabled:opacity-60",
                        disabled: *account_action_loading.read() || *is_saving.read(),
                        onclick: move |_| {
                            load_api_keys(
                                api_keys,
                                keys_loading,
                                keys_error,
                                refresh_keys_credit_id.clone(),
                            );
                        },
                        "Refresh Keys"
                    }
                    button {
                        class: "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent disabled:opacity-60",
                        disabled: *account_action_loading.read() || *is_saving.read(),
                        onclick: move |_| {
                            load_nwc(
                                nwc,
                                nwc_loading,
                                nwc_error,
                                refresh_nwc_credit_id.clone(),
                            );
                        },
                        "Refresh NWC"
                    }
                }
            } else {
                div { class: "rounded-xl border border-dashed border-border bg-background p-5 text-sm text-muted-foreground space-y-3",
                    p { "No PPQ account is configured on this device yet." }
                    p { "Create one here to use the built-in default provider, or skip PPQ and keep using your own custom OpenAI-compatible provider below." }
                    button {
                        class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                        disabled: *account_action_loading.read() || *is_saving.read() || !provider_state_ready,
                        onclick: move |_| {
                            if *account_action_loading.read() {
                                return;
                            }
                            account_action_loading.set(true);
                            save_error.set(None);
                            spawn(async move {
                                match ppq::create_account().await {
                                    Ok(created_account) => {
                                        let mut next_state = state.read().clone();
                                        next_state.ppq_account = Some(PpqAccountState {
                                            credit_id: created_account.credit_id,
                                            api_key: created_account.api_key,
                                            active_api_key_id: None,
                                        });
                                        match ai_provider_store::save_provider_state(&next_state).await {
                                            Ok(()) => state.set(next_state),
                                            Err(err) => save_error.set(Some(err)),
                                        }
                                    }
                                    Err(err) => save_error.set(Some(err)),
                                }
                                account_action_loading.set(false);
                            });
                        },
                        if *account_action_loading.read() { "Creating PPQ Account..." } else { "Create PPQ Account" }
                    }
                }
            }

            if let Some(err) = save_error.read().as_ref() {
                p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
            }

            if let Some(account) = ppq_account.clone() {
                div { class: "rounded-xl border border-border p-4 space-y-4",
                    h4 { class: "font-medium text-foreground", "Deposit Funds" }
                    p { class: "text-sm text-muted-foreground",
                        "Create a PPQ Lightning invoice. Only Bitcoin Lightning deposits are supported here. The current built-in chat key is used to authenticate the topup request."
                    }
                    div { class: "grid gap-4 md:grid-cols-2",
                        label { class: "space-y-2",
                            span { class: "text-sm font-medium text-foreground", "Amount" }
                            input {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                value: "{topup_amount}",
                                disabled: *topup_loading.read(),
                                oninput: move |evt| topup_amount.set(evt.value()),
                            }
                        }
                        label { class: "space-y-2",
                            span { class: "text-sm font-medium text-foreground", "Currency" }
                            input {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                value: "{topup_currency}",
                                disabled: *topup_loading.read(),
                                oninput: move |evt| topup_currency.set(evt.value().to_uppercase()),
                            }
                        }
                    }
                    p { class: "text-xs text-muted-foreground", "Payment method: Bitcoin Lightning. Supported currencies: USD, BTC, SATS." }
                    div { class: "flex flex-wrap gap-3",
                        button {
                            class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                            disabled: *topup_loading.read(),
                            onclick: move |_| {
                                let amount = parse_optional_f64(&topup_amount.read()).unwrap_or(0.0);
                                if amount <= 0.0 {
                                    topup_error.set(Some("Topup amount must be greater than 0".to_string()));
                                    return;
                                }
                                if create_topup_api_key.trim().is_empty() {
                                    topup_error.set(Some("PPQ account is missing an API key".to_string()));
                                    return;
                                }
                                topup_loading.set(true);
                                topup_error.set(None);
                                let currency = topup_currency.read().trim().to_uppercase();
                                let api_key = create_topup_api_key.clone();
                                let credit_id = create_topup_credit_id.clone();
                                spawn(async move {
                                    match ppq::create_topup_invoice(&api_key, "btc-lightning", amount, &currency).await {
                                        Ok(invoice) => {
                                            topup_invoice.set(Some(invoice));
                                            load_balance(balance, balance_loading, balance_error, credit_id);
                                        }
                                        Err(err) => topup_error.set(Some(err)),
                                    }
                                    topup_loading.set(false);
                                });
                            },
                            if *topup_loading.read() { "Creating Invoice..." } else { "Create Invoice" }
                        }
                        button {
                            class: "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent disabled:opacity-60",
                            disabled: *topup_loading.read() || topup_invoice.read().is_none(),
                            onclick: move |_| {
                                let Some(invoice_id) = topup_invoice.read().as_ref().map(|invoice| invoice.invoice_id.clone()) else {
                                    return;
                                };
                                if invoice_id.is_empty() {
                                    topup_error.set(Some("Current invoice has no invoice id".to_string()));
                                    return;
                                }
                                topup_loading.set(true);
                                let api_key = refresh_topup_api_key.clone();
                                let credit_id = refresh_topup_credit_id.clone();
                                spawn(async move {
                                    match ppq::get_topup_status(&api_key, &invoice_id).await {
                                        Ok(invoice) => {
                                            topup_invoice.set(Some(invoice));
                                            load_balance(balance, balance_loading, balance_error, credit_id);
                                        }
                                        Err(err) => topup_error.set(Some(err)),
                                    }
                                    topup_loading.set(false);
                                });
                            },
                            "Refresh Invoice Status"
                        }
                    }
                    if let Some(err) = topup_error.read().as_ref() {
                        p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                    }
                    if let Some(invoice) = topup_invoice.read().as_ref() {
                        InvoiceCard { invoice: invoice.clone() }
                    }
                }

                div { class: "rounded-xl border border-border p-4 space-y-4",
                    h4 { class: "font-medium text-foreground", "NWC Auto-Topup" }
                    p { class: "text-sm text-muted-foreground",
                        "Connect PPQ auto-topup to your site-wide Nostr Wallet Connect wallet, or paste a wallet URI manually if you want PPQ to use a different wallet."
                    }
                    div { class: "rounded-lg bg-muted/60 p-3 text-sm space-y-2",
                        p { class: "font-medium text-foreground", "Site Wallet" }
                        p {
                            class: "text-muted-foreground",
                            match main_nwc_status {
                                nwc_store::ConnectionStatus::Connected => "A site-wide NWC wallet is connected and available to reuse.".to_string(),
                                nwc_store::ConnectionStatus::Connecting => "The site-wide NWC wallet is connecting.".to_string(),
                                nwc_store::ConnectionStatus::Error(ref err) => format!("The site-wide NWC wallet is in an error state: {}", err),
                                nwc_store::ConnectionStatus::Disconnected => "No site-wide NWC wallet is connected right now.".to_string(),
                            }
                        }
                        if let Some(uri) = main_nwc_uri.as_ref() {
                            p { class: "break-all text-xs text-muted-foreground", "{uri}" }
                        }
                        button {
                            class: if main_nwc_uri.is_some() && !*nwc_loading.read() {
                                "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent"
                            } else {
                                "rounded-lg border border-border px-4 py-2 text-sm opacity-60 cursor-not-allowed"
                            },
                            disabled: main_nwc_uri.is_none() || *nwc_loading.read(),
                            onclick: move |_| {
                                if let Some(uri) = nwc_store::current_nwc_uri() {
                                    nwc_url.set(uri);
                                    nwc_error.set(None);
                                }
                            },
                            "Use Connected Site Wallet"
                        }
                    }
                    div { class: "grid gap-4 md:grid-cols-3",
                        label { class: "space-y-2 md:col-span-3",
                            span { class: "text-sm font-medium text-foreground", "NWC URL" }
                            input {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                value: "{nwc_url}",
                                disabled: *nwc_loading.read(),
                                oninput: move |evt| nwc_url.set(evt.value()),
                            }
                        }
                        label { class: "space-y-2",
                            span { class: "text-sm font-medium text-foreground", "Threshold USD" }
                            input {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                value: "{nwc_threshold}",
                                disabled: *nwc_loading.read(),
                                oninput: move |evt| nwc_threshold.set(evt.value()),
                            }
                        }
                        label { class: "space-y-2",
                            span { class: "text-sm font-medium text-foreground", "Topup USD" }
                            input {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                value: "{nwc_topup_amount}",
                                disabled: *nwc_loading.read(),
                                oninput: move |evt| nwc_topup_amount.set(evt.value()),
                            }
                        }
                    }
                    div { class: "flex flex-wrap gap-3",
                        button {
                            class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                            disabled: *nwc_loading.read(),
                            onclick: move |_| {
                                let nwc_url_value = nwc_url.read().trim().to_string();
                                if nwc_url_value.is_empty() {
                                    nwc_error.set(Some("NWC URL is required".to_string()));
                                    return;
                                }
                                nwc_loading.set(true);
                                nwc_error.set(None);
                                let threshold_usd = parse_optional_f64(&nwc_threshold.read());
                                let topup_amount_usd = parse_optional_f64(&nwc_topup_amount.read());
                                let credit_id = connect_nwc_credit_id.clone();
                                spawn(async move {
                                    match ppq::connect_nwc_auto_topup(&credit_id, &nwc_url_value, threshold_usd, topup_amount_usd).await {
                                        Ok(next_nwc) => nwc.set(Some(next_nwc)),
                                        Err(err) => nwc_error.set(Some(err)),
                                    }
                                    nwc_loading.set(false);
                                });
                            },
                            if *nwc_loading.read() { "Saving..." } else { "Connect / Update" }
                        }
                        button {
                            class: "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent disabled:opacity-60",
                            disabled: *nwc_loading.read() || nwc.read().is_none(),
                            onclick: move |_| {
                                nwc_loading.set(true);
                                nwc_error.set(None);
                                let credit_id = disconnect_nwc_credit_id.clone();
                                spawn(async move {
                                    match ppq::disconnect_nwc_auto_topup(&credit_id).await {
                                        Ok(()) => nwc.set(None),
                                        Err(err) => nwc_error.set(Some(err)),
                                    }
                                    nwc_loading.set(false);
                                });
                            },
                            "Disconnect"
                        }
                    }
                    if let Some(err) = nwc_error.read().as_ref() {
                        p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                    }
                    if let Some(nwc_state) = nwc.read().as_ref() {
                        div { class: "rounded-lg bg-muted/60 p-3 text-sm space-y-1",
                            p { class: "font-medium text-foreground", "Connected" }
                            p {
                                "Threshold: "
                                {nwc_state
                                    .threshold_usd
                                    .map(|value| format!("${:.2}", value))
                                    .unwrap_or_else(|| "Unknown".to_string())}
                            }
                            p {
                                "Topup amount: "
                                {nwc_state
                                    .topup_amount_usd
                                    .map(|value| format!("${:.2}", value))
                                    .unwrap_or_else(|| "Unknown".to_string())}
                            }
                            if let Some(url) = nwc_state.nwc_url.as_ref() {
                                p { class: "break-all text-muted-foreground", "{url}" }
                            }
                        }
                    }
                }

                div { class: "rounded-xl border border-border p-4 space-y-4",
                    h4 { class: "font-medium text-foreground", "Managed PPQ API Keys" }
                    p { class: "text-sm text-muted-foreground",
                        "Create, edit, revoke, and activate managed PPQ API keys. The original PPQ account key created during onboarding is used by default and is not listed here unless you create additional managed keys."
                    }
                    div { class: "grid gap-4 md:grid-cols-2",
                        label { class: "space-y-2",
                            span { class: "text-sm font-medium text-foreground", "Key Name" }
                            input {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                value: "{key_name}",
                                disabled: *key_form_loading.read(),
                                oninput: move |evt| key_name.set(evt.value()),
                            }
                        }
                        label { class: "space-y-2",
                            span { class: "text-sm font-medium text-foreground", "Usage Limit USD" }
                            input {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                value: "{key_usage_limit}",
                                disabled: *key_form_loading.read(),
                                oninput: move |evt| key_usage_limit.set(evt.value()),
                            }
                        }
                        label { class: "space-y-2",
                            span { class: "text-sm font-medium text-foreground", "Reset Period" }
                            select {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                value: "{key_reset_period}",
                                disabled: *key_form_loading.read(),
                                onchange: move |evt| key_reset_period.set(evt.value()),
                                option { value: "", "None" }
                                option { value: "daily", "Daily" }
                                option { value: "weekly", "Weekly" }
                                option { value: "monthly", "Monthly" }
                            }
                        }
                        label { class: "space-y-2",
                            span { class: "text-sm font-medium text-foreground", "Expire At (ISO 8601)" }
                            input {
                                class: "h-10 w-full rounded-lg border border-border bg-background px-3 text-sm",
                                placeholder: "2027-01-01T00:00:00Z",
                                value: "{key_expire_at}",
                                disabled: *key_form_loading.read(),
                                oninput: move |evt| key_expire_at.set(evt.value()),
                            }
                        }
                    }
                    if let Some(err) = key_form_error.read().as_ref() {
                        p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                    }
                    div { class: "flex flex-wrap gap-3",
                        button {
                            class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                            disabled: *key_form_loading.read(),
                            onclick: move |_| {
                                let name = key_name.read().trim().to_string();
                                if name.is_empty() {
                                    key_form_error.set(Some("Key name is required".to_string()));
                                    return;
                                }
                                let usage_limit_usd = parse_optional_f64(&key_usage_limit.read());
                                let reset_period = empty_to_none(key_reset_period.read().clone());
                                if usage_limit_usd.is_none() && reset_period.is_some() {
                                    key_form_error.set(Some("Reset period requires a usage limit".to_string()));
                                    return;
                                }
                                key_form_error.set(None);
                                key_form_loading.set(true);
                                let credit_id = key_form_credit_id.clone();
                                let editing = editing_key_id.read().clone();
                                let input = PpqApiKeyInput {
                                    name,
                                    usage_limit_usd,
                                    reset_period,
                                    expire_at: empty_to_none(key_expire_at.read().clone()),
                                };
                                spawn(async move {
                                    let result = if let Some(key_id) = editing.as_deref() {
                                        ppq::update_api_key(&credit_id, key_id, &input).await
                                    } else {
                                        ppq::create_api_key(&credit_id, &input).await
                                    };
                                    match result {
                                        Ok(created_or_updated) => {
                                            editing_key_id.set(None);
                                            key_name.set(String::new());
                                            key_usage_limit.set(String::new());
                                            key_reset_period.set(String::new());
                                            key_expire_at.set(String::new());
                                            load_api_keys(api_keys, keys_loading, keys_error, credit_id.clone());
                                            if created_or_updated.api_key.is_some() {
                                                set_active_ppq_key(
                                                    state,
                                                    is_saving,
                                                    save_error,
                                                    created_or_updated.id.clone(),
                                                    created_or_updated.api_key.clone().unwrap_or_default(),
                                                );
                                            }
                                        }
                                        Err(err) => key_form_error.set(Some(err)),
                                    }
                                    key_form_loading.set(false);
                                });
                            },
                            if *key_form_loading.read() {
                                "Saving Key..."
                            } else if editing_key_id.read().is_some() {
                                "Save Key"
                            } else {
                                "Create Key"
                            }
                        }
                        if editing_key_id.read().is_some() {
                            button {
                                class: "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent",
                                disabled: *key_form_loading.read(),
                                onclick: move |_| {
                                    editing_key_id.set(None);
                                    key_name.set(String::new());
                                    key_usage_limit.set(String::new());
                                    key_reset_period.set(String::new());
                                    key_expire_at.set(String::new());
                                    key_form_error.set(None);
                                },
                                "Cancel Edit"
                            }
                        }
                    }
                    if let Some(err) = keys_error.read().as_ref() {
                        p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                    }
                    if api_keys_list.is_empty() {
                        p { class: "text-sm text-muted-foreground",
                            if *keys_loading.read() { "Loading keys..." } else { "No managed PPQ API keys yet." }
                        }
                    } else {
                        div { class: "space-y-3",
                            for key in api_keys_list.clone() {
                                {
                                    let is_active = active_key_id.as_deref() == Some(key.id.as_str());
                                    let key_clone = key.clone();
                                    let key_for_edit = key.clone();
                                    let key_credit_id_for_use = account.credit_id.clone();
                                    let key_credit_id_for_revoke = account.credit_id.clone();
                                    let active_key_id_for_revoke = active_key_id.clone();
                                    rsx! {
                                        div { key: "{key.id}", class: "rounded-lg border border-border p-3 space-y-3",
                                            div { class: "flex flex-wrap items-start justify-between gap-3",
                                                div {
                                                    div { class: "flex items-center gap-2",
                                                        p { class: "font-medium text-foreground", "{key.name}" }
                                                        if is_active {
                                                            span { class: "rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary", "Active for Chat" }
                                                        }
                                                        if key.deleted_at.is_some() {
                                                            span { class: "rounded-full bg-red-500/10 px-2 py-0.5 text-xs font-medium text-red-600 dark:text-red-400", "Revoked" }
                                                        }
                                                    }
                                                    p { class: "text-xs text-muted-foreground break-all", "{key.id}" }
                                                    p { class: "text-xs text-muted-foreground",
                                                        "Limit: "
                                                        {key
                                                            .usage_limit_usd
                                                            .map(|value| format!("${:.2}", value))
                                                            .unwrap_or_else(|| "Unlimited".to_string())}
                                                        " · Reset: "
                                                        {key.reset_period.clone().unwrap_or_else(|| "None".to_string())}
                                                    }
                                                    p { class: "text-xs text-muted-foreground",
                                                        "Current period: "
                                                        {key
                                                            .current_period_usage_usd
                                                            .map(|value| format!("${:.2}", value))
                                                            .unwrap_or_else(|| "$0.00".to_string())}
                                                        " · All time: "
                                                        {key
                                                            .total_usage_all_time_usd
                                                            .map(|value| format!("${:.2}", value))
                                                            .unwrap_or_else(|| "$0.00".to_string())}
                                                    }
                                                }
                                                div { class: "flex flex-wrap gap-2",
                                                    button {
                                                        class: "rounded-lg border border-border px-3 py-2 text-xs transition hover:bg-accent disabled:opacity-60",
                                                        disabled: key.deleted_at.is_some() || *is_saving.read(),
                                                        onclick: move |_| {
                                                            let credit_id = key_credit_id_for_use.clone();
                                                            let key_id = key_clone.id.clone();
                                                            keys_error.set(None);
                                                            spawn(async move {
                                                                match ppq::get_api_key(&credit_id, &key_id, true).await {
                                                                    Ok(full_key) => {
                                                                        if let Some(api_key) = full_key.api_key.as_ref() {
                                                                            set_active_ppq_key(
                                                                                state,
                                                                                is_saving,
                                                                                save_error,
                                                                                full_key.id.clone(),
                                                                                api_key.clone(),
                                                                            );
                                                                        } else {
                                                                            keys_error.set(Some("PPQ did not return the full API key for this key id".to_string()));
                                                                        }
                                                                    }
                                                                    Err(err) => keys_error.set(Some(err)),
                                                                }
                                                            });
                                                        },
                                                        "Use for Chat"
                                                    }
                                                    button {
                                                        class: "rounded-lg border border-border px-3 py-2 text-xs transition hover:bg-accent disabled:opacity-60",
                                                        disabled: key.deleted_at.is_some(),
                                                        onclick: move |_| {
                                                            editing_key_id.set(Some(key_for_edit.id.clone()));
                                                            key_name.set(key_for_edit.name.clone());
                                                            key_usage_limit.set(key_for_edit.usage_limit_usd.map(|value| value.to_string()).unwrap_or_default());
                                                            key_reset_period.set(key_for_edit.reset_period.clone().unwrap_or_default());
                                                            key_expire_at.set(key_for_edit.expire_at.clone().unwrap_or_default());
                                                            key_form_error.set(None);
                                                        },
                                                        "Edit"
                                                    }
                                                    button {
                                                        class: "rounded-lg border border-red-500/20 px-3 py-2 text-xs text-red-600 transition hover:bg-red-500/10 dark:text-red-400 disabled:opacity-60",
                                                        disabled: key.deleted_at.is_some(),
                                                        onclick: move |_| {
                                                            let credit_id = key_credit_id_for_revoke.clone();
                                                            let key_id = key.id.clone();
                                                            let active_key_id_for_revoke = active_key_id_for_revoke.clone();
                                                            spawn(async move {
                                                                match ppq::delete_api_key(&credit_id, &key_id).await {
                                                                    Ok(()) => {
                                                                        if active_key_id_for_revoke.as_deref() == Some(key_id.as_str()) {
                                                                            clear_active_ppq_key(state, is_saving, save_error);
                                                                        }
                                                                        load_api_keys(api_keys, keys_loading, keys_error, credit_id);
                                                                    }
                                                                    Err(err) => keys_error.set(Some(err)),
                                                                }
                                                            });
                                                        },
                                                        "Revoke"
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
            }
        }
    }
}

#[component]
fn InfoCard(title: String, value: String, subtle: String) -> Element {
    rsx! {
        div { class: "rounded-xl border border-border bg-background p-4",
            p { class: "text-xs uppercase tracking-wide text-muted-foreground", "{title}" }
            p { class: "mt-2 break-all text-sm font-medium text-foreground", "{value}" }
            p { class: "mt-1 text-xs text-muted-foreground", "{subtle}" }
        }
    }
}

#[component]
fn InvoiceCard(invoice: PpqTopupInvoice) -> Element {
    rsx! {
        div { class: "rounded-lg bg-muted/60 p-3 text-sm space-y-2",
            p { class: "font-medium text-foreground", "Current Invoice" }
            p { class: "break-all", "Invoice ID: {invoice.invoice_id}" }
            if let Some(status) = invoice.status.as_ref() {
                p { "Status: {status}" }
            }
            if let Some(payment_request) = invoice.payment_request.as_ref() {
                p { class: "break-all", "Payment request: {payment_request}" }
            }
            if let Some(address) = invoice.address.as_ref() {
                p { class: "break-all", "Address: {address}" }
            }
            if let Some(amount) = invoice.amount {
                p { "Amount: {amount}" }
            }
            if let Some(currency) = invoice.currency.as_ref() {
                p { "Currency: {currency}" }
            }
            details {
                summary { class: "cursor-pointer text-xs text-muted-foreground", "Raw response" }
                pre { class: "mt-2 overflow-x-auto rounded-lg bg-background p-3 text-xs text-muted-foreground", "{invoice.raw_json}" }
            }
        }
    }
}

fn load_balance(
    mut balance: Signal<Option<PpqBalance>>,
    mut balance_loading: Signal<bool>,
    mut balance_error: Signal<Option<String>>,
    credit_id: String,
) {
    balance_loading.set(true);
    balance_error.set(None);
    spawn(async move {
        match ppq::get_balance(&credit_id).await {
            Ok(next_balance) => balance.set(Some(next_balance)),
            Err(err) => balance_error.set(Some(err)),
        }
        balance_loading.set(false);
    });
}

fn load_api_keys(
    mut api_keys: Signal<Vec<PpqApiKey>>,
    mut keys_loading: Signal<bool>,
    mut keys_error: Signal<Option<String>>,
    credit_id: String,
) {
    keys_loading.set(true);
    keys_error.set(None);
    spawn(async move {
        match ppq::list_api_keys(&credit_id).await {
            Ok(keys) => api_keys.set(keys),
            Err(err) => keys_error.set(Some(err)),
        }
        keys_loading.set(false);
    });
}

fn load_nwc(
    mut nwc: Signal<Option<PpqNwcAutoTopup>>,
    mut nwc_loading: Signal<bool>,
    mut nwc_error: Signal<Option<String>>,
    credit_id: String,
) {
    nwc_loading.set(true);
    nwc_error.set(None);
    spawn(async move {
        match ppq::get_nwc_auto_topup(&credit_id).await {
            Ok(next_nwc) => nwc.set(next_nwc),
            Err(err) => nwc_error.set(Some(err)),
        }
        nwc_loading.set(false);
    });
}

fn set_active_ppq_key(
    state: Signal<AiProviderState>,
    is_saving: Signal<bool>,
    mut save_error: Signal<Option<String>>,
    key_id: String,
    api_key: String,
) {
    let mut next_state = state.read().clone();
    let Some(account) = next_state.ppq_account.as_mut() else {
        save_error.set(Some("PPQ account is not configured".to_string()));
        return;
    };
    account.active_api_key_id = Some(key_id);
    account.api_key = api_key;
    persist_state(next_state, state, is_saving, save_error);
}

fn clear_active_ppq_key(
    state: Signal<AiProviderState>,
    is_saving: Signal<bool>,
    save_error: Signal<Option<String>>,
) {
    let mut next_state = state.read().clone();
    let Some(account) = next_state.ppq_account.as_mut() else {
        return;
    };
    account.active_api_key_id = None;
    account.api_key.clear();
    persist_state(next_state, state, is_saving, save_error);
}

fn persist_state(
    next_state: AiProviderState,
    mut state: Signal<AiProviderState>,
    mut is_saving: Signal<bool>,
    mut save_error: Signal<Option<String>>,
) {
    is_saving.set(true);
    save_error.set(None);
    if let Err(err) = ai_provider_store::cache_provider_state(&next_state) {
        save_error.set(Some(err));
        is_saving.set(false);
        return;
    }
    spawn(async move {
        match ai_provider_store::save_provider_state(&next_state).await {
            Ok(()) => state.set(next_state),
            Err(err) => save_error.set(Some(err)),
        }
        is_saving.set(false);
    });
}

fn parse_optional_f64(input: &str) -> Option<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<f64>().ok()
    }
}

fn empty_to_none(input: String) -> Option<String> {
    let trimmed = input.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
