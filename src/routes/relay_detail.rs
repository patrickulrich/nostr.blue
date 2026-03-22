#[cfg(not(feature = "web"))]
use crate::platform::http::http_client;
use crate::routes::Route;
use crate::stores::{nostr_client, relay};
use crate::utils::format_bytes;
use crate::utils::is_valid_http_url;
use crate::utils::relay::{
    build_known_relay_set, decode_relay_route_id, normalize_known_relay_url, relay_http_url,
};
use dioxus::prelude::*;
#[cfg(not(feature = "web"))]
use futures::StreamExt;
#[cfg(feature = "web")]
use js_sys::{Reflect, Uint8Array};
use nostr_sdk::nips::nip11::{FeeSchedule, Limitation, RelayInformationDocument, RetentionKind};
use nostr_sdk::prelude::JsonUtil;
use nostr_sdk::PublicKey;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use wasm_bindgen_futures::JsFuture;
#[cfg(feature = "web")]
use web_sys::AbortController;
#[cfg(feature = "web")]
use web_sys::{Request, RequestInit, RequestMode, RequestRedirect, Response};

const MAX_NIP11_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, PartialEq)]
struct RelayDetailData {
    relay_url: String,
    http_url: String,
    info: Option<RelayInformationDocument>,
    stats: Option<nostr_client::RelayDisplayInfo>,
    metadata_error: Option<String>,
}

async fn fetch_nip11_document(url: &str) -> Result<RelayInformationDocument, String> {
    let body = fetch_nip11_body(url).await?;
    RelayInformationDocument::from_json(&body)
        .map_err(|e| format!("Failed to parse relay metadata: {}", e))
}

#[cfg(feature = "web")]
async fn fetch_nip11_body(url: &str) -> Result<String, String> {
    use futures::FutureExt;

    let controller = AbortController::new()
        .map_err(|e| format!("Failed to create abort controller: {:?}", e))?;
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    opts.set_redirect(RequestRedirect::Error);
    opts.set_signal(Some(&controller.signal()));

    let request = Request::new_with_str_and_init(url, &opts)
        .map_err(|e| format!("Failed to create relay metadata request: {:?}", e))?;
    request
        .headers()
        .set("Accept", "application/nostr+json")
        .map_err(|e| format!("Failed to set relay metadata headers: {:?}", e))?;

    let window = web_sys::window().ok_or("No window object")?;
    let deadline = crate::platform::timer::sleep_ms(15_000).fuse();
    let request = JsFuture::from(window.fetch_with_request(&request)).fuse();
    futures::pin_mut!(request, deadline);
    let response = futures::select! {
        resp = request => resp,
        _ = deadline => {
            controller.abort();
            return Err("Request timeout".to_string());
        },
    }
    .map_err(|e| format!("Failed to fetch relay metadata: {:?}", e))?;

    let response: Response = response
        .dyn_into()
        .map_err(|_| "Failed to cast relay metadata response".to_string())?;
    if !response.ok() {
        return Err(format!(
            "Relay metadata request failed: {}",
            response.status()
        ));
    }

    let mut bytes = Vec::new();
    let mut total_bytes = 0usize;
    let body = response
        .body()
        .ok_or_else(|| "Relay metadata response body missing".to_string())?;
    let reader = body
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| "Failed to create relay metadata stream reader".to_string())?;
    loop {
        let read = JsFuture::from(reader.read()).fuse();
        futures::pin_mut!(read);
        let chunk = futures::select! {
            read = read => read,
            _ = deadline => {
                controller.abort();
                return Err("Request timeout".to_string());
            },
        }
        .map_err(|e| format!("Failed to read relay metadata body: {:?}", e))?;
        let done = Reflect::get(&chunk, &"done".into())
            .map_err(|e| format!("Failed to inspect relay metadata stream state: {:?}", e))?
            .as_bool()
            .unwrap_or(false);
        if done {
            break;
        }
        let value = Reflect::get(&chunk, &"value".into())
            .map_err(|e| format!("Failed to read relay metadata stream chunk: {:?}", e))?;
        let chunk = Uint8Array::new(&value).to_vec();
        total_bytes = total_bytes
            .checked_add(chunk.len())
            .ok_or_else(|| format!("Relay metadata exceeds {} bytes", MAX_NIP11_BYTES))?;
        if total_bytes > MAX_NIP11_BYTES {
            controller.abort();
            return Err(format!("Relay metadata exceeds {} bytes", MAX_NIP11_BYTES));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|e| format!("Failed to decode relay metadata as UTF-8: {}", e))
}

