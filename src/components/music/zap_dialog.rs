use crate::services::lnurl;
use crate::services::wavlake::WavlakeAPI;
use crate::stores::music_player::{self, MusicPlayerStateStoreExt, MUSIC_PLAYER};
use crate::stores::nostr_client;
use crate::stores::nostr_music::TrackSource;
use crate::stores::nwc_store;
use crate::stores::profiles;
use crate::utils::audio::v4v_payment::{
    self, BoostContext, RecipientPaymentStatus, RecipientSplit,
};
use crate::utils::nips::dip03::{self, ZapVisibility};
use crate::utils::podcast::ValueBlock;
use crate::utils::relay::configured_write_relay_urls;
use dioxus::prelude::*;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::{PublicKey, RelayUrl};
use serde::{Deserialize, Serialize};
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

fn redact_url(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(mut parsed) => {
            if !parsed.username().is_empty() {
                let _ = parsed.set_username("");
            }
            if parsed.password().is_some() {
                let _ = parsed.set_password(None);
            }
            let redacted_pairs = parsed
                .query_pairs()
                .map(|(key, value)| {
                    let needs_redaction = matches!(
                        key.as_ref(),
                        "comment" | "token" | "pr" | "invoice" | "auth" | "authorization"
                    );
                    let redacted_value = if needs_redaction {
                        "[redacted]".to_string()
                    } else {
                        value.into_owned()
                    };
                    (key.into_owned(), redacted_value)
                })
                .collect::<Vec<_>>();
            parsed
                .query_pairs_mut()
                .clear()
                .extend_pairs(redacted_pairs);
            parsed.to_string()
        }
        Err(_) => "[invalid-url]".to_string(),
    }
}

