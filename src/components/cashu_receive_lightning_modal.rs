use dioxus::prelude::*;
use crate::stores::{cashu_wallet, cashu_ws};
use crate::utils::shorten_url;
use qrcode::{QrCode, render::svg};

#[component]
pub fn CashuReceiveLightningModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut amount = use_signal(|| String::new());
    let mints = cashu_wallet::get_mints();
    let mut selected_mint = use_signal(|| mints.first().cloned().unwrap_or_default());
    let mut is_generating = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut quote_info = use_signal(|| Option::<cashu_wallet::MintQuoteInfo>::None);
    let mut is_polling = use_signal(|| false);
    let mut success_message = use_signal(|| Option::<String>::None);

    // Keep selected_mint in sync with available mints
    use_effect(move || {
        let current_mints = cashu_wallet::get_mints();
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

    // Polling for payment with WebSocket support (NUT-17)
    use_effect(move || {
        if let Some(q) = quote_info.read().as_ref() {
            if !*is_polling.read() && success_message.read().is_none() {
                let quote_id = q.quote_id.clone();
                let mint_url = q.mint_url.clone();

                is_polling.set(true);

                // Clone reactive handles into the async task to observe cancellation
                let is_polling_clone = is_polling.clone();
                let quote_info_clone = quote_info.clone();

                spawn(async move {
                    // Try WebSocket subscription first (NUT-17)
                    let ws_result = cashu_ws::subscribe_to_quote(
                        mint_url.clone(),
                        quote_id.clone(),
                        cashu_ws::SubscriptionKind::Bolt11MintQuote,
                    ).await;

                    match ws_result {
                        Ok(mut rx) => {
                            log::info!("Using WebSocket for quote status updates");

                            // WebSocket-based waiting
                            let timeout_secs = 600; // 10 minutes
                            let start = instant::Instant::now();

                            loop {
                                // Check for cancellation
                                if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                    log::info!("WebSocket polling cancelled");
                                    break;
                                }

                                // Check timeout
                                if start.elapsed().as_secs() > timeout_secs {
                                    error_message.set(Some("Invoice expired. Please try again.".to_string()));
                                    is_polling.set(false);
                                    quote_info.set(None);
                                    break;
                                }

                                // Wait for WebSocket message with timeout
                                tokio::select! {
                                    status = rx.recv() => {
                                        match status {
                                            Some(cashu_ws::QuoteStatus::Paid) | Some(cashu_ws::QuoteStatus::Issued) => {
                                                log::info!("Payment detected via WebSocket, minting tokens...");
                                                gloo_timers::future::TimeoutFuture::new(1000).await;

                                                if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                                    break;
                                                }

                                                match cashu_wallet::mint_tokens_from_quote(
                                                    mint_url.clone(),
                                                    quote_id.clone()
                                                ).await {
                                                    Ok(amount) => {
                                                        if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                                            break;
                                                        }
                                                        success_message.set(Some(format!(
                                                            "Successfully received {} sats!", amount
                                                        )));
                                                        quote_info.set(None);
                                                        is_polling.set(false);

                                                        spawn(async move {
                                                            gloo_timers::future::TimeoutFuture::new(2000).await;
                                                            on_close.call(());
                                                        });
                                                    }
                                                    Err(e) => {
                                                        error_message.set(Some(format!("Failed to mint tokens: {}", e)));
                                                        is_polling.set(false);
                                                        quote_info.set(None);
                                                    }
                                                }
                                                break;
                                            }
                                            Some(cashu_ws::QuoteStatus::Expired) => {
                                                error_message.set(Some("Invoice expired".to_string()));
                                                is_polling.set(false);
                                                quote_info.set(None);
                                                break;
                                            }
                                            Some(cashu_ws::QuoteStatus::Pending) => {
                                                // Still waiting, continue
                                            }
                                            Some(cashu_ws::QuoteStatus::Unknown(_)) => {
                                                // Unknown status, continue waiting
                                            }
                                            None => {
                                                // Channel closed, fall back to HTTP polling
                                                log::warn!("WebSocket channel closed, falling back to HTTP polling");
                                                break;
                                            }
                                        }
                                    }
                                    _ = gloo_timers::future::TimeoutFuture::new(30000) => {
                                        // Timeout waiting for WebSocket, check via HTTP as backup
                                        match cashu_wallet::check_mint_quote_status(
                                            mint_url.clone(),
                                            quote_id.clone()
                                        ).await {
                                            Ok(cashu_wallet::QuoteStatus::Paid) => {
                                                log::info!("Payment detected via HTTP backup check, minting tokens...");
                                                gloo_timers::future::TimeoutFuture::new(1000).await;

                                                if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                                    break;
                                                }

                                                match cashu_wallet::mint_tokens_from_quote(
                                                    mint_url.clone(),
                                                    quote_id.clone()
                                                ).await {
                                                    Ok(amount) => {
                                                        if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                                            break;
                                                        }
                                                        success_message.set(Some(format!(
                                                            "Successfully received {} sats!", amount
                                                        )));
                                                        quote_info.set(None);
                                                        is_polling.set(false);

                                                        spawn(async move {
                                                            gloo_timers::future::TimeoutFuture::new(2000).await;
                                                            on_close.call(());
                                                        });
                                                    }
                                                    Err(e) => {
                                                        error_message.set(Some(format!("Failed to mint tokens: {}", e)));
                                                        is_polling.set(false);
                                                        quote_info.set(None);
                                                    }
                                                }
                                                break;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            // WebSocket failed, fall back to HTTP polling
                            log::warn!("WebSocket not available ({}), using HTTP polling", e);

                            let mut attempts = 0;
                            let max_attempts = 300; // 10 minutes at 2-second intervals

                            loop {
                                if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                    log::info!("Polling cancelled, modal was closed");
                                    break;
                                }

                                if attempts >= max_attempts {
                                    error_message.set(Some("Invoice expired. Please try again.".to_string()));
                                    is_polling.set(false);
                                    quote_info.set(None);
                                    break;
                                }

                                if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                    log::info!("Polling cancelled before network call");
                                    break;
                                }

                                match cashu_wallet::check_mint_quote_status(
                                    mint_url.clone(),
                                    quote_id.clone()
                                ).await {
                                    Ok(cashu_wallet::QuoteStatus::Paid) => {
                                        log::info!("Payment detected, waiting 2 seconds before minting...");
                                        gloo_timers::future::TimeoutFuture::new(2000).await;

                                        if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                            log::info!("Polling cancelled before minting");
                                            break;
                                        }

                                        match cashu_wallet::mint_tokens_from_quote(
                                            mint_url.clone(),
                                            quote_id.clone()
                                        ).await {
                                            Ok(amount) => {
                                                if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                                    log::info!("Polling cancelled after minting, not updating state");
                                                    break;
                                                }

                                                success_message.set(Some(format!(
                                                    "Successfully received {} sats!", amount
                                                )));
                                                quote_info.set(None);
                                                is_polling.set(false);

                                                spawn(async move {
                                                    gloo_timers::future::TimeoutFuture::new(2000).await;
                                                    on_close.call(());
                                                });
                                            }
                                            Err(e) => {
                                                error_message.set(Some(format!("Failed to mint tokens: {}", e)));
                                                is_polling.set(false);
                                                quote_info.set(None);
                                            }
                                        }
                                        break;
                                    }
                                    Ok(cashu_wallet::QuoteStatus::Expired) => {
                                        error_message.set(Some("Invoice expired".to_string()));
                                        is_polling.set(false);
                                        quote_info.set(None);
                                        break;
                                    }
                                    Ok(cashu_wallet::QuoteStatus::Failed) => {
                                        error_message.set(Some("Payment failed".to_string()));
                                        is_polling.set(false);
                                        quote_info.set(None);
                                        break;
                                    }
                                    Ok(_) => {
                                        // Still unpaid or pending, continue polling
                                    }
                                    Err(e) => {
                                        log::error!("Failed to check quote status: {}", e);
                                    }
                                }

                                attempts += 1;

                                if !*is_polling_clone.read() || quote_info_clone.read().is_none() {
                                    log::info!("Polling cancelled before sleep");
                                    break;
                                }

                                gloo_timers::future::TimeoutFuture::new(2000).await;
                            }
                        }
                    }
                });
            }
        }
    });

    let on_generate = move |_| {
        // Early guard: prevent duplicate submissions if already generating
        if *is_generating.read() {
            return;
        }

        let amount_str = amount.read().clone();
        let mint = selected_mint.read().clone();

        // Validate amount
        let amount_sats = match amount_str.parse::<u64>() {
            Ok(a) if a > 0 => a,
            _ => {
                error_message.set(Some("Please enter a valid amount".to_string()));
                return;
            }
        };

        if mint.is_empty() {
            error_message.set(Some("Please select a mint".to_string()));
            return;
        }

        is_generating.set(true);
        error_message.set(None);
        success_message.set(None);

        spawn(async move {
            match cashu_wallet::create_mint_quote(mint, amount_sats, None).await {
                Ok(quote) => {
                    quote_info.set(Some(quote));
                    is_generating.set(false);
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to create invoice: {}", e)));
                    is_generating.set(false);
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
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 {
                        class: "text-xl font-bold flex items-center gap-2",
                        span { "⚡" }
                        "Receive Lightning"
                    }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Show success message
                    if let Some(msg) = success_message.read().as_ref() {
                        div {
                            class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div { class: "text-2xl", "✅" }
                                div {
                                    p {
                                        class: "text-sm font-semibold text-green-800 dark:text-green-200",
                                        "{msg}"
                                    }
                                }
                            }
                        }
                    }

                    // Show error message
                    if let Some(msg) = error_message.read().as_ref() {
                        div {
                            class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div { class: "text-2xl", "⚠️" }
                                div {
                                    p {
                                        class: "text-sm text-red-800 dark:text-red-200",
                                        "{msg}"
                                    }
                                }
                            }
                        }
                    }

                    // Invoice display (if generated)
                    if let Some(q) = quote_info.read().as_ref() {
                        div {
                            class: "space-y-4",

                            // QR Code
                            div {
                                class: "flex justify-center p-4 bg-white dark:bg-gray-900 rounded-lg",
                                dangerous_inner_html: "{generate_qr_svg(&q.invoice)}"
                            }

                            // Invoice string
                            div {
                                label {
                                    class: "block text-xs font-semibold mb-1",
                                    "Lightning Invoice:"
                                }
                                div {
                                    class: "flex gap-2",
                                    textarea {
                                        id: "lightning-invoice",
                                        class: "flex-1 px-3 py-2 bg-background border border-border rounded font-mono text-xs min-h-[80px]",
                                        readonly: true,
                                        value: q.invoice.clone(),
                                        onclick: move |_| {
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                use wasm_bindgen::JsCast;
                                                if let Some(window) = web_sys::window() {
                                                    if let Some(document) = window.document() {
                                                        if let Some(textarea) = document.query_selector("#lightning-invoice").ok().flatten() {
                                                            if let Ok(element) = textarea.dyn_into::<web_sys::HtmlTextAreaElement>() {
                                                                element.select();
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    button {
                                        class: "px-3 py-2 bg-blue-500 hover:bg-blue-600 text-white text-xs rounded transition",
                                        onclick: move |_| {
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                if let Some(invoice_to_copy) = quote_info.read().as_ref() {
                                                    if let Some(window) = web_sys::window() {
                                                        let navigator = window.navigator();
                                                        let clipboard = navigator.clipboard();
                                                        let _ = clipboard.write_text(&invoice_to_copy.invoice);
                                                    }
                                                }
                                            }
                                        },
                                        "Copy"
                                    }
                                }
                            }

                            // Waiting message
                            if *is_polling.read() {
                                div {
                                    class: "flex items-center justify-center gap-2 text-sm text-muted-foreground",
                                    div { class: "animate-spin", "⚡" }
                                    span { "Waiting for payment..." }
                                }
                            }
                        }
                    } else {
                        // Amount input (before invoice generated)
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "Amount (sats)"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg text-lg",
                                r#type: "number",
                                placeholder: "1000",
                                min: "1",
                                value: amount.read().clone(),
                                oninput: move |evt| amount.set(evt.value())
                            }
                        }

                        // Mint selection
                        if !mints.is_empty() {
                            div {
                                label {
                                    class: "block text-sm font-semibold mb-2",
                                    "Select Mint"
                                }
                                select {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg",
                                    value: selected_mint.read().clone(),
                                    onchange: move |evt| selected_mint.set(evt.value()),
                                    for mint_url in mints.iter() {
                                        option {
                                            value: mint_url.clone(),
                                            "{shorten_url(mint_url, 35)}"
                                        }
                                    }
                                }
                            }
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
                            is_polling.set(false);
                            error_message.set(None);
                            success_message.set(None);
                            on_close.call(());
                        },
                        if quote_info.read().is_some() { "Close" } else { "Cancel" }
                    }
                    if quote_info.read().is_none() {
                        button {
                            class: if *is_generating.read() || amount.read().is_empty() {
                                "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                            } else {
                                "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                            },
                            disabled: *is_generating.read() || amount.read().is_empty(),
                            onclick: on_generate,
                            if *is_generating.read() {
                                "Generating..."
                            } else {
                                "Generate Invoice"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Generate QR code SVG
fn generate_qr_svg(data: &str) -> String {
    match QrCode::new(data) {
        Ok(code) => {
            let svg = code.render::<svg::Color>()
                .min_dimensions(200, 200)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#ffffff"))
                .build();
            svg
        }
        Err(_) => {
            "<div>Failed to generate QR code</div>".to_string()
        }
    }
}
