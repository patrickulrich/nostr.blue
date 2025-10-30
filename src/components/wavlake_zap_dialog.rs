use dioxus::prelude::*;
use crate::stores::music_player::{self, MUSIC_PLAYER};
use crate::services::wavlake::WavlakeAPI;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LnurlPayParams {
    callback: String,
    #[serde(rename = "minSendable")]
    min_sendable: u64,
    #[serde(rename = "maxSendable")]
    max_sendable: u64,
    metadata: String,
    #[serde(rename = "commentAllowed")]
    comment_allowed: Option<u32>,
    tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InvoiceResponse {
    #[serde(default)]
    pr: Option<String>,
    #[serde(rename = "successAction", default)]
    success_action: Option<serde_json::Value>,
    #[serde(default)]
    routes: Option<Vec<serde_json::Value>>,
    // Error response fields (LNURL standard)
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    // Error response field (Wavlake format)
    #[serde(default)]
    error: Option<String>,
}

#[component]
pub fn WavlakeZapDialog() -> Element {
    let state = MUSIC_PLAYER.read();
    let show_dialog = state.show_zap_dialog;
    let track = state.zap_track.clone();

    // Early return if dialog shouldn't be shown or no track
    if !show_dialog {
        return rsx! {};
    }

    if track.is_none() {
        return rsx! {};
    }

    let track = track.unwrap();

    let mut amount = use_signal(|| 100u64);
    let mut comment = use_signal(|| String::new());
    let mut invoice = use_signal(|| None::<String>);
    let mut is_generating = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut qr_code_url = use_signal(|| None::<String>);

    let preset_amounts = vec![21, 100, 500, 1000, 2100];

    // Generate invoice
    let track_id = track.id.clone();
    let generate_invoice = move |e: Event<MouseData>| {
        e.stop_propagation();
        let track_id = track_id.clone();
        let amount_value = *amount.read();
        let comment_value = comment.read().clone();

        is_generating.set(true);
        error_msg.set(None);

        spawn(async move {
            match generate_invoice_flow(&track_id, amount_value, &comment_value).await {
                Ok((inv, qr)) => {
                    log::info!("Invoice generated successfully");
                    invoice.set(Some(inv));
                    qr_code_url.set(Some(qr));
                    is_generating.set(false);
                }
                Err(e) => {
                    log::error!("Invoice generation failed: {}", e);
                    error_msg.set(Some(e));
                    is_generating.set(false);
                }
            }
        });
    };

    // Pay with WebLN
    let pay_with_webln = move |e: Event<MouseData>| {
        e.stop_propagation();
        if let Some(inv) = invoice.read().clone() {
            spawn(async move {
                let script = format!(
                    r#"
                    (async function() {{
                        if (typeof window.webln !== 'undefined') {{
                            try {{
                                await window.webln.enable();
                                const result = await window.webln.sendPayment('{}');
                                return {{ success: true, preimage: result.preimage }};
                            }} catch (e) {{
                                return {{ success: false, error: e.message }};
                            }}
                        }} else {{
                            return {{ success: false, error: 'WebLN not available' }};
                        }}
                    }})()
                    "#,
                    inv
                );

                match js_sys::eval(&script) {
                    Ok(_) => log::info!("WebLN payment initiated"),
                    Err(e) => log::error!("WebLN payment failed: {:?}", e),
                }
            });
        }
    };

    rsx! {
        // Dialog overlay - rendered at Layout level with high z-index
        if show_dialog {
            div {
                id: "wavlake-zap-dialog",
                style: "position: fixed; top: 0; left: 0; right: 0; bottom: 0; z-index: 99999; display: flex; align-items: center; justify-content: center;",
                class: "bg-background/80 backdrop-blur-sm overflow-y-auto p-4",
                onclick: move |_: Event<MouseData>| {
                    music_player::hide_zap_dialog();
                    invoice.set(None);
                    qr_code_url.set(None);
                    error_msg.set(None);
                },

                // Dialog content wrapper for centering
                div {
                    class: "w-full max-w-md",

                    // Dialog content
                    div {
                        class: "bg-card border border-border rounded-lg shadow-lg p-6",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),

                    // Header
                    div {
                        class: "flex items-center justify-between mb-4",
                        div {
                            h3 {
                                class: "text-lg font-semibold",
                                "Zap Artist"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "{track.title} by {track.artist}"
                            }
                        }
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent transition-colors",
                            onclick: move |e: Event<MouseData>| {
                                e.stop_propagation();
                                music_player::hide_zap_dialog();
                            },
                            "✕"
                        }
                    }

                    // Error message
                    if let Some(error) = error_msg.read().as_ref() {
                        div {
                            class: "mb-4 p-3 bg-destructive/10 text-destructive rounded-md text-sm",
                            "{error}"
                        }
                    }

                    // Amount selection or invoice display
                    if invoice.read().is_none() {
                        // Amount selection view
                        div {
                            class: "space-y-4",

                            // Preset amounts
                            div {
                                p {
                                    class: "text-sm font-medium mb-2",
                                    "Select Amount (sats)"
                                }
                                div {
                                    class: "flex flex-wrap gap-2",
                                    for preset in preset_amounts.iter() {
                                        {
                                            let is_selected = *amount.read() == *preset;
                                            let preset_val = *preset;
                                            rsx! {
                                                button {
                                                    key: "{preset}",
                                                    class: if is_selected {
                                                        "px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                                                    } else {
                                                        "px-4 py-2 rounded-md bg-muted hover:bg-muted/80 transition-colors"
                                                    },
                                                    onclick: move |e: Event<MouseData>| {
                                                        e.stop_propagation();
                                                        amount.set(preset_val);
                                                    },
                                                    "{preset}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Custom amount
                            div {
                                label {
                                    class: "text-sm font-medium",
                                    "Custom Amount"
                                }
                                input {
                                    r#type: "number",
                                    value: "{amount}",
                                    class: "w-full mt-1 px-3 py-2 border border-border rounded-md bg-background",
                                    oninput: move |evt| {
                                        if let Ok(val) = evt.value().parse::<u64>() {
                                            amount.set(val);
                                        }
                                    }
                                }
                            }

                            // Comment
                            div {
                                label {
                                    class: "text-sm font-medium",
                                    "Comment (optional)"
                                }
                                textarea {
                                    value: "{comment}",
                                    placeholder: "Great track!",
                                    class: "w-full mt-1 px-3 py-2 border border-border rounded-md bg-background",
                                    rows: "3",
                                    oninput: move |evt| comment.set(evt.value())
                                }
                            }

                            // Generate button
                            button {
                                class: "w-full py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors disabled:opacity-50",
                                disabled: *is_generating.read(),
                                onclick: generate_invoice,
                                if *is_generating.read() {
                                    "Generating Invoice..."
                                } else {
                                    "Generate Invoice for {amount} sats"
                                }
                            }
                        }
                    } else {
                        // Invoice display view
                        div {
                            class: "space-y-4",

                            // QR Code
                            if let Some(qr_url) = qr_code_url.read().as_ref() {
                                div {
                                    class: "flex justify-center",
                                    img {
                                        src: "{qr_url}",
                                        alt: "Invoice QR Code",
                                        class: "w-64 h-64"
                                    }
                                }
                            }

                            // Invoice string
                            div {
                                label {
                                    class: "text-sm font-medium",
                                    "Lightning Invoice"
                                }
                                div {
                                    class: "mt-1 p-3 bg-muted rounded-md break-all text-xs font-mono",
                                    "{invoice.read().as_ref().unwrap()}"
                                }
                            }

                            // Action buttons
                            div {
                                class: "space-y-2",

                                // WebLN button
                                button {
                                    class: "w-full py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors",
                                    onclick: pay_with_webln,
                                    "⚡ Pay with WebLN"
                                }

                                // Open wallet button
                                button {
                                    class: "w-full py-2 bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80 transition-colors",
                                    onclick: {
                                        let inv = invoice.read().clone();
                                        move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                            if let Some(invoice_str) = inv.as_ref() {
                                                let url = format!("lightning:{}", invoice_str);
                                                let _ = web_sys::window()
                                                    .and_then(|w| w.open_with_url_and_target(&url, "_blank").ok());
                                            }
                                        }
                                    },
                                    "Open in Wallet"
                                }

                                // Back button
                                button {
                                    class: "w-full py-2 bg-muted text-foreground rounded-md hover:bg-muted/80 transition-colors",
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                        invoice.set(None);
                                        qr_code_url.set(None);
                                    },
                                    "← Back"
                                }
                            }
                        }
                    }
                    }  // Close dialog content div
                }  // Close wrapper div
            }  // Close overlay div
        }
    }
}

/// 5-step LNURL-pay flow
async fn generate_invoice_flow(track_id: &str, amount_sats: u64, comment: &str) -> Result<(String, String), String> {
    let track_id = track_id.trim();
    log::info!("Starting invoice flow for track: {}, amount: {} sats", track_id, amount_sats);

    // Step 1: Get LNURL from Wavlake
    let api = WavlakeAPI::new();
    // Wavlake requires appId - using "nostrmusic" as the app identifier
    let lnurl_response = api.get_lnurl(track_id, Some("nostrmusic")).await
        .map_err(|e| format!("Failed to get LNURL: {}", e))?;

    log::info!("Received LNURL: {}", lnurl_response.lnurl);

    // Step 2: Decode bech32 LNURL
    let lnurl_pay_url = decode_lnurl(&lnurl_response.lnurl)
        .map_err(|e| format!("Failed to decode LNURL: {}", e))?;

    log::info!("Decoded LNURL to: {}", lnurl_pay_url);

    // Step 3: Fetch LNURL-pay parameters
    log::info!("Fetching LNURL-pay parameters from: {}", lnurl_pay_url);

    let params: LnurlPayParams = Request::get(&lnurl_pay_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch LNURL-pay params: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse LNURL-pay params: {}", e))?;

    log::info!("LNURL-pay params received. Callback: {}, min: {}, max: {}",
        params.callback, params.min_sendable, params.max_sendable);

    // Validate amount
    let amount_millisats = amount_sats * 1000;
    if amount_millisats < params.min_sendable || amount_millisats > params.max_sendable {
        return Err(format!(
            "Amount must be between {} and {} sats",
            params.min_sendable / 1000,
            params.max_sendable / 1000
        ));
    }

    // Step 4: Request invoice
    let mut callback_url = params.callback.clone();

    // Check if callback URL already has query parameters
    let separator = if callback_url.contains('?') { "&" } else { "?" };
    callback_url.push_str(&format!("{}amount={}", separator, amount_millisats));

    if !comment.is_empty() {
        if let Some(max_comment) = params.comment_allowed {
            if comment.len() <= max_comment as usize {
                callback_url.push_str(&format!("&comment={}", urlencoding::encode(comment)));
            }
        }
    }

    log::info!("Requesting invoice from callback: {}", callback_url);

    let response = Request::get(&callback_url)
        .send()
        .await
        .map_err(|e| format!("Failed to request invoice: {}", e))?;

    // Get response as text first for better error messages
    let response_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    log::info!("Invoice callback response: {}", response_text);

    let invoice_response: InvoiceResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse invoice response: {}. Response body: {}", e, response_text))?;

    // Check for error responses
    // Wavlake format: {"error": "..."}
    if let Some(error) = &invoice_response.error {
        return Err(format!("Invoice generation failed: {}", error));
    }

    // LNURL standard format: {"status": "ERROR", "reason": "..."}
    if let Some(status) = &invoice_response.status {
        if status.to_uppercase() == "ERROR" {
            let reason = invoice_response.reason.as_deref().unwrap_or("Unknown error");
            return Err(format!("Invoice generation failed: {}", reason));
        }
    }

    // Extract the payment request
    let pr = invoice_response.pr
        .ok_or_else(|| format!("No invoice in response. Response body: {}", response_text))?;

    // Step 5: Generate QR code
    let qr_code_url = generate_qr_code(&pr)
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;

    Ok((pr, qr_code_url))
}

/// Decode bech32 LNURL to HTTPS URL
fn decode_lnurl(lnurl: &str) -> Result<String, String> {
    // In bech32 0.11, decode() returns (Hrp, Vec<u8>) - already decoded bytes
    let (_, data) = bech32::decode(lnurl)
        .map_err(|e| format!("Bech32 decode error: {}", e))?;

    String::from_utf8(data)
        .map_err(|e| format!("UTF-8 conversion error: {}", e))
}

/// Generate QR code as data URL
fn generate_qr_code(invoice: &str) -> Result<String, String> {
    use qrcode::QrCode;
    use qrcode::render::svg;
    use base64::Engine;

    let code = QrCode::new(invoice.to_uppercase())
        .map_err(|e| format!("QR code generation error: {}", e))?;

    let svg_string = code.render::<svg::Color>()
        .min_dimensions(256, 256)
        .build();

    // Convert SVG to data URL
    let encoded = base64::engine::general_purpose::STANDARD.encode(svg_string.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{}", encoded))
}