fn redact_body(body: &str) -> String {
    format!("{} bytes", body.len())
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
    let store = MUSIC_PLAYER.resolve();
    let show_dialog = *store.show_zap_dialog().read();
    let track = store.zap_track().cloned();
    if !show_dialog {
        return rsx! {};
    }
    if track.is_none() {
        return rsx! {};
    }
    let track = track.unwrap();
    let is_nostr_track = matches!(
        track.source,
        TrackSource::Nostr { .. } | TrackSource::NostrPodcast { .. } | TrackSource::Radio { .. }
    );
    let is_v4v_track = matches!(
        track.source,
        TrackSource::RssPodcast { .. } | TrackSource::RssMusic { .. }
    );
    let mut amount = use_signal(|| 100u64);
    let mut comment = use_signal(String::new);
    let mut zap_visibility = use_signal(|| ZapVisibility::Public);
    let mut invoice = use_signal(|| None::<String>);
    let mut is_generating = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut qr_code_url = use_signal(|| None::<String>);
    let mut artist_profile = use_signal(|| None::<profiles::Profile>);
    let mut v4v_statuses = use_signal(Vec::<RecipientPaymentStatus>::new);
    let mut v4v_splits = use_signal(Vec::<RecipientSplit>::new);
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
    let track_title = track.title.clone();
    let track_artist = track.artist.clone();
    let track_source = track.source.clone();
    let track_value_block = track.value_block.clone();
    let generate_invoice = move |e: Event<MouseData>| {
        e.stop_propagation();
        let track_id = track_id.clone();
        let track_title = track_title.clone();
        let track_artist = track_artist.clone();
        let track_source = track_source.clone();
        let track_value_block = track_value_block.clone();
        let amount_value = *amount.read();
        let comment_value = comment.read().clone();
        let visibility = *zap_visibility.read();
        let profile = artist_profile.read().clone();
        is_generating.set(true);
        error_msg.set(None);
        invoice.set(None);
        qr_code_url.set(None);
        v4v_statuses.set(Vec::new());
        v4v_splits.set(Vec::new());
        spawn(async move {
            enum InvoiceResult {
                Invoice(String, String),
                V4vDirectPaid,
            }
            let result = match track_source {
                TrackSource::Wavlake { .. } => {
                    generate_wavlake_lnurl_invoice(&track_id, amount_value, &comment_value)
                        .await
                        .map(|(inv, qr)| InvoiceResult::Invoice(inv, qr))
                }
                TrackSource::Nostr {
                    ref pubkey,
                    ref coordinate,
                    ..
                } => {
                    generate_nostr_zap_invoice(
                        pubkey,
                        Some(coordinate),
                        profile.as_ref(),
                        amount_value,
                        &comment_value,
                        visibility,
                    )
                    .await
                    .map(|(inv, qr)| InvoiceResult::Invoice(inv, qr))
                }
                TrackSource::NostrPodcast {
                    ref pubkey,
                    ref addr,
                    ..
                } => {
                    // F4 episodes are regular (event-id addressed); the zap helper
                    // only supports coordinate (a-tag) references, so F4 zaps fall
                    // back to the recipient pubkey only.
                    let coord_opt = match addr {
                        crate::stores::nostr_music::PodcastAddr::Legacy { coordinate, .. } => {
                            Some(coordinate)
                        }
                        crate::stores::nostr_music::PodcastAddr::F4 { .. } => None,
                    };
                    generate_nostr_zap_invoice(
                        pubkey,
                        coord_opt,
                        profile.as_ref(),
                        amount_value,
                        &comment_value,
                        visibility,
                    )
                    .await
                    .map(|(inv, qr)| InvoiceResult::Invoice(inv, qr))
                }
                TrackSource::RssPodcast { .. } => {
                    if let Some(ref value_block) = track_value_block {
                        if nwc_store::is_connected() {
                            execute_v4v_boost_with_status(
                                value_block,
                                amount_value,
                                &comment_value,
                                &track_title,
                                &track_artist,
                                v4v_statuses,
                                v4v_splits,
                            )
                            .await
                            .map(|()| InvoiceResult::V4vDirectPaid)
                        } else {
                            generate_v4v_single_invoice(value_block, amount_value, &comment_value)
                                .await
                                .map(|(inv, qr)| InvoiceResult::Invoice(inv, qr))
                        }
                    } else {
                        Err(
                            "This podcast doesn't have V4V payment info configured. Contact the podcast creator to enable Lightning payments."
                                .to_string(),
                        )
                    }
                }
                TrackSource::RssMusic { .. } => {
                    if let Some(ref value_block) = track_value_block {
                        if nwc_store::is_connected() {
                            execute_v4v_boost_with_status(
                                value_block,
                                amount_value,
                                &comment_value,
                                &track_title,
                                &track_artist,
                                v4v_statuses,
                                v4v_splits,
                            )
                            .await
                            .map(|()| InvoiceResult::V4vDirectPaid)
                        } else {
                            generate_v4v_single_invoice(value_block, amount_value, &comment_value)
                                .await
                                .map(|(inv, qr)| InvoiceResult::Invoice(inv, qr))
                        }
                    } else {
                        Err(
                            "This music doesn't have V4V payment info configured. Contact the artist to enable Lightning payments."
                                .to_string(),
                        )
                    }
                }
                TrackSource::Radio {
                    ref pubkey,
                    ref coordinate,
                    ..
                } => {
                    generate_nostr_zap_invoice(
                        pubkey,
                        Some(coordinate),
                        profile.as_ref(),
                        amount_value,
                        &comment_value,
                        visibility,
                    )
                    .await
                    .map(|(inv, qr)| InvoiceResult::Invoice(inv, qr))
                }
                TrackSource::Bible { .. } => {
                    Err("Bible audio does not support zapping.".to_string())
                }
                TrackSource::Quran { .. } => {
                    Err("Quran audio does not support zapping.".to_string())
                }
            };
            match result {
                Ok(InvoiceResult::Invoice(inv, qr)) => {
                    log::info!("Invoice generated successfully");
                    invoice.set(Some(inv));
                    qr_code_url.set(Some(qr));
                    is_generating.set(false);
                }
                Ok(InvoiceResult::V4vDirectPaid) => {
                    log::info!("V4V boost sent successfully");
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
                    error_msg.set(None);
                    let webln = match js_sys::Reflect::get(&js_sys::global(), &"webln".into()) {
                        Ok(webln) if !webln.is_undefined() && !webln.is_null() => webln,
                        _ => {
                            error_msg.set(Some("WebLN not available".to_string()));
                            return;
                        }
                    };
                    let enable_fn = match js_sys::Reflect::get(&webln, &"enable".into())
                        .ok()
                        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
                    {
                        Some(function) => function,
                        None => {
                            error_msg.set(Some("WebLN enable() is not available".to_string()));
                            return;
                        }
                    };
                    let send_payment_fn = match js_sys::Reflect::get(&webln, &"sendPayment".into())
                        .ok()
                        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
                    {
                        Some(function) => function,
                        None => {
                            error_msg.set(Some("WebLN sendPayment() is not available".to_string()));
                            return;
                        }
                    };
                    let enable_result = enable_fn.call0(&webln);
                    let payment_result = match enable_result {
                        Ok(enable_promise) => {
                            match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(
                                enable_promise,
                            ))
                            .await
                            {
                                Ok(_) => {
                                    let inv_js = wasm_bindgen::JsValue::from_str(&inv);
                                    send_payment_fn.call1(&webln, &inv_js)
                                }
                                Err(e) => Err(e),
                            }
                        }
                        Err(e) => Err(e),
                    };
                    match payment_result {
                        Ok(payment_promise) => match wasm_bindgen_futures::JsFuture::from(
                            js_sys::Promise::from(payment_promise),
                        )
                        .await
                        {
                            Ok(result) => {
                                let js_error = js_sys::Reflect::get(&result, &"error".into())
                                    .ok()
                                    .and_then(|v| v.as_string());
                                if let Some(js_error) = js_error {
                                    log::warn!("WebLN payment returned error: {}", js_error);
                                    error_msg.set(Some(js_error));
                                } else {
                                    error_msg.set(None);
                                    log::info!("WebLN payment completed successfully");
                                    music_player::hide_zap_dialog();
                                    invoice.set(None);
                                    qr_code_url.set(None);
                                }
                            }
                            Err(e) => {
                                let err_str = format!("{:?}", e);
                                log::error!("WebLN payment rejected: {}", err_str);
                                error_msg.set(Some(err_str));
                            }
                        },
                        Err(e) => {
                            let err_str = format!("{:?}", e);
                            log::error!("WebLN invocation failed: {}", err_str);
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
                class: "fixed inset-0 z-[99999] flex items-center justify-center bg-background/80 backdrop-blur-sm overflow-y-auto p-4",
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
                                if is_nostr_track {
                                    div {
                                        label { class: "text-sm font-medium", "Visibility" }
                                        div { class: "grid grid-cols-3 gap-2 mt-1",
                                            for (mode, mode_label) in [
                                                (ZapVisibility::Public, "Public"),
                                                (ZapVisibility::Anonymous, "Anonymous"),
                                                (ZapVisibility::Private, "Private"),
                                            ] {
                                                button {
                                                    class: if *zap_visibility.read() == mode {
                                                        "px-3 py-1.5 rounded bg-primary text-primary-foreground text-sm font-medium"
                                                    } else {
                                                        "px-3 py-1.5 rounded bg-secondary text-secondary-foreground hover:bg-secondary/80 text-sm"
                                                    },
                                                    onclick: move |_| zap_visibility.set(mode),
                                                    "{mode_label}"
                                                }
                                            }
                                        }
                                        p { class: "text-xs text-muted-foreground mt-1",
                                            if *zap_visibility.read() == ZapVisibility::Private {
                                                "Only the artist can see your identity and message"
                                            } else if *zap_visibility.read() == ZapVisibility::Anonymous {
                                                "Your identity is hidden from everyone"
                                            } else {
                                                "Everyone can see your identity and message"
                                            }
                                        }
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
                                        if is_v4v_track && nwc_store::is_connected() {
                                            "Sending Boost..."
                                        } else {
                                            "Generating Invoice..."
                                        }
                                    } else if is_v4v_track && nwc_store::is_connected() {
                                        "Boost {amount} sats"
                                    } else {
                                        "Generate Invoice for {amount} sats"
                                    }
                                }
                                if !v4v_splits.read().is_empty() {
                                    div { class: "mt-3 space-y-2",
                                        div { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide",
                                            "Payment Status"
                                        }
                                        for (idx , split) in v4v_splits.read().iter().enumerate() {
                                            {
                                                let status = v4v_statuses.read().get(idx).cloned().unwrap_or(RecipientPaymentStatus::Pending);
                                                let name = split.recipient.name.clone().unwrap_or_else(|| split.recipient.address.clone());
                                                let sats = split.amount_sats;
                                                let pct = split.percentage;
                                                rsx! {
                                                    div {
                                                        key: "{idx}",
                                                        class: "flex items-center gap-2 p-2 rounded-lg bg-muted/50 text-sm",
                                                        match &status {
                                                            RecipientPaymentStatus::Pending => rsx! {
                                                                span { class: "w-4 h-4 text-muted-foreground", "○" }
                                                            },
                                                            RecipientPaymentStatus::Paying => rsx! {
                                                                span { class: "w-4 h-4 text-amber-500 animate-spin", "⟳" }
                                                            },
                                                            RecipientPaymentStatus::Success => rsx! {
                                                                span { class: "w-4 h-4 text-green-500", "✓" }
                                                            },
                                                            RecipientPaymentStatus::Failed(_) => rsx! {
                                                                span { class: "w-4 h-4 text-red-500", "✗" }
                                                            },
                                                            RecipientPaymentStatus::Skipped(_) => rsx! {
                                                                span { class: "w-4 h-4 text-muted-foreground", "—" }
                                                            },
                                                        }
                                                        div { class: "flex-1 min-w-0 truncate", "{name}" }
                                                        span { class: "text-xs text-muted-foreground",
                                                            "{sats} sats ({pct}%)"
                                                        }
                                                    }
                                                }
                                            }
                                        }
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
        "Starting invoice flow for track: {}, amount: {} sats",
        track_id,
        amount_sats
    );
    let api = WavlakeAPI::new();
    let lnurl_response = api
        .get_lnurl(track_id, Some("nostrmusic"))
        .await
        .map_err(|e| format!("Failed to get LNURL: {}", e))?;
    log::info!("Received LNURL response for track {}", track_id);
    let lnurl_pay_url = decode_lnurl(&lnurl_response.lnurl)
        .map_err(|e| format!("Failed to decode LNURL: {}", e))?;
    lnurl::validate_url(&lnurl_pay_url).map_err(|e| format!("Unsafe LNURL pay URL: {}", e))?;
    log::info!("Decoded LNURL pay URL: {}", redact_url(&lnurl_pay_url));
    log::info!(
        "Fetching LNURL-pay parameters from: {}",
        redact_url(&lnurl_pay_url)
    );
    let response = crate::platform::http::http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
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
    lnurl::validate_url(&params.callback)
        .map_err(|e| format!("Unsafe LNURL callback URL: {}", e))?;
    log::info!(
        "LNURL-pay params received. Callback: {}, min: {}, max: {}",
        redact_url(&params.callback),
        params.min_sendable,
        params.max_sendable
    );
    let amount_millisats = amount_sats
        .checked_mul(1000)
        .ok_or_else(|| "Amount overflow - too many sats".to_string())?;
    let min_sats = params.min_sendable.div_ceil(1000);
    let max_sats = params.max_sendable / 1000;
    if min_sats > max_sats {
        return Err("No whole-sat amount fits the server bounds".to_string());
    }
    if amount_millisats < params.min_sendable || amount_millisats > params.max_sendable {
        return Err(format!(
            "Amount must be between {} and {} sats",
            min_sats, max_sats,
        ));
    }
    let mut callback_url = params.callback.clone();
    let separator = if callback_url.contains('?') { "&" } else { "?" };
    callback_url.push_str(&format!("{}amount={}", separator, amount_millisats));
    if !comment.is_empty() {
        if let Some(max_comment) = params.comment_allowed {
            let truncated_comment = comment
                .chars()
                .take(max_comment as usize)
                .collect::<String>();
            if !truncated_comment.is_empty() {
                callback_url.push_str(&format!(
                    "&comment={}",
                    urlencoding::encode(&truncated_comment)
                ));
            }
        }
    }
    log::info!(
        "Requesting invoice from callback: {}",
        redact_url(&callback_url)
    );
    let response = crate::platform::http::http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
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
    log::info!(
        "Invoice callback response received ({})",
        redact_body(&response_text)
    );
    let invoice_response: InvoiceResponse = serde_json::from_str(&response_text).map_err(|e| {
        format!(
            "Failed to parse invoice response: {}. Response size: {}",
            e,
            redact_body(&response_text)
        )
    })?;
    if let Some(error) = &invoice_response.error {
        return Err(format!("Invoice generation failed: {}", error));
    }
    if let Some(status) = &invoice_response.status {
        if status.to_uppercase() == "ERROR" {
            let reason = invoice_response
                .reason
                .as_deref()
                .unwrap_or("Unknown error");
            return Err(format!("Invoice generation failed: {}", reason));
        }
    }
    let pr = invoice_response.pr.ok_or_else(|| {
        format!(
            "No invoice in response. Response size: {}",
            redact_body(&response_text)
        )
    })?;
    let qr_code_url =
        generate_qr_code(&pr).map_err(|e| format!("Failed to generate QR code: {}", e))?;
    Ok((pr, qr_code_url))
}
/// Decode bech32 LNURL to HTTPS URL
fn decode_lnurl(lnurl: &str) -> Result<String, String> {
    let (_, data) = bech32::decode(lnurl).map_err(|e| format!("Bech32 decode error: {}", e))?;
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
    let encoded = base64::engine::general_purpose::STANDARD.encode(svg_string.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{}", encoded))
}
/// Generate invoice for Nostr tracks using NIP-57 zap flow
async fn generate_nostr_zap_invoice(
    artist_pubkey: &str,
    track_coordinate: Option<&String>,
    profile: Option<&profiles::Profile>,
    amount_sats: u64,
    comment: &str,
    visibility: ZapVisibility,
) -> Result<(String, String), String> {
    log::info!(
        "Starting NIP-57 zap flow for artist: {}, amount: {} sats",
        artist_pubkey,
        amount_sats
    );
    let profile = profile.ok_or_else(|| "Artist profile not loaded yet".to_string())?;
    let lud16_opt = profile.lud16.as_deref();
    let lud06_opt = profile.lud06.as_deref();
    if lud16_opt.is_none() && lud06_opt.is_none() {
        return Err("This artist hasn't set up a Lightning address".to_string());
    }
    let (pay_info, amount_msats) = lnurl::prepare_zap(lud16_opt, lud06_opt, amount_sats)
        .await
        .map_err(|e| format!("Failed to prepare zap: {}", e))?;
    log::info!(
        "LNURL pay info received for nostr zap. Callback: {}",
        redact_url(&pay_info.callback)
    );
    let recipient_pubkey =
        PublicKey::parse(artist_pubkey).map_err(|e| format!("Invalid artist pubkey: {}", e))?;
    let relays: Vec<RelayUrl> = configured_write_relay_urls();
    let message = if comment.is_empty() {
        None
    } else {
        Some(comment.to_string())
    };
    let event_coordinate = match track_coordinate {
        Some(coord) => Some(
            Coordinate::parse(coord)
                .map_err(|e| format!("Invalid track coordinate '{}': {}", coord, e))?,
        ),
        None => None,
    };
    let client =
        nostr_client::get_client().ok_or_else(|| "Nostr client not available".to_string())?;
    let zap_request = match visibility {
        ZapVisibility::Public => {
            let builder = lnurl::create_zap_request_unsigned(
                recipient_pubkey,
                relays,
                amount_msats,
                message,
                None,
                event_coordinate,
            );
            client
                .sign_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
                .await
                .map_err(|e| format!("Failed to sign zap request: {}", e))?
        }
        ZapVisibility::Anonymous => dip03::build_anonymous_zap_request(
            recipient_pubkey,
            relays,
            amount_msats,
            message,
            None,
            event_coordinate,
        )
        .map_err(|e| e.to_string())?,
        ZapVisibility::Private => dip03::build_private_zap_request(
            recipient_pubkey,
            relays,
            amount_msats,
            message,
            None,
            event_coordinate,
        )
        .await
        .map_err(|e| e.to_string())?,
    };
    log::info!("Zap request event created: {}", zap_request.id.to_hex());
    let invoice_response =
        lnurl::request_zap_invoice(&pay_info.callback, amount_msats, &zap_request, None)
            .await
            .map_err(|e| format!("Failed to get zap invoice: {}", e))?;
    let qr_code_url = generate_qr_code(&invoice_response.pr)
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;
    Ok((invoice_response.pr, qr_code_url))
}
async fn execute_v4v_boost_with_status(
    value_block: &ValueBlock,
    amount_sats: u64,
    comment: &str,
    track_title: &str,
    track_artist: &str,
    mut v4v_statuses: Signal<Vec<RecipientPaymentStatus>>,
    mut v4v_splits: Signal<Vec<RecipientSplit>>,
) -> Result<(), String> {
    let ctx = BoostContext {
        podcast_name: Some(track_artist.to_string()),
        episode_name: Some(track_title.to_string()),
        feed_guid: None,
        episode_guid: None,
        feed_url: None,
        sender_name: None,
        message: if comment.is_empty() {
            None
        } else {
            Some(comment.to_string())
        },
    };
    let splits = v4v_payment::calculate_splits(value_block, amount_sats);
    if splits.is_empty() {
        return Err("No valid payment splits calculated".to_string());
    }
    {
        let mut statuses_vec = Vec::with_capacity(splits.len());
        for _ in &splits {
            statuses_vec.push(RecipientPaymentStatus::Pending);
        }
        v4v_statuses.set(statuses_vec);
        v4v_splits.set(splits.clone());
    }
    let total_split: u64 = value_block
        .recipients
        .iter()
        .try_fold(0u64, |acc, r| acc.checked_add(r.split as u64))
        .unwrap_or(0);
    for (idx, split) in splits.iter().enumerate() {
        if split.amount_sats == 0 {
            v4v_statuses.write()[idx] = RecipientPaymentStatus::Skipped("Amount too small".to_string());
            continue;
        }
        v4v_statuses.write()[idx] = RecipientPaymentStatus::Paying;
        match v4v_payment::pay_recipient_via_nwc(split, amount_sats, &ctx).await {
            Ok(()) => {
                v4v_statuses.write()[idx] = RecipientPaymentStatus::Success;
            }
            Err(e) => {
                log::error!("V4V payment failed for {}: {}", split.recipient.address, e);
                v4v_statuses.write()[idx] = RecipientPaymentStatus::Failed(e);
            }
        }
    }
    let statuses = v4v_statuses.read();
    let success_count = statuses
        .iter()
        .filter(|s| matches!(s, RecipientPaymentStatus::Success))
        .count();
    let total = statuses.len();
    if success_count == 0 {
        return Err("All payment attempts failed".to_string());
    }
    if success_count < total {
        log::warn!(
            "V4V boost partially succeeded: {}/{} recipients",
            success_count,
            total
        );
    }
    let _ = total_split;
    Ok(())
}

async fn generate_v4v_single_invoice(
    value_block: &ValueBlock,
    amount_sats: u64,
    comment: &str,
) -> Result<(String, String), String> {
    if value_block.recipients.is_empty() {
        return Err("No payment recipients configured".to_string());
    }
    if value_block.recipients.len() > 1 {
        return Err(
            "Multi-recipient V4V boosts require a wallet connection (NWC). Please connect your wallet in Settings to boost this podcast."
                .to_string(),
        );
    }
    let recipient = &value_block.recipients[0];
    let is_lnaddress = recipient.recipient_type == "lnaddress" || recipient.address.contains('@');
    if !is_lnaddress {
        return Err(format!(
            "Direct keysend payments to node {} require a wallet connection (NWC). Please connect your wallet in Settings.",
            recipient.address,
        ));
    }
    let lnaddress = &recipient.address;
    let total_split = recipient.split.max(1);
    let recipient_share = (amount_sats * recipient.split as u64) / total_split as u64;
    let recipient_name = recipient.name.as_deref().unwrap_or("Podcast Creator");
    let full_comment = if comment.is_empty() {
        format!("V4V boost to {}", recipient_name)
    } else {
        comment.to_string()
    };
    let (pay_info, amount_msats) = lnurl::prepare_zap(Some(lnaddress), None, recipient_share)
        .await
        .map_err(|e| format!("Failed to resolve Lightning Address '{}': {}", lnaddress, e))?;
    lnurl::validate_url(&pay_info.callback)
        .map_err(|e| format!("Unsafe LNURL callback URL: {}", e))?;
    let mut callback_url = pay_info.callback.clone();
    let separator = if callback_url.contains('?') { "&" } else { "?" };
    callback_url.push_str(&format!("{}amount={}", separator, amount_msats));
    if !full_comment.is_empty() {
        if let Some(max_comment) = pay_info.comment_allowed {
            let max = max_comment as usize;
            let char_count = full_comment.chars().count();
            let text = if char_count <= max {
                full_comment.clone()
            } else {
                full_comment.chars().take(max).collect()
            };
            let encoded = urlencoding::encode(&text).to_string();
            if !encoded.is_empty() {
                callback_url.push_str(&format!("&comment={encoded}"));
            }
        }
    }
    let client = crate::platform::http::http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?;
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
            let reason = invoice_response
                .reason
                .as_deref()
                .unwrap_or("Unknown error");
            return Err(format!("Invoice generation failed: {}", reason));
        }
    }
    let pr = invoice_response
        .pr
        .ok_or_else(|| "No invoice in response".to_string())?;
    let qr_code_url =
        generate_qr_code(&pr).map_err(|e| format!("Failed to generate QR code: {}", e))?;
    Ok((pr, qr_code_url))
}
