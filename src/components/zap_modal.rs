use dioxus::prelude::*;
use nostr_sdk::{PublicKey, EventId, RelayUrl};
use crate::services::lnurl;
use crate::stores::nostr_client::get_client;
use crate::stores::{signer, nwc_store, settings_store};
use qrcode::QrCode;
use qrcode::render::svg;
use wasm_bindgen::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "webln"], js_name = enable, catch)]
    async fn webln_enable_raw() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "webln"], js_name = sendPayment, catch)]
    async fn webln_send_payment_raw(invoice: &str) -> Result<JsValue, JsValue>;
}

// Safe wrapper for webln_enable that handles errors gracefully
async fn webln_enable() -> Result<(), String> {
    webln_enable_raw()
        .await
        .map(|_| ())
        .map_err(|e| format!("WebLN enable failed: {:?}", e))
}

// Safe wrapper for webln_send_payment that handles errors gracefully
async fn webln_send_payment(invoice: &str) -> Result<JsValue, String> {
    webln_send_payment_raw(invoice)
        .await
        .map_err(|e| {
            // Check if it's a user cancellation
            let error_msg = format!("{:?}", e);
            if error_msg.contains("Prompt was closed") || error_msg.contains("User rejected") {
                "Payment cancelled by user".to_string()
            } else {
                format!("WebLN payment failed: {}", error_msg)
            }
        })
}

fn is_webln_available() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
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
    pub on_close: EventHandler<()>,
}