#[cfg(not(feature = "web"))]
async fn fetch_nip11_body(url: &str) -> Result<String, String> {
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(url)
        .header("Accept", "application/nostr+json")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Request timeout".to_string()
            } else {
                format!("Failed to fetch relay metadata: {}", e)
            }
        })?;

    if !response.status().is_success() {
        return Err(format!(
            "Relay metadata request failed: {}",
            response.status()
        ));
    }

    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    let mut total_bytes = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Failed to stream relay metadata: {}", e))?;
        total_bytes = total_bytes
            .checked_add(chunk.len())
            .ok_or_else(|| format!("Relay metadata exceeds {} bytes", MAX_NIP11_BYTES))?;
        if total_bytes > MAX_NIP11_BYTES {
            return Err(format!("Relay metadata exceeds {} bytes", MAX_NIP11_BYTES));
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body).map_err(|e| format!("Failed to decode relay metadata as UTF-8: {}", e))
}

fn limitation_rows(limitation: &Limitation) -> Vec<(String, String)> {
    let mut rows = Vec::new();

    let mut push_opt = |label: &str, value: Option<String>| {
        if let Some(value) = value {
            rows.push((label.to_string(), value));
        }
    };

    push_opt(
        "Max message length",
        limitation.max_message_length.map(|v| v.to_string()),
    );
    push_opt(
        "Max subscriptions",
        limitation.max_subscriptions.map(|v| v.to_string()),
    );
    push_opt("Max filters", limitation.max_filters.map(|v| v.to_string()));
    push_opt("Max limit", limitation.max_limit.map(|v| v.to_string()));
    push_opt(
        "Max subid length",
        limitation.max_subid_length.map(|v| v.to_string()),
    );
    push_opt(
        "Max event tags",
        limitation.max_event_tags.map(|v| v.to_string()),
    );
    push_opt(
        "Max content length",
        limitation.max_content_length.map(|v| v.to_string()),
    );
    push_opt(
        "Min PoW difficulty",
        limitation.min_pow_difficulty.map(|v| v.to_string()),
    );
    push_opt(
        "Auth required",
        limitation
            .auth_required
            .map(|v| if v { "Yes" } else { "No" }.to_string()),
    );
    push_opt(
        "Payment required",
        limitation
            .payment_required
            .map(|v| if v { "Yes" } else { "No" }.to_string()),
    );
    push_opt(
        "Created_at lower limit",
        limitation
            .created_at_lower_limit
            .map(|v| v.as_secs().to_string()),
    );
    push_opt(
        "Created_at upper limit",
        limitation
            .created_at_upper_limit
            .map(|v| v.as_secs().to_string()),
    );

    rows
}

fn fee_section_rows(fees: &[FeeSchedule]) -> Vec<String> {
    fees.iter()
        .map(|fee| {
            let mut parts = vec![format!("{} {}", fee.amount, fee.unit)];
            if let Some(period) = fee.period {
                parts.push(format!("period {}", period));
            }
            if let Some(kinds) = &fee.kinds {
                if !kinds.is_empty() {
                    parts.push(format!("kinds {}", kinds.join(", ")));
                }
            }
            parts.join(" • ")
        })
        .collect()
}

