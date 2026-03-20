use crate::services::lnurl;
use crate::stores::nostr_client::get_client;
use crate::stores::{cashu, nwc_store, settings_store, signer};
use dioxus::hooks::use_reactive;
use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::{EventId, PublicKey, RelayUrl};
use qrcode::render::svg;
use qrcode::QrCode;
use std::time::Duration;
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "web")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "webln"], js_name = enable, catch)]
    async fn webln_enable_raw() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_namespace = ["window", "webln"], js_name = sendPayment, catch)]
    async fn webln_send_payment_raw(invoice: &str) -> Result<JsValue, JsValue>;
}
#[cfg(feature = "web")]
async fn webln_enable() -> Result<(), String> {
    webln_enable_raw()
        .await
        .map(|_| ())
        .map_err(|e| format!("WebLN enable failed: {:?}", e))
}
#[cfg(feature = "web")]
async fn webln_send_payment(invoice: &str) -> Result<JsValue, String> {
    webln_send_payment_raw(invoice).await.map_err(|e| {
        let error_msg = format!("{:?}", e);
        if error_msg.contains("Prompt was closed") || error_msg.contains("User rejected") {
            "Payment cancelled by user".to_string()
        } else {
            format!("WebLN payment failed: {}", error_msg)
        }
    })
}
fn is_webln_available() -> bool {
    #[cfg(feature = "web")]
    {
        use wasm_bindgen::prelude::*;
        use web_sys::window;
        if let Some(window) = window() {
            return js_sys::Reflect::has(&window, &JsValue::from_str("webln")).unwrap_or(false);
        }
    }
    false
}
#[derive(Props, Clone, PartialEq)]
pub struct ZapModalProps {
    pub recipient_pubkey: String,
    pub recipient_name: String,
    pub lud16: Option<String>,
    pub lud06: Option<String>,
    pub event_id: Option<String>,
    #[props(default)]
    pub initial_amount: Option<u64>,
    #[props(default)]
    pub relay_hints: Option<Vec<String>>,
    pub on_close: EventHandler<()>,
}
#[component]
pub fn ZapModal(props: ZapModalProps) -> Element {
    let preset_amounts = vec![21, 100, 500, 1000, 5000, 10000];
    let initial_amount = props.initial_amount.unwrap_or(21);
    let initial_custom_amount = props
        .initial_amount
        .filter(|amount| !preset_amounts.contains(amount))
        .map(|amount| amount.to_string())
        .unwrap_or_default();
    let mut zap_amount = use_signal(|| initial_amount);
    let mut custom_amount = use_signal(|| initial_custom_amount.clone());
    let mut zap_message = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut invoice = use_signal(|| None::<String>);
    let mut qr_code_svg = use_signal(|| None::<String>);
    let webln_available = is_webln_available();
    let toast = consume_toast();
    let mut nutzap_mint = use_signal(|| None::<cashu::NutzapMint>);
    let mut checking_nutzap = use_signal(|| false);
    let mut nutzap_request_version = use_signal(|| 0u32);
    {
        let recipient_pubkey = props.recipient_pubkey.clone();
        use_effect(use_reactive!(|recipient_pubkey| {
            let current_version = nutzap_request_version.peek().saturating_add(1);
            nutzap_request_version.set(current_version);
            let pubkey_snapshot = recipient_pubkey.clone();
            checking_nutzap.set(true);
            nutzap_mint.set(None);
            spawn(async move {
                let result = cashu::validate_nutzap_recipient(&pubkey_snapshot).await;
                if *nutzap_request_version.peek() != current_version {
                    log::debug!("Discarding stale nutzap eligibility result");
                    return;
                }
                match result {
                    Ok(mint) => nutzap_mint.set(Some(mint)),
                    Err(_) => nutzap_mint.set(None),
                }
                checking_nutzap.set(false);
            });
        }));
    }
    let handle_zap = move |_| {
        let recipient_pubkey_str = props.recipient_pubkey.clone();
        let lud16 = props.lud16.clone();
        let lud06 = props.lud06.clone();
        let amount = *zap_amount.read();
        let message = zap_message.read().clone();
        let event_id_str = props.event_id.clone();
        let relay_hints = props.relay_hints.clone();
        let toast_api = toast;
        loading.set(true);
        error_msg.set(None);
        invoice.set(None);
        qr_code_svg.set(None);
        spawn(async move {
            let signer_type = match signer::get_signer() {
                Some(s) => s,
                None => {
                    error_msg.set(Some(
                        "No signer available. Please connect a signer first.".to_string(),
                    ));
                    loading.set(false);
                    return;
                }
            };
            let recipient_pubkey = match PublicKey::parse(&recipient_pubkey_str) {
                Ok(pk) => pk,
                Err(e) => {
                    error_msg.set(Some(format!("Invalid recipient pubkey: {}", e)));
                    loading.set(false);
                    return;
                }
            };
            let event_id = if let Some(eid_str) = event_id_str {
                match EventId::parse(&eid_str) {
                    Ok(eid) => Some(eid),
                    Err(e) => {
                        error_msg.set(Some(format!("Invalid event ID: {}", e)));
                        loading.set(false);
                        return;
                    }
                }
            } else {
                None
            };
            let relays = {
                let hinted_relays = relay_hints
                    .as_ref()
                    .map(|relay_hints| {
                        relay_hints
                            .iter()
                            .filter_map(|relay| RelayUrl::parse(relay).ok())
                            .take(5)
                            .collect::<Vec<RelayUrl>>()
                    })
                    .filter(|relays| !relays.is_empty());

                if let Some(relays) = hinted_relays {
                    relays
                } else if let Some(client) = get_client() {
                    client
                        .relays()
                        .await
                        .into_keys()
                        .take(5)
                        .collect::<Vec<RelayUrl>>()
                } else {
                    vec![]
                }
            };
            if relays.is_empty() {
                error_msg.set(Some("No relays available".to_string()));
                loading.set(false);
                return;
            }
            let (pay_info, amount_msats) =
                match lnurl::prepare_zap(lud16.as_deref(), lud06.as_deref(), amount).await {
                    Ok(info) => info,
                    Err(e) => {
                        error_msg.set(Some(format!("Failed to prepare zap: {}", e)));
                        loading.set(false);
                        return;
                    }
                };
            let msg_opt = if message.is_empty() {
                None
            } else {
                Some(message.clone())
            };
            let builder = lnurl::create_zap_request_unsigned(
                recipient_pubkey,
                relays,
                amount_msats,
                msg_opt,
                event_id,
                None,
            );
            let zap_request = match signer_type {
                signer::SignerType::Keys(ref keys) => match builder.sign_with_keys(keys) {
                    Ok(event) => event,
                    Err(e) => {
                        error_msg.set(Some(format!("Failed to sign zap request: {}", e)));
                        loading.set(false);
                        return;
                    }
                },
                #[cfg(target_family = "wasm")]
                signer::SignerType::BrowserExtension(ref signer) => {
                    #[allow(unused_imports)]
                    use nostr::signer::NostrSigner;
                    match builder.sign(signer.as_ref()).await {
                        Ok(event) => event,
                        Err(e) => {
                            error_msg.set(Some(format!("Failed to sign zap request: {}", e)));
                            loading.set(false);
                            return;
                        }
                    }
                }
                signer::SignerType::NostrConnect(ref nostr_connect) => {
                    #[allow(unused_imports)]
                    use nostr::signer::NostrSigner;
                    match builder.sign(nostr_connect.as_ref()).await {
                        Ok(event) => event,
                        Err(e) => {
                            error_msg.set(Some(format!("Failed to sign zap request: {}", e)));
                            loading.set(false);
                            return;
                        }
                    }
                }
                #[cfg(feature = "mobile")]
                signer::SignerType::AndroidSigner(ref android_signer) => {
                    #[allow(unused_imports)]
                    use nostr::signer::NostrSigner;
                    match builder.sign(android_signer.as_ref()).await {
                        Ok(event) => event,
                        Err(e) => {
                            error_msg.set(Some(format!("Failed to sign zap request: {}", e)));
                            loading.set(false);
                            return;
                        }
                    }
                }
            };
            let lnurl_param = if lud16.is_some() {
                None
            } else {
                lud06.as_deref()
            };
            let inv = match lnurl::request_zap_invoice(
                &pay_info.callback,
                amount_msats,
                &zap_request,
                lnurl_param,
            )
            .await
            {
                Ok(response) => response.pr,
                Err(e) => {
                    error_msg.set(Some(format!("Failed to get invoice: {}", e)));
                    loading.set(false);
                    return;
                }
            };
            let inv_clone = inv.clone();
            let payment_preference = settings_store::SETTINGS
                .read()
                .payment_method_preference
                .clone();
            let nwc_available = nwc_store::is_connected();
            match payment_preference.as_str() {
                "cashu_first" => {
                    use futures::future::{select, Either};
                    let timeout = async {
                        crate::platform::timer::sleep_ms(5000).await;
                    };
                    let check_done = async {
                        while *checking_nutzap.peek() {
                            crate::platform::timer::sleep_ms(100).await;
                        }
                    };
                    match select(Box::pin(timeout), Box::pin(check_done)).await {
                        Either::Left(_) => {
                            log::warn!(
                                "Nutzap eligibility check timed out, proceeding with Lightning"
                            );
                        }
                        Either::Right(_) => {}
                    }
                    if let Some(mint) = nutzap_mint.read().as_ref() {
                        if mint.unit != "sat" {
                            log::info!(
                                "Mint {} uses unit '{}', not sats - skipping nutzap",
                                mint.url,
                                mint.unit
                            );
                        } else {
                            let normalized_mint_url = cashu::normalize_mint_url(&mint.url);
                            let balance = cashu::get_mint_unit_spendable_balance(
                                &normalized_mint_url,
                                &mint.unit,
                            );
                            if balance >= amount {
                                log::info!("Attempting payment with Cashu nutzap via {}", mint.url);
                                let nutzap_event_id = event_id.as_ref().map(|e| e.to_hex());
                                let nutzap_comment_opt = if message.is_empty() {
                                    None
                                } else {
                                    Some(message.clone())
                                };
                                match cashu::send_nutzap(
                                    &recipient_pubkey_str,
                                    amount,
                                    nutzap_event_id.as_deref(),
                                    None,
                                    nutzap_comment_opt.as_deref(),
                                )
                                .await
                                {
                                    Ok(result) => {
                                        log::info!(
                                            "Nutzap successful: {} sats (fee: {} sats)",
                                            result.amount,
                                            result.fee
                                        );
                                        loading.set(false);
                                        toast_api.success(
                                            "Nutzap sent!".to_string(),
                                            ToastOptions::new()
                                                .description(format!(
                                                    "Sent {} sats via ecash (fee: {} sats)",
                                                    result.amount, result.fee,
                                                ))
                                                .duration(Duration::from_secs(3))
                                                .permanent(false),
                                        );
                                        props.on_close.call(());
                                        return;
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "Nutzap failed, falling back to Lightning: {}",
                                            e
                                        );
                                    }
                                }
                            } else {
                                log::info!(
                                    "Insufficient Cashu balance ({} < {}), using Lightning",
                                    balance,
                                    amount
                                );
                            }
                        }
                    } else {
                        log::info!("Recipient doesn't support nutzaps, using Lightning");
                    }
                    if nwc_available {
                        match nwc_store::pay_invoice(inv_clone.clone()).await {
                            Ok(_) => {
                                loading.set(false);
                                toast_api.success(
                                    "Zap sent!".to_string(),
                                    ToastOptions::new()
                                        .description(
                                            "Zap successfully sent via Nostr Wallet Connect",
                                        )
                                        .duration(Duration::from_secs(2))
                                        .permanent(false),
                                );
                                props.on_close.call(());
                                return;
                            }
                            Err(e) => {
                                log::warn!("NWC payment failed: {}", e);
                            }
                        }
                    }
                }
                "nwc_first" if nwc_available => {
                    log::info!("Attempting payment with NWC");
                    match nwc_store::pay_invoice(inv_clone.clone()).await {
                        Ok(_) => {
                            log::info!("NWC payment successful");
                            loading.set(false);
                            toast_api.success(
                                "Zap sent!".to_string(),
                                ToastOptions::new()
                                    .description("Zap successfully sent via Nostr Wallet Connect")
                                    .duration(Duration::from_secs(2))
                                    .permanent(false),
                            );
                            props.on_close.call(());
                            return;
                        }
                        Err(e) => {
                            log::warn!("NWC payment failed, falling back to WebLN: {}", e);
                        }
                    }
                }
                "webln_first" if webln_available => {}
                "manual_only" => {
                    invoice.set(Some(inv_clone.clone()));
                    if let Ok(code) = QrCode::new(&inv_clone) {
                        let svg_string =
                            code.render::<svg::Color>().min_dimensions(200, 200).build();
                        qr_code_svg.set(Some(svg_string));
                    }
                    loading.set(false);
                    return;
                }
                _ => {
                    if nwc_available {
                        log::info!("Attempting payment with NWC");
                        match nwc_store::pay_invoice(inv_clone.clone()).await {
                            Ok(_) => {
                                log::info!("NWC payment successful");
                                loading.set(false);
                                toast_api.success(
                                    "Zap sent!".to_string(),
                                    ToastOptions::new()
                                        .description(
                                            "Zap successfully sent via Nostr Wallet Connect",
                                        )
                                        .duration(Duration::from_secs(2))
                                        .permanent(false),
                                );
                                props.on_close.call(());
                                return;
                            }
                            Err(e) => {
                                log::warn!("NWC payment failed, falling back to WebLN: {}", e);
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "web")]
            if webln_available {
                match webln_enable().await {
                    Ok(_) => match webln_send_payment(&inv_clone).await {
                        Ok(result) if !result.is_null() && !result.is_undefined() => {
                            loading.set(false);
                            toast_api.success(
                                "Zap sent!".to_string(),
                                ToastOptions::new()
                                    .description("Zap successfully sent via WebLN")
                                    .duration(Duration::from_secs(2))
                                    .permanent(false),
                            );
                            props.on_close.call(());
                            return;
                        }
                        Ok(_) => {
                            log::info!("WebLN payment returned null/undefined");
                        }
                        Err(e) => {
                            log::info!("WebLN payment failed: {}", e);
                        }
                    },
                    Err(e) => {
                        log::warn!("WebLN enable failed: {}", e);
                    }
                }
                if payment_preference == "webln_first" && nwc_available {
                    log::info!("WebLN failed, trying NWC as fallback");
                    match nwc_store::pay_invoice(inv_clone.clone()).await {
                        Ok(_) => {
                            log::info!("NWC fallback payment successful");
                            loading.set(false);
                            toast_api.success(
                                "Zap sent!".to_string(),
                                ToastOptions::new()
                                    .description("Zap successfully sent via Nostr Wallet Connect")
                                    .duration(Duration::from_secs(2))
                                    .permanent(false),
                            );
                            props.on_close.call(());
                            return;
                        }
                        Err(e) => {
                            log::warn!("NWC fallback also failed: {}", e);
                        }
                    }
                }
            }
            if let Ok(code) = QrCode::new(&inv_clone) {
                let svg_string = code.render::<svg::Color>().min_dimensions(256, 256).build();
                qr_code_svg.set(Some(svg_string));
            }
            invoice.set(Some(inv));
            loading.set(false);
        });
    };
    let copy_invoice = move |_| {
        if let Some(inv) = invoice.read().as_ref() {
            let inv_clone = inv.clone();
            let toast_api = toast;
            spawn(async move {
                match crate::platform::clipboard::copy_to_clipboard(&inv_clone).await {
                    Ok(()) => {
                        toast_api.success(
                            "Invoice copied".to_string(),
                            ToastOptions::new().duration(Duration::from_secs(2)),
                        );
                    }
                    Err(error) => {
                        toast_api.error(
                            "Copy failed".to_string(),
                            ToastOptions::new()
                                .description(error)
                                .duration(Duration::from_secs(3)),
                        );
                    }
                }
            });
        }
    };
    let open_in_wallet = move |_| {
        if let Some(_inv) = invoice.read().as_ref() {
            #[cfg(feature = "web")]
            {
                use web_sys::window;
                if let Some(window) = window() {
                    let uri = format!("lightning:{}", _inv);
                    let _ = window.open_with_url(&uri);
                }
            }
        }
    };
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "bg-background border border-border rounded-lg shadow-lg max-w-md w-full mx-4 max-h-[90vh] overflow-y-auto",
                tabindex: "-1",
                onmounted: move |_evt| {
                    #[cfg(feature = "web")]
                    {
                        if let Some(html_element) = _evt.data().downcast::<web_sys::HtmlElement>() {
                            let _ = html_element.focus();
                        }
                    }
                },
                onkeydown: move |evt: KeyboardEvent| {
                    if evt.key() == Key::Escape {
                        evt.stop_propagation();
                        props.on_close.call(());
                    }
                },
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div { class: "flex items-center justify-between p-4 border-b border-border",
                    h2 { class: "text-xl font-bold", "⚡ Zap {props.recipient_name}" }
                    button {
                        class: "text-muted-foreground hover:text-foreground",
                        onclick: move |_| props.on_close.call(()),
                        "✕"
                    }
                }
                div { class: "p-4 space-y-4",
                    if let Some(inv) = invoice.read().as_ref() {
                        div { class: "space-y-4",
                            if let Some(qr) = qr_code_svg.read().as_ref() {
                                div {
                                    class: "flex justify-center bg-white p-4 rounded-lg",
                                    dangerous_inner_html: "{qr}",
                                }
                            }
                            div { class: "bg-accent/20 p-4 rounded-lg",
                                p { class: "text-sm text-muted-foreground mb-2", "Lightning Invoice" }
                                p { class: "font-mono text-xs break-all", "{inv}" }
                            }
                            div { class: "flex gap-2",
                                button {
                                    class: "flex-1 bg-primary text-primary-foreground px-4 py-2 rounded hover:bg-primary/90 transition",
                                    onclick: open_in_wallet,
                                    "Open in Wallet"
                                }
                                button {
                                    class: "flex-1 bg-secondary text-secondary-foreground px-4 py-2 rounded hover:bg-secondary/90 transition",
                                    onclick: copy_invoice,
                                    "Copy Invoice"
                                }
                            }
                            if !webln_available {
                                p { class: "text-xs text-muted-foreground text-center",
                                    "💡 Install a WebLN wallet extension (like Alby) for one-click zaps"
                                }
                            }
                        }
                    } else {
                        if settings_store::SETTINGS.read().payment_method_preference == "cashu_first" {
                            if *checking_nutzap.read() {
                                div { class: "bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-3 mb-4",
                                    p { class: "text-sm text-blue-700 dark:text-blue-300",
                                        "Checking nutzap availability..."
                                    }
                                }
                            } else if let Some(mint) = nutzap_mint.read().as_ref() {
                                {
                                    let normalized_url = cashu::normalize_mint_url(&mint.url);
                                    if mint.unit != "sat" {
                                        rsx! {
                                            div { class: "bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-lg p-3 mb-4",
                                                p { class: "text-sm text-amber-700 dark:text-amber-300",
                                                    "Mint uses '{mint.unit}' unit, not sats. Will use Lightning."
                                                }
                                            }
                                        }
                                    } else {
                                        let balance = cashu::get_mint_unit_spendable_balance(
                                            &normalized_url,
                                            &mint.unit,
                                        );
                                        rsx! {
                                            div { class: "bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg p-3 mb-4",
                                                p { class: "text-sm text-green-700 dark:text-green-300",
                                                    "✓ Nutzap available via {normalized_url} ({balance} sats at mint)"
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-lg p-3 mb-4",
                                    p { class: "text-sm text-amber-700 dark:text-amber-300",
                                        "Recipient doesn't support nutzaps. Will use Lightning."
                                    }
                                }
                            }
                        }
                        div { class: "space-y-2",
                            label { class: "block text-sm font-medium mb-2", "Select Amount (sats)" }
                            div { class: "grid grid-cols-3 gap-2",
                                for amount in preset_amounts {
                                    button {
                                        class: if *zap_amount.read() == amount { "px-4 py-2 rounded bg-primary text-primary-foreground font-medium" } else { "px-4 py-2 rounded bg-secondary text-secondary-foreground hover:bg-secondary/80" },
                                        onclick: move |_| {
                                            custom_amount.set(String::new());
                                            zap_amount.set(amount);
                                        },
                                        "{amount}"
                                    }
                                }
                            }
                            div { class: "flex items-center gap-2 mt-2",
                                input {
                                    class: "flex-1 px-3 py-2 bg-background border border-border rounded",
                                    r#type: "number",
                                    placeholder: "Custom amount",
                                    value: "{custom_amount}",
                                    oninput: move |e| {
                                        custom_amount.set(e.value());
                                        if let Ok(amt) = e.value().parse::<u64>() {
                                            zap_amount.set(amt);
                                        }
                                    },
                                }
                                span { class: "text-sm text-muted-foreground", "sats" }
                            }
                        }
                        div { class: "space-y-2",
                            label { class: "block text-sm font-medium", "Message (optional)" }
                            textarea {
                                class: "w-full px-3 py-2 bg-background border border-border rounded resize-none",
                                rows: 3,
                                placeholder: "Add a message with your zap...",
                                value: "{zap_message}",
                                oninput: move |e| zap_message.set(e.value()),
                            }
                        }
                        if let Some(err) = error_msg.read().as_ref() {
                            div { class: "bg-red-500/10 border border-red-500/20 text-red-500 p-3 rounded",
                                "{err}"
                            }
                        }
                        div { class: "flex gap-2 pt-2",
                            button {
                                class: "flex-1 bg-secondary text-secondary-foreground px-4 py-2 rounded hover:bg-secondary/90 transition",
                                onclick: move |_| props.on_close.call(()),
                                "Cancel"
                            }
                            button {
                                class: "flex-1 bg-yellow-500 text-white px-4 py-2 rounded hover:bg-yellow-600 transition font-medium",
                                disabled: *loading.read(),
                                onclick: handle_zap,
                                if *loading.read() {
                                    "⚡ Creating invoice..."
                                } else {
                                    "⚡ Zap {zap_amount} sats"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