#[component]
pub fn ZapModal(props: ZapModalProps) -> Element {
    let mut zap_amount = use_signal(|| 21u64);
    let mut custom_amount = use_signal(|| String::new());
    let mut zap_message = use_signal(|| String::new());
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut invoice = use_signal(|| None::<String>);
    let mut qr_code_svg = use_signal(|| None::<String>);
    let webln_available = is_webln_available();
    let toast = consume_toast();

    // Preset amounts in sats
    let preset_amounts = vec![21, 100, 500, 1000, 5000, 10000];

    let handle_zap = move |_| {
        let recipient_pubkey_str = props.recipient_pubkey.clone();
        let lud16 = props.lud16.clone();
        let lud06 = props.lud06.clone();
        let amount = *zap_amount.read();
        let message = zap_message.read().clone();
        let event_id_str = props.event_id.clone();
        let toast_api = toast.clone();

        loading.set(true);
        error_msg.set(None);
        invoice.set(None);
        qr_code_svg.set(None);

        spawn(async move {
            // Get signer
            let signer_type = match signer::get_signer() {
                Some(s) => s,
                None => {
                    error_msg.set(Some("No signer available. Please connect a signer first.".to_string()));
                    loading.set(false);
                    return;
                }
            };

            // Parse recipient pubkey
            let recipient_pubkey = match PublicKey::parse(&recipient_pubkey_str) {
                Ok(pk) => pk,
                Err(e) => {
                    error_msg.set(Some(format!("Invalid recipient pubkey: {}", e)));
                    loading.set(false);
                    return;
                }
            };

            // Parse event ID if provided
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

            // Get relays from client
            let relays = if let Some(client) = get_client() {
                client
                    .relays()
                    .await
                    .into_iter()
                    .map(|(url, _)| url)
                    .take(5)
                    .collect::<Vec<RelayUrl>>()
            } else {
                vec![]
            };

            if relays.is_empty() {
                error_msg.set(Some("No relays available".to_string()));
                loading.set(false);
                return;
            }

            // Prepare zap
            let (pay_info, amount_msats) = match lnurl::prepare_zap(
                lud16.as_deref(),
                lud06.as_deref(),
                amount,
            ).await {
                Ok(info) => info,
                Err(e) => {
                    error_msg.set(Some(format!("Failed to prepare zap: {}", e)));
                    loading.set(false);
                    return;
                }
            };

            // Create zap request builder
            let msg_opt = if message.is_empty() { None } else { Some(message) };
            let builder = lnurl::create_zap_request_unsigned(
                recipient_pubkey,
                relays,
                amount_msats,
                msg_opt,
                event_id,
                None, // No event coordinate for generic zaps
            );

            // Sign the zap request based on signer type
            let zap_request = match signer_type {
                signer::SignerType::Keys(ref keys) => {
                    match builder.sign_with_keys(keys) {
                        Ok(event) => event,
                        Err(e) => {
                            error_msg.set(Some(format!("Failed to sign zap request: {}", e)));
                            loading.set(false);
                            return;
                        }
                    }
                }
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
            };

            // Request invoice
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
            ).await {
                Ok(response) => response.pr,
                Err(e) => {
                    error_msg.set(Some(format!("Failed to get invoice: {}", e)));
                    loading.set(false);
                    return;
                }
            };

            let inv_clone = inv.clone();

            // Get payment preference
            let payment_preference = settings_store::SETTINGS.read().payment_method_preference.clone();
            let nwc_available = nwc_store::is_connected();

            // Try payment based on preference
            match payment_preference.as_str() {
                "nwc_first" if nwc_available => {
                    // Try NWC first
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
                            // Continue to WebLN fallback
                        }
                    }
                }
                "webln_first" if webln_available => {
                    // WebLN will be tried below, then NWC as fallback
                    // Skip NWC attempt here
                }
                "manual_only" => {
                    // Skip auto-payment, just show invoice and QR
                    invoice.set(Some(inv_clone.clone()));

                    // Generate QR code
                    if let Ok(code) = QrCode::new(&inv_clone) {
                        let svg_string = code.render::<svg::Color>()
                            .min_dimensions(200, 200)
                            .build();
                        qr_code_svg.set(Some(svg_string));
                    }

                    loading.set(false);
                    return;
                }
                _ => {
                    // Default or "always_ask": try NWC if available
                    if nwc_available {
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
                                // Continue to WebLN fallback
                            }
                        }
                    }
                }
            }

            // Try to pay with WebLN if available
            if webln_available {
                // Enable WebLN
                match webln_enable().await {
                    Ok(_) => {
                        // Try to send payment
                        match webln_send_payment(&inv_clone).await {
                            Ok(result) if !result.is_null() && !result.is_undefined() => {
                                // Payment successful
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
                                // Payment failed, show invoice
                            }
                            Err(e) => {
                                log::info!("WebLN payment failed: {}", e);
                                // Payment failed or cancelled, show invoice
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("WebLN enable failed: {}", e);
                        // Continue to fallback
                    }
                }

                // If WebLN failed and preference is "webln_first", try NWC as fallback
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

            // Generate QR code
            if let Ok(code) = QrCode::new(&inv_clone) {
                let svg_string = code
                    .render::<svg::Color>()
                    .min_dimensions(256, 256)
                    .build();
                qr_code_svg.set(Some(svg_string));
            }

            invoice.set(Some(inv));
            loading.set(false);
        });
    };

    let copy_invoice = move |_| {
        if let Some(_inv) = invoice.read().as_ref() {
            // Try to copy to clipboard using web_sys
            #[cfg(target_arch = "wasm32")]
            {
                use web_sys::window;

                if let Some(window) = window() {
                    let navigator = window.navigator();
                    let clipboard = navigator.clipboard();
                    let inv_clone = _inv.clone();
                    spawn(async move {
                        let promise = clipboard.write_text(&inv_clone);
                        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                    });
                }
            }
        }
    };

    let open_in_wallet = move |_| {
        if let Some(_inv) = invoice.read().as_ref() {
            // Open lightning: URI
            #[cfg(target_arch = "wasm32")]
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
                onclick: move |e: MouseEvent| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between p-4 border-b border-border",
                    h2 {
                        class: "text-xl font-bold",
                        "⚡ Zap {props.recipient_name}"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground",
                        onclick: move |_| props.on_close.call(()),
                        "✕"
                    }
                }

                // Content
                div {
                    class: "p-4 space-y-4",

                    if let Some(inv) = invoice.read().as_ref() {
                        div {
                            class: "space-y-4",

                            // QR Code
                            if let Some(qr) = qr_code_svg.read().as_ref() {
                                div {
                                    class: "flex justify-center bg-white p-4 rounded-lg",
                                    dangerous_inner_html: "{qr}"
                                }
                            }

                            // Invoice string
                            div {
                                class: "bg-accent/20 p-4 rounded-lg",
                                p {
                                    class: "text-sm text-muted-foreground mb-2",
                                    "Lightning Invoice"
                                }
                                p {
                                    class: "font-mono text-xs break-all",
                                    "{inv}"
                                }
                            }

                            // Action buttons
                            div {
                                class: "flex gap-2",
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

                            // WebLN availability hint
                            if !webln_available {
                                p {
                                    class: "text-xs text-muted-foreground text-center",
                                    "💡 Install a WebLN wallet extension (like Alby) for one-click zaps"
                                }
                            }
                        }
                    } else {
                        // Amount selection
                        div {
                            class: "space-y-2",
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Select Amount (sats)"
                            }
                            div {
                                class: "grid grid-cols-3 gap-2",
                                for amount in preset_amounts {
                                    button {
                                        class: if *zap_amount.read() == amount {
                                            "px-4 py-2 rounded bg-primary text-primary-foreground font-medium"
                                        } else {
                                            "px-4 py-2 rounded bg-secondary text-secondary-foreground hover:bg-secondary/80"
                                        },
                                        onclick: move |_| zap_amount.set(amount),
                                        "{amount}"
                                    }
                                }
                            }

                            div {
                                class: "flex items-center gap-2 mt-2",
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
                                    }
                                }
                                span {
                                    class: "text-sm text-muted-foreground",
                                    "sats"
                                }
                            }
                        }

                        // Message
                        div {
                            class: "space-y-2",
                            label {
                                class: "block text-sm font-medium",
                                "Message (optional)"
                            }
                            textarea {
                                class: "w-full px-3 py-2 bg-background border border-border rounded resize-none",
                                rows: 3,
                                placeholder: "Add a message with your zap...",
                                value: "{zap_message}",
                                oninput: move |e| zap_message.set(e.value())
                            }
                        }

                        // Error message
                        if let Some(err) = error_msg.read().as_ref() {
                            div {
                                class: "bg-red-500/10 border border-red-500/20 text-red-500 p-3 rounded",
                                "{err}"
                            }
                        }

                        // Action buttons
                        div {
                            class: "flex gap-2 pt-2",
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
