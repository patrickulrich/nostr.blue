use dioxus::prelude::*;
use futures::future::join_all;
use crate::stores::{
    cashu::{
        get_mints, create_melt_quote, melt_tokens,
        MeltProgress, MeltQuoteInfo, MELT_PROGRESS, WALLET_BALANCE,
        // MPP types and functions
        MppQuoteInfo, get_balances_per_mint, mint_supports_mpp,
        calculate_mpp_split, create_mpp_melt_quotes, execute_mpp_melt,
    },
    cashu_ws,
};
use crate::utils::shorten_url;

/// Payment mode for Lightning send
#[derive(Clone, Debug, PartialEq)]
enum PaymentMode {
    /// Single mint payment (default)
    Single,
    /// Multi-path payment across multiple mints (NUT-15)
    Mpp,
}

#[component]
pub fn CashuSendLightningModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut invoice = use_signal(|| String::new());
    let mints = get_mints();
    let mut selected_mint = use_signal(|| mints.first().cloned().unwrap_or_default());
    let mut is_creating_quote = use_signal(|| false);
    let mut is_paying = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut quote_info = use_signal(|| Option::<MeltQuoteInfo>::None);
    let mut payment_result = use_signal(|| Option::<(bool, Option<String>, u64)>::None);
    // Real-time melt status from NUT-17 WebSocket
    let mut melt_status = use_signal(|| Option::<String>::None);

    // MPP state
    let mut payment_mode = use_signal(|| PaymentMode::Single);
    let mut mpp_quote = use_signal(|| Option::<MppQuoteInfo>::None);
    let mut mpp_allocations = use_signal(|| Vec::<(String, u64)>::new());
    let mut mint_balances = use_signal(|| Vec::<(String, u64)>::new());
    let mut mpp_mint_balances = use_signal(|| Vec::<(String, u64)>::new()); // Only MPP-supporting mints

    // Read melt progress for UI updates
    let melt_progress = MELT_PROGRESS.read();

    // Load mint balances on mount and check MPP support
    use_effect(move || {
        spawn(async move {
            if let Ok(balances) = get_balances_per_mint().await {
                let all_balances: Vec<_> = balances.iter().map(|b| (b.mint_url.clone(), b.balance)).collect();
                mint_balances.set(all_balances.clone());

                // Check which mints support MPP in parallel (now faster with caching)
                let mpp_futures: Vec<_> = all_balances.iter()
                    .map(|(mint_url, balance)| {
                        let url = mint_url.clone();
                        let bal = *balance;
                        async move {
                            if mint_supports_mpp(&url).await {
                                Some((url, bal))
                            } else {
                                None
                            }
                        }
                    })
                    .collect();

                let results = join_all(mpp_futures).await;
                let mpp_balances: Vec<_> = results.into_iter().flatten().collect();
                mpp_mint_balances.set(mpp_balances);
            }
        });
    });

    // Keep selected_mint in sync with available mints
    use_effect(move || {
        let current_mints = get_mints();
        let current_selection = selected_mint.read().clone();

        if current_selection.is_empty() {
            if let Some(first_mint) = current_mints.first() {
                selected_mint.set(first_mint.clone());
            }
        } else if !current_mints.contains(&current_selection) {
            if let Some(first_mint) = current_mints.first() {
                selected_mint.set(first_mint.clone());
            } else {
                selected_mint.set(String::new());
            }
        }
    });

    // Get balance for selected mint
    let selected_mint_balance = {
        let mint = selected_mint.read().clone();
        mint_balances.read().iter()
            .find(|(url, _)| *url == mint)
            .map(|(_, b)| *b)
            .unwrap_or(0)
    };

    let on_create_quote = move |_| {
        let invoice_str = invoice.read().clone().trim().to_string();
        let mint = selected_mint.read().clone();
        let mode = payment_mode.read().clone();

        if invoice_str.is_empty() {
            error_message.set(Some("Please enter a lightning invoice".to_string()));
            return;
        }

        if !invoice_str.to_lowercase().starts_with("lnbc") && !invoice_str.to_lowercase().starts_with("lntb") {
            error_message.set(Some("Invalid lightning invoice format".to_string()));
            return;
        }

        is_creating_quote.set(true);
        error_message.set(None);
        payment_result.set(None);

        match mode {
            PaymentMode::Single => {
                if mint.is_empty() {
                    error_message.set(Some("Please select a mint".to_string()));
                    is_creating_quote.set(false);
                    return;
                }

                spawn(async move {
                    match create_melt_quote(mint, invoice_str).await {
                        Ok(q) => {
                            quote_info.set(Some(q));
                            mpp_quote.set(None);
                            is_creating_quote.set(false);
                        }
                        Err(e) => {
                            error_message.set(Some(format!("Failed to create quote: {}", e)));
                            is_creating_quote.set(false);
                        }
                    }
                });
            }
            PaymentMode::Mpp => {
                let allocations = mpp_allocations.read().clone();
                if allocations.is_empty() {
                    error_message.set(Some("No MPP allocations configured".to_string()));
                    is_creating_quote.set(false);
                    return;
                }

                spawn(async move {
                    match create_mpp_melt_quotes(invoice_str, allocations).await {
                        Ok(q) => {
                            mpp_quote.set(Some(q));
                            quote_info.set(None);
                            is_creating_quote.set(false);
                        }
                        Err(e) => {
                            error_message.set(Some(format!("Failed to create MPP quotes: {}", e)));
                            is_creating_quote.set(false);
                        }
                    }
                });
            }
        }
    };

    let on_pay = move |_| {
        // Early guard: prevent duplicate submissions if already paying
        if *is_paying.read() {
            return;
        }

        let mode = payment_mode.read().clone();

        match mode {
            PaymentMode::Single => {
                if let Some(q) = quote_info.read().as_ref() {
                    let quote_id = q.quote_id.clone();
                    let mint = q.mint_url.clone();

                    is_paying.set(true);
                    error_message.set(None);

                    // WebSocket subscription for real-time status updates (NUT-17)
                    let mint_for_ws = mint.clone();
                    let quote_id_for_ws = quote_id.clone();
                    melt_status.set(Some("Connecting...".to_string()));
                    spawn(async move {
                        if let Ok(mut rx) = cashu_ws::subscribe_to_quote(
                            mint_for_ws,
                            quote_id_for_ws,
                            cashu_ws::SubscriptionKind::Bolt11MeltQuote,
                        ).await {
                            melt_status.set(Some("Processing payment...".to_string()));
                            while let Some(status) = rx.recv().await {
                                match status {
                                    cashu_ws::QuoteStatus::Pending => {
                                        melt_status.set(Some("Payment pending...".to_string()));
                                    }
                                    cashu_ws::QuoteStatus::Paid => {
                                        melt_status.set(Some("Payment confirmed!".to_string()));
                                        break;
                                    }
                                    cashu_ws::QuoteStatus::Expired => {
                                        melt_status.set(Some("Quote expired".to_string()));
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        } else {
                            melt_status.set(Some("Processing...".to_string()));
                        }
                    });

                    spawn(async move {
                        match melt_tokens(mint, quote_id).await {
                            Ok((paid, preimage, fee)) => {
                                payment_result.set(Some((paid, preimage, fee)));
                                is_paying.set(false);
                                melt_status.set(None);
                                *MELT_PROGRESS.write() = None;

                                if paid {
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(3000).await;
                                        on_close.call(());
                                    });
                                }
                            }
                            Err(e) => {
                                error_message.set(Some(format!("Payment failed: {}", e)));
                                is_paying.set(false);
                                melt_status.set(None);
                                *MELT_PROGRESS.write() = None;
                            }
                        }
                    });
                }
            }
            PaymentMode::Mpp => {
                if let Some(q) = mpp_quote.read().as_ref() {
                    let contributions = q.contributions.clone();

                    is_paying.set(true);
                    error_message.set(None);
                    melt_status.set(Some("Processing MPP payment...".to_string()));

                    spawn(async move {
                        match execute_mpp_melt(contributions).await {
                            Ok(result) => {
                                payment_result.set(Some((
                                    result.paid,
                                    result.preimage,
                                    result.total_fee_paid,
                                )));
                                is_paying.set(false);
                                melt_status.set(None);
                                *MELT_PROGRESS.write() = None;

                                if result.paid {
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(3000).await;
                                        on_close.call(());
                                    });
                                }
                            }
                            Err(e) => {
                                error_message.set(Some(format!("MPP payment failed: {}", e)));
                                is_paying.set(false);
                                melt_status.set(None);
                                *MELT_PROGRESS.write() = None;
                            }
                        }
                    });
                }
            }
        }
    };

    // Auto-calculate MPP split (can be used for manual "Auto-split" button in future)
    // Only uses MPP-supporting mints
    let _on_auto_split = move |amount: u64| {
        let mpp_mints: Vec<String> = mpp_mint_balances.read().iter().map(|(url, _)| url.clone()).collect();
        spawn(async move {
            let include_mints = if mpp_mints.is_empty() { None } else { Some(mpp_mints) };
            match calculate_mpp_split(amount, include_mints).await {
                Ok(allocations) => {
                    mpp_allocations.set(allocations);
                }
                Err(e) => {
                    error_message.set(Some(format!("Cannot split payment: {}", e)));
                }
            }
        });
    };

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 {
                        class: "text-xl font-bold flex items-center gap-2",
                        span { "lightning" }
                        "Send Lightning"
                    }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "x"
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Payment result
                    if let Some((paid, preimage, fee)) = payment_result.read().as_ref() {
                        if *paid {
                            div {
                                class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4 space-y-2",
                                div {
                                    class: "flex items-start gap-3",
                                    div { class: "text-2xl", "+" }
                                    div {
                                        p {
                                            class: "text-sm font-semibold text-green-800 dark:text-green-200",
                                            if *payment_mode.read() == PaymentMode::Mpp {
                                                "MPP Payment successful!"
                                            } else {
                                                "Payment successful!"
                                            }
                                        }
                                        if let Some(pre) = preimage {
                                            p {
                                                class: "text-xs text-green-700 dark:text-green-300 mt-1 font-mono break-all",
                                                "Preimage: {pre}"
                                            }
                                        }
                                        p {
                                            class: "text-xs text-green-700 dark:text-green-300 mt-1",
                                            "Fee paid: {fee} sats"
                                        }
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "bg-yellow-50 dark:bg-yellow-950/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-4 space-y-2",
                                div {
                                    class: "flex items-start gap-3",
                                    div { class: "text-2xl", "..." }
                                    div {
                                        p {
                                            class: "text-sm font-semibold text-yellow-800 dark:text-yellow-200",
                                            "Payment pending or unpaid"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Error message
                    if let Some(msg) = error_message.read().as_ref() {
                        div {
                            class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div { class: "text-2xl", "!" }
                                div {
                                    p {
                                        class: "text-sm text-red-800 dark:text-red-200",
                                        "{msg}"
                                    }
                                }
                            }
                        }
                    }

                    // Single mint quote display
                    if let Some(q) = quote_info.read().as_ref() {
                        div {
                            class: "bg-accent/50 rounded-lg p-4 space-y-2",
                            h4 { class: "text-sm font-semibold mb-2", "Payment Details" }
                            div {
                                class: "space-y-2 text-sm",
                                div { class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Amount:" }
                                    span { class: "font-mono font-semibold", "{q.amount} sats" }
                                }
                                div { class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Fee reserve:" }
                                    span { class: "font-mono", "{q.fee_reserve} sats" }
                                }
                                div { class: "flex justify-between border-t border-border pt-2",
                                    span { class: "font-semibold", "Total:" }
                                    span { class: "font-mono font-semibold", "{q.amount + q.fee_reserve} sats" }
                                }
                            }
                        }
                    }

                    // MPP quote display
                    if let Some(q) = mpp_quote.read().as_ref() {
                        div {
                            class: "bg-accent/50 rounded-lg p-4 space-y-2",
                            h4 { class: "text-sm font-semibold mb-2", "MPP Payment Details (NUT-15)" }
                            div {
                                class: "space-y-2 text-sm",
                                // Show each mint contribution
                                for contrib in &q.contributions {
                                    div { class: "flex justify-between text-xs",
                                        span { class: "text-muted-foreground truncate max-w-[150px]", "{shorten_url(&contrib.mint_url, 30)}" }
                                        span { class: "font-mono", "{contrib.amount} + {contrib.fee_reserve} sats" }
                                    }
                                }
                                div { class: "flex justify-between border-t border-border pt-2",
                                    span { class: "text-muted-foreground", "Total amount:" }
                                    span { class: "font-mono font-semibold", "{q.total_amount} sats" }
                                }
                                div { class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Total fees:" }
                                    span { class: "font-mono", "{q.total_fee_reserve} sats" }
                                }
                                div { class: "flex justify-between border-t border-border pt-2",
                                    span { class: "font-semibold", "Grand Total:" }
                                    span { class: "font-mono font-semibold", "{q.total_amount + q.total_fee_reserve} sats" }
                                }
                            }
                        }
                    }

                    // Progress displays
                    if *is_creating_quote.read() {
                        div {
                            class: "bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded-lg p-4",
                            div {
                                class: "flex items-center justify-center text-sm text-blue-700 dark:text-blue-300",
                                if *payment_mode.read() == PaymentMode::Mpp {
                                    "Creating MPP quotes..."
                                } else {
                                    "Creating quote..."
                                }
                            }
                        }
                    }

                    if *is_paying.read() {
                        div {
                            class: "bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded-lg p-4",
                            h4 { class: "text-sm font-semibold mb-3 text-blue-800 dark:text-blue-200", "Payment Progress" }
                            div {
                                class: "space-y-2",
                                ProgressStep { label: "Preparing payment", is_active: matches!(*melt_progress, Some(MeltProgress::PreparingPayment)), is_complete: matches!(*melt_progress, Some(MeltProgress::PayingInvoice) | Some(MeltProgress::WaitingForConfirmation) | Some(MeltProgress::Completed { .. })) }
                                ProgressStep { label: "Sending to Lightning Network", is_active: matches!(*melt_progress, Some(MeltProgress::PayingInvoice)), is_complete: matches!(*melt_progress, Some(MeltProgress::WaitingForConfirmation) | Some(MeltProgress::Completed { .. })) }
                                ProgressStep { label: "Waiting for confirmation", is_active: matches!(*melt_progress, Some(MeltProgress::WaitingForConfirmation)), is_complete: matches!(*melt_progress, Some(MeltProgress::Completed { .. })) }
                            }
                            div {
                                class: "flex items-center justify-center text-sm text-blue-700 dark:text-blue-300 mt-4",
                                // Show NUT-17 WebSocket status if available
                                if let Some(status) = melt_status.read().as_ref() {
                                    span { class: "animate-pulse", "{status}" }
                                } else {
                                    span { "Processing..." }
                                }
                            }
                        }
                    }

                    // Invoice input (before quote)
                    if quote_info.read().is_none() && mpp_quote.read().is_none() && payment_result.read().is_none() {
                        div {
                            label { class: "block text-sm font-semibold mb-2", "Lightning Invoice" }
                            textarea {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg font-mono text-sm min-h-[100px]",
                                placeholder: "lnbc...",
                                value: invoice.read().clone(),
                                oninput: move |evt| invoice.set(evt.value())
                            }
                        }

                        // Payment mode toggle
                        if mints.len() > 1 {
                            div {
                                class: "flex items-center gap-3 p-3 bg-accent/30 rounded-lg",
                                input {
                                    r#type: "checkbox",
                                    id: "mpp-mode",
                                    class: "w-4 h-4 rounded border-border",
                                    checked: *payment_mode.read() == PaymentMode::Mpp,
                                    onchange: move |evt| {
                                        let is_mpp = evt.checked();
                                        payment_mode.set(if is_mpp { PaymentMode::Mpp } else { PaymentMode::Single });
                                        // Clear allocations when switching modes
                                        // User should configure allocations after getting quote
                                        if is_mpp {
                                            mpp_allocations.set(Vec::new());
                                        }
                                    }
                                }
                                div {
                                    class: "flex-1",
                                    label {
                                        r#for: "mpp-mode",
                                        class: "text-sm font-medium cursor-pointer",
                                        "Multi-path payment (NUT-15)"
                                    }
                                    p {
                                        class: "text-xs text-muted-foreground mt-1",
                                        "Split payment across multiple mints"
                                    }
                                }
                            }
                        }

                        // Single mint selection (when not MPP)
                        if *payment_mode.read() == PaymentMode::Single && !mints.is_empty() {
                            div {
                                label { class: "block text-sm font-semibold mb-2", "Pay from Mint" }
                                select {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg",
                                    value: selected_mint.read().clone(),
                                    onchange: move |evt| selected_mint.set(evt.value()),
                                    for mint_url in mints.iter() {
                                        option {
                                            value: mint_url.clone(),
                                            "{shorten_url(mint_url, 30)} ({get_mint_balance(&mint_balances.read(), mint_url)} sats)"
                                        }
                                    }
                                }
                                p {
                                    class: "text-xs text-muted-foreground mt-1",
                                    "Selected mint balance: {selected_mint_balance} sats"
                                }
                            }
                        }

                        // MPP mint balances display
                        if *payment_mode.read() == PaymentMode::Mpp {
                            div {
                                class: "space-y-2",
                                label { class: "block text-sm font-semibold", "MPP-Supported Mints" }

                                if mpp_mint_balances.read().is_empty() {
                                    div {
                                        class: "bg-yellow-50 dark:bg-yellow-950/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-3",
                                        p {
                                            class: "text-sm text-yellow-800 dark:text-yellow-200",
                                            "No mints support MPP (NUT-15). Add a mint that supports multi-path payments to use this feature."
                                        }
                                    }
                                } else {
                                    div {
                                        class: "bg-background border border-border rounded-lg p-3 space-y-2",
                                        for (mint_url, balance) in mpp_mint_balances.read().iter() {
                                            div {
                                                class: "flex justify-between text-sm",
                                                span { class: "text-muted-foreground truncate max-w-[200px]", "{shorten_url(mint_url, 30)}" }
                                                span { class: "font-mono", "{balance} sats" }
                                            }
                                        }
                                        div {
                                            class: "border-t border-border pt-2 flex justify-between text-sm font-semibold",
                                            span { "MPP Balance:" }
                                            span { class: "font-mono", "{mpp_mint_balances.read().iter().map(|(_, b)| b).sum::<u64>()} sats" }
                                        }
                                    }
                                }
                            }
                        }

                        // Balance info
                        div {
                            class: "text-sm text-muted-foreground",
                            "Total available: ",
                            span { class: "font-mono font-semibold", "{*WALLET_BALANCE.read()} sats" }
                        }
                    }
                }

                // Footer
                div {
                    class: "px-6 py-4 border-t border-border flex gap-3",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| {
                            quote_info.set(None);
                            mpp_quote.set(None);
                            payment_result.set(None);
                            error_message.set(None);
                            on_close.call(());
                        },
                        if payment_result.read().is_some() { "Close" } else { "Cancel" }
                    }

                    if payment_result.read().is_none() {
                        if quote_info.read().is_some() || mpp_quote.read().is_some() {
                            button {
                                class: if *is_paying.read() {
                                    "flex-1 px-4 py-3 bg-orange-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                } else {
                                    "flex-1 px-4 py-3 bg-orange-500 hover:bg-orange-600 text-white font-semibold rounded-lg transition"
                                },
                                disabled: *is_paying.read(),
                                onclick: on_pay,
                                if *is_paying.read() {
                                    "Paying..."
                                } else if *payment_mode.read() == PaymentMode::Mpp {
                                    "Pay with MPP"
                                } else {
                                    "Pay Invoice"
                                }
                            }
                        } else {
                            button {
                                class: if *is_creating_quote.read() || invoice.read().is_empty() {
                                    "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                } else {
                                    "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                                },
                                disabled: *is_creating_quote.read() || invoice.read().is_empty(),
                                onclick: on_create_quote,
                                if *is_creating_quote.read() {
                                    "Creating Quote..."
                                } else {
                                    "Continue"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Get balance for a specific mint from balances list
fn get_mint_balance(balances: &[(String, u64)], mint_url: &str) -> u64 {
    balances.iter()
        .find(|(url, _)| url == mint_url)
        .map(|(_, b)| *b)
        .unwrap_or(0)
}

/// Progress step indicator component
#[component]
fn ProgressStep(label: &'static str, is_active: bool, is_complete: bool) -> Element {
    let (icon, icon_class) = if is_complete {
        ("*", "text-green-500")
    } else if is_active {
        ("o", "text-blue-500 animate-pulse")
    } else {
        ("-", "text-muted-foreground")
    };

    let label_class = if is_active {
        "font-medium text-foreground"
    } else if is_complete {
        "text-green-700 dark:text-green-300"
    } else {
        "text-muted-foreground"
    };

    rsx! {
        div {
            class: "flex items-center gap-3",
            span { class: "{icon_class} text-sm", "{icon}" }
            span { class: "text-sm {label_class}", "{label}" }
        }
    }
}