fn retention_rows(info: &RelayInformationDocument) -> Vec<String> {
    info.retention
        .iter()
        .map(|retention| {
            let mut parts = Vec::new();
            if let Some(kinds) = &retention.kinds {
                if !kinds.is_empty() {
                    let kinds = kinds
                        .iter()
                        .map(|kind| match kind {
                            RetentionKind::Single(kind) => kind.to_string(),
                            RetentionKind::Range(start, end) => format!("{start}-{end}"),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    parts.push(format!("kinds {kinds}"));
                }
            }
            if let Some(time) = retention.time {
                parts.push(format!("time {time}s"));
            }
            if let Some(count) = retention.count {
                parts.push(format!("count {count}"));
            }
            if parts.is_empty() {
                "No retention details".to_string()
            } else {
                parts.join(" • ")
            }
        })
        .collect()
}

#[component]
pub fn RelayDetail(relay_id: String) -> Element {
    let detail = use_resource(move || {
        let relay_id = relay_id.clone();
        async move {
            let relay_url = decode_relay_route_id(&relay_id)?;
            let http_url = relay_http_url(&relay_url)?;
            let normalized_relay_url = normalize_known_relay_url(&relay_url);
            let display_info = nostr_client::get_relay_display_info().await;
            let stats = display_info
                .iter()
                .find(|info| normalize_known_relay_url(&info.url) == normalized_relay_url)
                .cloned();
            let known_relays = build_known_relay_set(Some(&display_info));
            let (info, metadata_error) = if !known_relays.contains(&normalized_relay_url) {
                return Err(format!("Unknown relay: {}", relay_url));
            } else if relay::is_relay_blocked(&relay_url) {
                (
                    None,
                    Some("Relay metadata fetch skipped because this relay is blocked".to_string()),
                )
            } else {
                match fetch_nip11_document(&http_url).await {
                    Ok(info) => (Some(info), None),
                    Err(error) => (None, Some(error)),
                }
            };

            Ok::<RelayDetailData, String>(RelayDetailData {
                relay_url,
                http_url,
                info,
                stats,
                metadata_error,
            })
        }
    });

    rsx! {
        div { class: "max-w-3xl mx-auto px-4 py-6 space-y-6",
            div {
                Link {
                    to: Route::SettingsRelays {},
                    class: "text-blue-600 dark:text-blue-400 hover:underline flex items-center gap-2 mb-4",
                    "← Back to Relay Settings"
                }
                h1 { class: "text-2xl font-bold text-gray-900 dark:text-white", "Relay Details" }
            }

            match &*detail.read() {
                Some(Ok(data)) => {
                    let info = data.info.clone();
                    let stats = data.stats.clone();
                    let metadata_error = data.metadata_error.clone();
                    let relay_url = data.relay_url.clone();
                    let http_url = data.http_url.clone();
                    let limitation = info.as_ref().and_then(|info| info.limitation.clone());
                    let fees = info.as_ref().and_then(|info| info.fees.clone());
                    let pubkey = info.as_ref().and_then(|info| info.pubkey.clone());
                    let limitation_rows = limitation
                        .as_ref()
                        .map(limitation_rows)
                        .unwrap_or_default();
                    let (admission, subscription, publication) = fees
                        .as_ref()
                        .map(|fees| {
                            (
                                fee_section_rows(&fees.admission),
                                fee_section_rows(&fees.subscription),
                                fee_section_rows(&fees.publication),
                            )
                        })
                        .unwrap_or_default();
                    let retention = info
                        .as_ref()
                        .map(retention_rows)
                        .unwrap_or_default();
                    rsx! {
                        div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg overflow-hidden",
                            if let Some(icon) = info
                                .as_ref()
                                .and_then(|info| info.icon.clone())
                                .filter(|icon| is_valid_http_url(icon))
                            {
                                div { class: "p-6 border-b border-border flex items-center gap-4",
                                    img {
                                        class: "w-16 h-16 rounded-lg object-cover bg-muted",
                                        src: "{icon}",
                                        alt: "Relay icon",
                                    }
                                    div { class: "min-w-0",
                                        h2 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                                            "{info.as_ref().and_then(|info| info.name.clone()).unwrap_or_else(|| relay_url.clone())}"
                                        }
                                        p { class: "font-mono text-xs text-muted-foreground break-all", "{relay_url}" }
                                    }
                                }
                            } else {
                                div { class: "p-6 border-b border-border",
                                    h2 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                                        "{info.as_ref().and_then(|info| info.name.clone()).unwrap_or_else(|| relay_url.clone())}"
                                    }
                                    p { class: "font-mono text-xs text-muted-foreground break-all mt-1", "{relay_url}" }
                                }
                            }

                            div { class: "p-6 space-y-6",
                                if let Some(description) = info.as_ref().and_then(|info| info.description.clone()) {
                                    div {
                                        h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Description" }
                                        p { class: "text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap", "{description}" }
                                    }
                                }

                                if let Some(error) = metadata_error {
                                    div { class: "rounded-lg border border-yellow-300 bg-yellow-50 dark:bg-yellow-950/30 dark:border-yellow-800 p-4 text-sm text-yellow-800 dark:text-yellow-200",
                                        "NIP-11 metadata unavailable: {error}"
                                    }
                                }

                                div {
                                    h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Endpoints" }
                                    div { class: "space-y-2 text-sm",
                                        div {
                                            span { class: "font-medium text-gray-900 dark:text-white", "WebSocket: " }
                                            span { class: "font-mono break-all text-gray-700 dark:text-gray-300", "{relay_url}" }
                                        }
                                        div {
                                            span { class: "font-medium text-gray-900 dark:text-white", "NIP-11: " }
                                            a {
                                                href: "{http_url}",
                                                target: "_blank",
                                                rel: "noopener noreferrer",
                                                class: "font-mono break-all text-blue-600 dark:text-blue-400 hover:underline",
                                                "{http_url}"
                                            }
                                        }
                                    }
                                }

                                if info.is_some() {
                                    div { class: "grid gap-4 md:grid-cols-2",
                                        if let Some(software) = info.as_ref().and_then(|info| info.software.clone()) {
                                            div {
                                                h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Software" }
                                                if is_valid_http_url(&software) {
                                                    a {
                                                        href: "{software}",
                                                        target: "_blank",
                                                        rel: "noopener noreferrer",
                                                        class: "text-sm text-blue-600 dark:text-blue-400 hover:underline break-all",
                                                        "{software}"
                                                    }
                                                } else {
                                                    p { class: "text-sm text-gray-700 dark:text-gray-300 break-all", "{software}" }
                                                }
                                            }
                                        }
                                        if let Some(version) = info.as_ref().and_then(|info| info.version.clone()) {
                                            div {
                                                h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Version" }
                                                p { class: "text-sm text-gray-700 dark:text-gray-300", "{version}" }
                                            }
                                        }
                                        if let Some(contact) = info.as_ref().and_then(|info| info.contact.clone()) {
                                            div {
                                                h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Contact" }
                                                p { class: "text-sm text-gray-700 dark:text-gray-300 break-all", "{contact}" }
                                            }
                                        }
                                        if let Some(pubkey) = pubkey.clone() {
                                            div {
                                                h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Admin Pubkey" }
                                                if PublicKey::parse(&pubkey).is_ok() {
                                                    Link {
                                                        to: Route::Profile { pubkey: pubkey.clone() },
                                                        class: "text-sm font-mono text-blue-600 dark:text-blue-400 hover:underline break-all",
                                                        "{pubkey}"
                                                    }
                                                } else {
                                                    p { class: "text-sm font-mono text-gray-700 dark:text-gray-300 break-all", "{pubkey}" }
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Some(nips) = info.as_ref().and_then(|info| info.supported_nips.clone()) {
                                    if !nips.is_empty() {
                                        div {
                                            h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Supported NIPs" }
                                            div { class: "flex flex-wrap gap-2",
                                                for nip in nips {
                                                    span { key: "nip-{nip}", class: "px-2 py-1 rounded bg-muted text-muted-foreground text-xs font-medium", "NIP-{nip}" }
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Some(stats) = stats {
                                    div {
                                        h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Live Connection Stats" }
                                        div { class: "grid gap-3 md:grid-cols-2 text-sm",
                                            div { class: "rounded-lg bg-gray-50 dark:bg-gray-700 p-3",
                                                p { class: "font-medium text-gray-900 dark:text-white", "Status" }
                                                p { class: "text-gray-700 dark:text-gray-300 mt-1", "{stats.status_str()}" }
                                            }
                                            div { class: "rounded-lg bg-gray-50 dark:bg-gray-700 p-3",
                                                p { class: "font-medium text-gray-900 dark:text-white", "Traffic" }
                                                p { class: "text-gray-700 dark:text-gray-300 mt-1", "↓ {format_bytes(stats.bytes_received as u64)} • ↑ {format_bytes(stats.bytes_sent as u64)}" }
                                            }
                                            div { class: "rounded-lg bg-gray-50 dark:bg-gray-700 p-3",
                                                p { class: "font-medium text-gray-900 dark:text-white", "Flags" }
                                                p { class: "text-gray-700 dark:text-gray-300 mt-1",
                                                    if stats.has_read || stats.has_write || stats.is_gossip {
                                                        if stats.has_read { "R " }
                                                        if stats.has_write { "W " }
                                                        if stats.is_gossip { "G" }
                                                    } else {
                                                        "-"
                                                    }
                                                }
                                            }
                                            div { class: "rounded-lg bg-gray-50 dark:bg-gray-700 p-3",
                                                p { class: "font-medium text-gray-900 dark:text-white", "Reliability" }
                                                p { class: "text-gray-700 dark:text-gray-300 mt-1",
                                                    "{stats.successful_connections} successful / {stats.connection_attempts} attempts • {(stats.success_rate * 100.0) as u8}%"
                                                }
                                            }
                                        }
                                    }
                                }

                                if !limitation_rows.is_empty() {
                                    div {
                                        h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Limitations" }
                                        div { class: "rounded-lg border border-border overflow-hidden",
                                            for (index, (label, value)) in limitation_rows.into_iter().enumerate() {
                                                div { key: "limitation-{index}", class: "grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3 px-4 py-3 border-b border-border last:border-b-0 text-sm",
                                                    span { class: "font-medium text-gray-900 dark:text-white", "{label}" }
                                                    span { class: "text-gray-700 dark:text-gray-300 break-all", "{value}" }
                                                }
                                            }
                                        }
                                    }
                                }

                                if !admission.is_empty() || !subscription.is_empty() || !publication.is_empty() {
                                    div {
                                        h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Fee Schedules" }
                                        div { class: "grid gap-4 md:grid-cols-3",
                                            if !admission.is_empty() {
                                                div { class: "rounded-lg bg-gray-50 dark:bg-gray-700 p-3",
                                                    p { class: "font-medium text-gray-900 dark:text-white mb-2", "Admission" }
                                                    for (index, row) in admission.into_iter().enumerate() {
                                                        p { key: "admission-{index}", class: "text-sm text-gray-700 dark:text-gray-300", "{row}" }
                                                    }
                                                }
                                            }
                                            if !subscription.is_empty() {
                                                div { class: "rounded-lg bg-gray-50 dark:bg-gray-700 p-3",
                                                    p { class: "font-medium text-gray-900 dark:text-white mb-2", "Subscription" }
                                                    for (index, row) in subscription.into_iter().enumerate() {
                                                        p { key: "subscription-{index}", class: "text-sm text-gray-700 dark:text-gray-300", "{row}" }
                                                    }
                                                }
                                            }
                                            if !publication.is_empty() {
                                                div { class: "rounded-lg bg-gray-50 dark:bg-gray-700 p-3",
                                                    p { class: "font-medium text-gray-900 dark:text-white mb-2", "Publication" }
                                                    for (index, row) in publication.into_iter().enumerate() {
                                                        p { key: "publication-{index}", class: "text-sm text-gray-700 dark:text-gray-300", "{row}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if !retention.is_empty() {
                                    div {
                                        h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Retention" }
                                        div { class: "space-y-2",
                                            for (index, row) in retention.into_iter().enumerate() {
                                                div { key: "retention-{index}", class: "rounded-lg bg-gray-50 dark:bg-gray-700 p-3 text-sm text-gray-700 dark:text-gray-300", "{row}" }
                                            }
                                        }
                                    }
                                }

                                if info.as_ref().is_some_and(|info| !info.relay_countries.is_empty() || !info.language_tags.is_empty() || !info.tags.is_empty() || info.posting_policy.is_some() || info.payments_url.is_some()) {
                                    div { class: "space-y-4",
                                        if let Some(info) = info.as_ref() {
                                            if !info.relay_countries.is_empty() {
                                                div {
                                                    h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Relay Countries" }
                                                    p { class: "text-sm text-gray-700 dark:text-gray-300", "{info.relay_countries.join(\", \")}" }
                                                }
                                            }
                                            if !info.language_tags.is_empty() {
                                                div {
                                                    h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Languages" }
                                                    p { class: "text-sm text-gray-700 dark:text-gray-300", "{info.language_tags.join(\", \")}" }
                                                }
                                            }
                                            if !info.tags.is_empty() {
                                                div {
                                                    h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Topics" }
                                                    p { class: "text-sm text-gray-700 dark:text-gray-300", "{info.tags.join(\", \")}" }
                                                }
                                            }
                                            if let Some(posting_policy) = info.posting_policy.clone() {
                                                div {
                                                    h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Posting Policy" }
                                                    if is_valid_http_url(&posting_policy) {
                                                        a {
                                                            href: "{posting_policy}",
                                                            target: "_blank",
                                                            rel: "noopener noreferrer",
                                                            class: "text-sm text-blue-600 dark:text-blue-400 hover:underline break-all",
                                                            "{posting_policy}"
                                                        }
                                                    } else {
                                                        p { class: "text-sm text-gray-700 dark:text-gray-300 break-all", "{posting_policy}" }
                                                    }
                                                }
                                            }
                                            if let Some(payments_url) = info.payments_url.clone() {
                                                div {
                                                    h3 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2", "Payments URL" }
                                                    if is_valid_http_url(&payments_url) {
                                                        a {
                                                            href: "{payments_url}",
                                                            target: "_blank",
                                                            rel: "noopener noreferrer",
                                                            class: "text-sm text-blue-600 dark:text-blue-400 hover:underline break-all",
                                                            "{payments_url}"
                                                        }
                                                    } else {
                                                        p { class: "text-sm text-gray-700 dark:text-gray-300 break-all", "{payments_url}" }
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
                Some(Err(error)) => rsx! {
                    div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                        h2 { class: "text-lg font-semibold text-gray-900 dark:text-white mb-2", "Invalid relay" }
                        p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                    }
                },
                None => rsx! {
                    div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6 text-sm text-gray-500 dark:text-gray-400",
                        "Loading relay details..."
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limitation_rows_only_contains_present_values() {
        let limitation = Limitation {
            max_message_length: Some(1234),
            auth_required: Some(true),
            ..Default::default()
        };

        let rows = limitation_rows(&limitation);
        assert!(rows
            .iter()
            .any(|(label, value)| label == "Max message length" && value == "1234"));
        assert!(rows
            .iter()
            .any(|(label, value)| label == "Auth required" && value == "Yes"));
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn fee_section_rows_formats_kinds_and_period() {
        let rows = fee_section_rows(&[FeeSchedule {
            amount: 10,
            unit: "msat".to_string(),
            period: Some(30),
            kinds: Some(vec!["1".to_string(), "7".to_string()]),
        }]);

        assert_eq!(rows[0], "10 msat • period 30 • kinds 1, 7");
    }
}
