use crate::services::lnurl;
use crate::services::wavlake::WavlakeAPI;
use crate::stores::music_player::{self, MUSIC_PLAYER};
use crate::stores::nostr_client;
use crate::stores::nostr_music::TrackSource;
use crate::stores::profiles;
use crate::stores::relay::DEFAULT_RELAYS;
use crate::utils::podcast::ValueBlock;
use dioxus::prelude::*;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::{PublicKey, RelayUrl};
use serde::{Deserialize, Serialize};
/// Convert DEFAULT_RELAYS to parsed RelayUrls
fn default_relay_urls() -> Vec<RelayUrl> {
    DEFAULT_RELAYS.iter().filter_map(|s| RelayUrl::parse(s).ok()).collect()
}
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
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    error: Option<String>,
}
#[component]
pub fn MusicZapDialog() -> Element {
    let state = MUSIC_PLAYER.read();
    let show_dialog = state.show_zap_dialog;
    let track = state.zap_track.clone();
    if !show_dialog {
        return rsx! {};
    }
    if track.is_none() {
        return rsx! {};
    }
    let track = track.unwrap();
    let is_nostr_track = matches!(
        track.source,
        TrackSource::Nostr { .. }
        | TrackSource::NostrPodcast { .. }
        | TrackSource::Radio { .. }
    );
    let is_v4v_track = matches!(
        track.source,
        TrackSource::RssPodcast { .. } | TrackSource::RssMusic { .. }
    );
    let mut amount = use_signal(|| 100u64);
    let mut comment = use_signal(String::new);
    let mut invoice = use_signal(|| None::<String>);
    let mut is_generating = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut qr_code_url = use_signal(|| None::<String>);
    let mut artist_profile = use_signal(|| None::<profiles::Profile>);
    let preset_amounts = [21, 100, 500, 1000, 2100];
    let track_source_for_effect = track.source.clone();
    use_effect(move || {
        let pubkey_to_fetch = match &track_source_for_effect {
            TrackSource::Nostr { pubkey, .. } => Some(pubkey.clone()),
            TrackSource::Radio { pubkey, .. } => Some(pubkey.clone()),
            TrackSource::NostrPodcast { pubkey, .. } => Some(pubkey.clone()),
            _ => None,
        };
        if let Some(pubkey) = pubkey_to_fetch {
            spawn(async move {
                if let Ok(profile) = profiles::fetch_profile(pubkey).await {
                    artist_profile.set(Some(profile));
                }
            });
        }
    });
    let track_id = track.id.clone();
    let track_source = track.source.clone();
    let track_value_block = track.value_block.clone();
    let generate_invoice = move |e: Event<MouseData>| {
        e.stop_propagation();
        let track_id = track_id.clone();
        let track_source = track_source.clone();
        let track_value_block = track_value_block.clone();
        let amount_value = *amount.read();
        let comment_value = comment.read().clone();
        let profile = artist_profile.read().clone();
        is_generating.set(true);
        error_msg.set(None);
        spawn(async move {
            let result = match track_source {
                TrackSource::Wavlake { .. } => {
                    generate_wavlake_lnurl_invoice(
                            &track_id,
                            amount_value,
                            &comment_value,
                        )
                        .await
                }
                TrackSource::Nostr { ref pubkey, ref coordinate, .. } => {
                    generate_nostr_zap_invoice(
                            pubkey,
                            Some(coordinate),
                            profile.as_ref(),
                            amount_value,
                            &comment_value,
                        )
                        .await
                }
                TrackSource::NostrPodcast { ref pubkey, ref coordinate, .. } => {
                    generate_nostr_zap_invoice(
                            pubkey,
                            Some(coordinate),
                            profile.as_ref(),
                            amount_value,
                            &comment_value,
                        )
                        .await
                }
                TrackSource::RssPodcast { .. } => {
                    if let Some(ref value_block) = track_value_block {
                        generate_v4v_invoice(value_block, amount_value, &comment_value)
                            .await
                    } else {
                        Err(
                            "This podcast doesn't have V4V payment info configured. Contact the podcast creator to enable Lightning payments."
                                .to_string(),
                        )
                    }
                }
                TrackSource::RssMusic { .. } => {
                    if let Some(ref value_block) = track_value_block {
                        generate_v4v_invoice(value_block, amount_value, &comment_value)
                            .await
                    } else {
                        Err(
                            "This music doesn't have V4V payment info configured. Contact the artist to enable Lightning payments."
                                .to_string(),
                        )
                    }
                }
                TrackSource::Radio { ref pubkey, ref coordinate, .. } => {
                    generate_nostr_zap_invoice(
                            pubkey,
                            Some(coordinate),
                            profile.as_ref(),
                            amount_value,
                            &comment_value,
                        )
                        .await
                }
            };
            match result {
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
    let pay_with_webln = move |e: Event<MouseData>| {
        e.stop_propagation();
        if let Some(inv) = invoice.read().clone() {
            spawn(async move {
                #[cfg(feature = "web")]
                {
                    let inv_json = serde_json::to_string(&inv).unwrap_or_default();
                    let script = format!(
                        r#"
                        (async function() {{
                            if (typeof window.webln !== 'undefined') {{
                                try {{
                                    await window.webln.enable();
                                    const result = await window.webln.sendPayment({});
                                    return {{ success: true, preimage: result.preimage }};
                                }} catch (e) {{
                                    return {{ success: false, error: e.message }};
                                }}
                            }} else {{
                                return {{ success: false, error: 'WebLN not available' }};
                            }}
                        }})()
                        "#,
                        inv_json,
                    );
                    match js_sys::eval(&script) {
                        Ok(promise) => {
                            match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise)).await {
                                Ok(result) => {
                                    // Check success field in WebLN result - must be explicitly true
                                    let success = js_sys::Reflect::get(&result, &"success".into())
                                        .ok()
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);
                                    if success {
                                        log::info!("WebLN payment completed successfully");
                                    } else {
                                        let js_error = js_sys::Reflect::get(&result, &"error".into())
                                            .ok()
                                            .and_then(|v| v.as_string())
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        log::warn!("WebLN payment returned success=false: {}", js_error);
                                        error_msg.set(Some(js_error));
                                    }
                                }
                                Err(e) => {
                                    let err_str = format!("{:?}", e);
                                    log::error!("WebLN payment rejected: {}", err_str);
                                    error_msg.set(Some(err_str));
                                }
                            }
                        }
                        Err(e) => {
                            let err_str = format!("{:?}", e);
                            log::error!("WebLN eval failed: {}", err_str);
                            error_msg.set(Some(err_str));
                        }
                    }
                }
                #[cfg(not(feature = "web"))]
                let _ = inv;
            });
        }
    };
    rsx! {
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
                div { class: "w-full max-w-md",
                    div {
                        class: "bg-card border border-border rounded-lg shadow-lg p-6",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        div { class: "flex items-center justify-between mb-4",
                            div {
                                h3 { class: "text-lg font-semibold flex items-center gap-2",
                                    "Zap Artist"
                                    if is_nostr_track {
                                        span { class: "text-xs px-2 py-0.5 rounded-full bg-purple-500/20 text-purple-400",
                                            "Nostr"
                                        }
                                    } else if is_v4v_track {
                                        span { class: "text-xs px-2 py-0.5 rounded-full bg-green-500/20 text-green-400",
                                            "V4V"
                                        }
                                    } else {
                                        span { class: "text-xs px-2 py-0.5 rounded-full bg-orange-500/20 text-orange-400",
                                            "Wavlake"
                                        }
                                    }
                                }
                                p { class: "text-sm text-muted-foreground",
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
                        if let Some(error) = error_msg.read().as_ref() {
                            div { class: "mb-4 p-3 bg-destructive/10 text-destructive rounded-md text-sm",
                                "{error}"
                            }
                        }
                        if invoice.read().is_none() {
                            div { class: "space-y-4",
                                div {
                                    p { class: "text-sm font-medium mb-2", "Select Amount (sats)" }
                                    div { class: "flex flex-wrap gap-2",
                                        for preset in preset_amounts.iter() {
                                            {
                                                let is_selected = *amount.read() == *preset;
                                                let preset_val = *preset;
                                                rsx! {
                                                    button {
                                                        key: "{preset}",
                                                        class: if is_selected { "px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors" } else { "px-4 py-2 rounded-md bg-muted hover:bg-muted/80 transition-colors" },
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
                                div {
                                    label { class: "text-sm font-medium", "Custom Amount" }
                                    input {
                                        r#type: "number",
                                        value: "{amount}",
                                        class: "w-full mt-1 px-3 py-2 border border-border rounded-md bg-background",
                                        oninput: move |evt| {
                                            if let Ok(val) = evt.value().parse::<u64>() {
                                                amount.set(val);
                                            }
                                        },
                                    }
                                }
                                div {
                                    label { class: "text-sm font-medium", "Comment (optional)" }
                                    textarea {
                                        value: "{comment}",
                                        placeholder: "Great track!",
                                        class: "w-full mt-1 px-3 py-2 border border-border rounded-md bg-background",
                                        rows: "3",
                                        oninput: move |evt| comment.set(evt.value()),
                                    }
                                }
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
                            div { class: "space-y-4",
                                if let Some(qr_url) = qr_code_url.read().as_ref() {
                                    div { class: "flex justify-center",
                                        img {
                                            src: "{qr_url}",
                                            alt: "Invoice QR Code",
                                            class: "w-64 h-64",
                                            loading: "lazy",
                                        }
                                    }
                                }
                                div {
                                    label { class: "text-sm font-medium", "Lightning Invoice" }
                                    div { class: "mt-1 p-3 bg-muted rounded-md break-all text-xs font-mono",
                                        "{invoice.read().as_ref().unwrap()}"
                                    }
                                }
                                div { class: "space-y-2",
                                    if cfg!(feature = "web") {
                                        button {
                                            class: "w-full py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors",
                                            onclick: pay_with_webln,
                                            "⚡ Pay with WebLN"
                                        }
                                        button {
                                            class: "w-full py-2 bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80 transition-colors",
                                            onclick: {
                                                let inv = invoice.read().clone();
                                                move |e: Event<MouseData>| {
                                                    e.stop_propagation();
                                                    if let Some(invoice_str) = inv.as_ref() {
                                                        let url = format!("lightning:{}", invoice_str);
                                                        #[cfg(feature = "web")]
                                                        {
                                                            let _ = web_sys::window()
                                                                .and_then(|w| w.open_with_url_and_target(&url, "_blank").ok());
                                                        }
                                                        #[cfg(not(feature = "web"))]
                                                        let _ = url;
                                                    }
                                                }
                                            },
                                            "Open in Wallet"
                                        }
                                    } else {
                                        p {
                                            class: "text-sm text-muted-foreground text-center py-2",
                                            "Copy the invoice above to pay with your Lightning wallet."
                                        }
                                    }
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
                    }
                }
            }
        }
    }
}
/// Generate invoice for Wavlake tracks using LNURL-pay flow
async fn generate_wavlake_lnurl_invoice(
    track_id: &str,
    amount_sats: u64,
    comment: &str,
) -> Result<(String, String), String> {
    let track_id = track_id.trim();
    log::info!(
        "Starting invoice flow for track: {}, amount: {} sats", track_id, amount_sats
    );
    let api = WavlakeAPI::new();
    let lnurl_response = api
        .get_lnurl(track_id, Some("nostrmusic"))
        .await
        .map_err(|e| format!("Failed to get LNURL: {}", e))?;
    log::info!("Received LNURL: {}", lnurl_response.lnurl);
    let lnurl_pay_url = decode_lnurl(&lnurl_response.lnurl)
        .map_err(|e| format!("Failed to decode LNURL: {}", e))?;
    log::info!("Decoded LNURL to: {}", lnurl_pay_url);
    log::info!("Fetching LNURL-pay parameters from: {}", lnurl_pay_url);
    let response = crate::platform::http::http_client()
        .get(&lnurl_pay_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch LNURL-pay params: {}", e))?;
    let params: LnurlPayParams = response
        .error_for_status()
        .map_err(|e| format!("LNURL-pay server error: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse LNURL-pay params: {}", e))?;
    log::info!(
        "LNURL-pay params received. Callback: {}, min: {}, max: {}", params.callback,
        params.min_sendable, params.max_sendable
    );
    let amount_millisats = amount_sats
        .checked_mul(1000)
        .ok_or_else(|| "Amount overflow - too many sats".to_string())?;
    if amount_millisats < params.min_sendable || amount_millisats > params.max_sendable {
        return Err(
            format!(
                "Amount must be between {} and {} sats",
                params.min_sendable / 1000,
                params.max_sendable / 1000,
            ),
        );
    }
    let mut callback_url = params.callback.clone();
    let separator = if callback_url.contains('?') { "&" } else { "?" };
    callback_url.push_str(&format!("{}amount={}", separator, amount_millisats));
    if !comment.is_empty() {
        if let Some(max_comment) = params.comment_allowed {
            if comment.len() <= max_comment as usize {
                callback_url
                    .push_str(&format!("&comment={}", urlencoding::encode(comment)));
            }
        }
    }
    log::info!("Requesting invoice from callback: {}", callback_url);
    let response = crate::platform::http::http_client()
        .get(&callback_url)
        .send()
        .await
        .map_err(|e| format!("Failed to request invoice: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Invoice request failed: {}", e))?;
    let response_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;
    log::info!("Invoice callback response: {}", response_text);
    let invoice_response: InvoiceResponse = serde_json::from_str(&response_text)
        .map_err(|e| {
            format!(
                "Failed to parse invoice response: {}. Response body: {}",
                e,
                response_text,
            )
        })?;
    if let Some(error) = &invoice_response.error {
        return Err(format!("Invoice generation failed: {}", error));
    }
    if let Some(status) = &invoice_response.status {
        if status.to_uppercase() == "ERROR" {
            let reason = invoice_response.reason.as_deref().unwrap_or("Unknown error");
            return Err(format!("Invoice generation failed: {}", reason));
        }
    }
    let pr = invoice_response
        .pr
        .ok_or_else(|| {
            format!("No invoice in response. Response body: {}", response_text)
        })?;
    let qr_code_url = generate_qr_code(&pr)
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;
    Ok((pr, qr_code_url))
}
/// Decode bech32 LNURL to HTTPS URL
fn decode_lnurl(lnurl: &str) -> Result<String, String> {
    let (_, data) = bech32::decode(lnurl)
        .map_err(|e| format!("Bech32 decode error: {}", e))?;
    String::from_utf8(data).map_err(|e| format!("UTF-8 conversion error: {}", e))
}
/// Generate QR code as data URL
fn generate_qr_code(invoice: &str) -> Result<String, String> {
    use base64::Engine;
    use qrcode::render::svg;
    use qrcode::QrCode;
    let code = QrCode::new(invoice.to_uppercase())
        .map_err(|e| format!("QR code generation error: {}", e))?;
    let svg_string = code.render::<svg::Color>().min_dimensions(256, 256).build();
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(svg_string.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{}", encoded))
}
/// Generate invoice for Nostr tracks using NIP-57 zap flow
async fn generate_nostr_zap_invoice(
    artist_pubkey: &str,
    track_coordinate: Option<&String>,
    profile: Option<&profiles::Profile>,
    amount_sats: u64,
    comment: &str,
) -> Result<(String, String), String> {
    log::info!(
        "Starting NIP-57 zap flow for artist: {}, amount: {} sats", artist_pubkey,
        amount_sats
    );
    let profile = profile.ok_or_else(|| "Artist profile not loaded yet".to_string())?;
    let lud16 = profile
        .lud16
        .as_deref()
        .ok_or_else(|| "This artist hasn't set up a Lightning address".to_string())?;
    let (pay_info, amount_msats) = lnurl::prepare_zap(Some(lud16), None, amount_sats)
        .await
        .map_err(|e| format!("Failed to prepare zap: {}", e))?;
    log::info!("LNURL pay info received for nostr zap. Callback: {}", pay_info.callback);
    let recipient_pubkey = PublicKey::parse(artist_pubkey)
        .map_err(|e| format!("Invalid artist pubkey: {}", e))?;
    let relays: Vec<RelayUrl> = {
        if let Some(client) = nostr_client::get_client() {
            let client_relays = client.relays().await;
            let mut urls: Vec<RelayUrl> = client_relays.keys().cloned().collect();
            if urls.is_empty() {
                default_relay_urls()
            } else {
                urls.truncate(5);
                urls
            }
        } else {
            default_relay_urls()
        }
    };
    let message = if comment.is_empty() { None } else { Some(comment.to_string()) };
    let event_coordinate = track_coordinate
        .and_then(|coord| Coordinate::parse(coord).ok());
    let builder = lnurl::create_zap_request_unsigned(
        recipient_pubkey,
        relays,
        amount_msats,
        message,
        None,
        event_coordinate,
    );
    let client = nostr_client::get_client()
        .ok_or_else(|| "Nostr client not available".to_string())?;
    let zap_request = client
        .sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign zap request: {}", e))?;
    log::info!("Zap request event created: {}", zap_request.id.to_hex());
    let invoice_response = lnurl::request_zap_invoice(
            &pay_info.callback,
            amount_msats,
            &zap_request,
            None,
        )
        .await
        .map_err(|e| format!("Failed to get zap invoice: {}", e))?;
    let qr_code_url = generate_qr_code(&invoice_response.pr)
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;
    Ok((invoice_response.pr, qr_code_url))
}
/// Generate invoice for V4V podcast payments
/// Currently supports Lightning Address recipients only
/// Keysend (node pubkey) requires a Lightning node connection
async fn generate_v4v_invoice(
    value_block: &ValueBlock,
    amount_sats: u64,
    comment: &str,
) -> Result<(String, String), String> {
    log::info!(
        "Starting V4V payment flow for {} sats with {} recipients", amount_sats,
        value_block.recipients.len()
    );
    if value_block.recipients.is_empty() {
        return Err("No payment recipients configured for this podcast".to_string());
    }
    let total_split: u32 = value_block
        .recipients
        .iter()
        .try_fold(0u32, |acc, r| acc.checked_add(r.split))
        .ok_or("Split values overflow - invalid podcast configuration")?;
    if total_split == 0 {
        return Err("Invalid split configuration - total is zero".to_string());
    }
    let lnaddress_recipient = value_block
        .recipients
        .iter()
        .find(|r| r.recipient_type == "lnaddress" || r.address.contains('@'));
    let primary_recipient = lnaddress_recipient
        .or_else(|| {
            log::debug!(
                "[V4V] No lnaddress recipient found, falling back to first recipient"
            );
            value_block.recipients.first()
        });
    let recipient = primary_recipient
        .ok_or_else(|| "No valid recipient found".to_string())?;
    let is_lnaddress = recipient.recipient_type == "lnaddress"
        || recipient.address.contains('@');
    if !is_lnaddress {
        return Err(
            format!(
                "Direct keysend payments to node {} not yet supported in browser. \
            The podcast creator can add a Lightning Address for web payments.",
                recipient.address,
            ),
        );
    }
    let lnaddress = &recipient.address;
    let recipient_share = amount_sats
        .checked_mul(recipient.split as u64)
        .and_then(|v| v.checked_add(total_split as u64 / 2))
        .map(|v| v / total_split as u64)
        .ok_or("Arithmetic overflow calculating recipient share")?
        .max(1);
    let recipient_name = recipient.name.as_deref().unwrap_or("Podcast Creator");
    let split_percentage = (recipient.split as f64 * 100.0) / total_split as f64;
    log::info!(
        "Generating invoice for {} ({}) - {} sats ({:.1}% split)", recipient_name,
        lnaddress, recipient_share, split_percentage
    );
    let full_comment = if comment.is_empty() {
        format!("V4V boost to {}", recipient_name)
    } else {
        comment.to_string()
    };
    let (pay_info, amount_msats) = lnurl::prepare_zap(
            Some(lnaddress),
            None,
            recipient_share,
        )
        .await
        .map_err(|e| {
            format!("Failed to resolve Lightning Address '{}': {}", lnaddress, e)
        })?;
    log::info!("Lightning Address resolved. Callback: {}", pay_info.callback);
    let mut callback_url = pay_info.callback.clone();
    let separator = if callback_url.contains('?') { "&" } else { "?" };
    callback_url.push_str(&format!("{}amount={}", separator, amount_msats));
    if !full_comment.is_empty() {
        if let Some(max_comment) = pay_info.comment_allowed {
            let max = max_comment as usize;
            let char_count = full_comment.chars().count();
            if char_count <= max {
                callback_url
                    .push_str(
                        &format!("&comment={}", urlencoding::encode(&full_comment)),
                    );
            } else {
                let truncated: String = full_comment.chars().take(max).collect();
                callback_url
                    .push_str(&format!("&comment={}", urlencoding::encode(&truncated)));
            }
        }
    }
    log::info!("Requesting invoice from: {}", callback_url);
    let client = crate::platform::http::http_client();
    let response = client
        .get(&callback_url)
        .send()
        .await
        .map_err(|e| format!("Failed to request invoice: {}", e))?;
    let response = response
        .error_for_status()
        .map_err(|e| format!("Invoice request failed: {}", e))?;
    let invoice_response: InvoiceResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse invoice response: {}", e))?;
    if let Some(error) = &invoice_response.error {
        return Err(format!("Invoice generation failed: {}", error));
    }
    if let Some(status) = &invoice_response.status {
        if status.to_uppercase() == "ERROR" {
            let reason = invoice_response.reason.as_deref().unwrap_or("Unknown error");
            return Err(format!("Invoice generation failed: {}", reason));
        }
    }
    let pr = invoice_response.pr.ok_or_else(|| "No invoice in response".to_string())?;
    let qr_code_url = generate_qr_code(&pr)
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;
    Ok((pr, qr_code_url))
}
