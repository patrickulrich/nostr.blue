//! Add-relay input with autocomplete suggestions (issue #359).
//!
//! Replaces the duplicated per-section add-relay blocks on
//! `/settings/relays`. Owns validation + normalization (secure `wss://`
//! only, or `ws://`-tolerant for the Local/Broadcast sections), duplicate
//! detection against the section's current list, and a suggestion dropdown
//! sourced from [`crate::stores::relay::suggestions::RELAY_SUGGESTIONS`].

use crate::stores::relay::suggestions::{self, RelaySuggestion, MAX_DROPDOWN_SUGGESTIONS};
use crate::utils::format_bytes;
use crate::utils::relay::normalize_known_relay_url;
use dioxus::prelude::*;
use url::Url;

/// Normalize a general relay URL: must be `wss://`.
fn normalize_relay_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    if let Ok(url) = nostr::Url::parse(trimmed) {
        let scheme = url.scheme();
        if scheme == "wss" {
            return Ok(url.to_string());
        }
        if scheme == "ws" {
            return Err(
                "Insecure ws:// is not supported. Use wss:// for secure connections.".to_string(),
            );
        }
        return Err("Unsupported URL scheme (use wss://)".to_string());
    }
    if let Ok(url) = nostr::Url::parse(&format!("wss://{}", trimmed)) {
        return Ok(url.to_string());
    }
    Err("Invalid relay URL".to_string())
}

/// Normalize a local relay URL: allows `ws://` and infers the scheme from
/// whether the host looks like a local/LAN address.
fn normalize_local_relay_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    let lower = trimmed.to_lowercase();
    if lower.starts_with("ws://") || lower.starts_with("wss://") {
        return Ok(trimmed.to_string());
    }
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.contains("://") {
        return Err("Unsupported URL scheme (use ws:// or wss://)".to_string());
    }
    fn is_private_172(host: &str) -> bool {
        if let Some(rest) = host.strip_prefix("172.") {
            if let Some(second_octet) = rest.split('.').next() {
                if let Ok(n) = second_octet.parse::<u8>() {
                    return (16..=31).contains(&n);
                }
            }
        }
        false
    }
    let is_local = lower.contains("127.0.0.1")
        || lower.contains("localhost")
        || lower.contains("192.168.")
        || lower.starts_with("10.")
        || is_private_172(&lower)
        || lower.contains("[::1]")
        || lower.contains("::1:")
        || lower.ends_with(".local")
        || lower.contains(".local:")
        || lower.contains(".local/")
        || lower.contains("umbrel:");
    let scheme = if is_local { "ws://" } else { "wss://" };
    Ok(format!("{}{}", scheme, trimmed))
}

/// Validate + normalize for the section kind, including the final
/// parseability check used by the local sections.
fn normalize_for(input: &str, allow_insecure: bool) -> Result<String, String> {
    if allow_insecure {
        let normalized = normalize_local_relay_url(input)?;
        match Url::parse(&normalized) {
            Ok(parsed)
                if matches!(parsed.scheme(), "ws" | "wss") && parsed.host_str().is_some() =>
            {
                Ok(normalized)
            }
            _ => Err("Invalid relay URL".to_string()),
        }
    } else {
        normalize_relay_url(input)
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct RelayUrlInputProps {
    /// Input text signal (owned by the parent section).
    pub text: Signal<String>,
    /// Error text signal (owned by the parent section; validation and
    /// parent-side save failures both write here).
    pub error: Signal<Option<String>>,
    /// URLs already in this section (duplicate detection + filtering).
    pub existing: Vec<String>,
    /// Input placeholder.
    pub placeholder: &'static str,
    /// Local/Broadcast sections allow `ws://` and LAN addresses.
    #[props(default = false)]
    pub allow_insecure: bool,
    /// Called with a validated, normalized relay URL.
    pub on_add: Callback<String>,
}

#[allow(clippy::too_many_arguments)]
fn try_add(
    mut text: Signal<String>,
    mut error: Signal<Option<String>>,
    existing: &[String],
    allow_insecure: bool,
    on_add: Callback<String>,
    mut focused: Signal<bool>,
) {
    let raw = text.read().clone();
    match normalize_for(&raw, allow_insecure) {
        Ok(normalized) => {
            let normalized_key = normalize_known_relay_url(&normalized);
            if existing
                .iter()
                .any(|url| normalize_known_relay_url(url) == normalized_key)
            {
                error.set(Some("Relay already exists".to_string()));
                return;
            }
            error.set(None);
            text.set(String::new());
            focused.set(false);
            on_add.call(normalized);
        }
        Err(e) => error.set(Some(e)),
    }
}

/// Add-relay input with autocomplete suggestions.
#[component]
pub fn RelayUrlInput(props: RelayUrlInputProps) -> Element {
    let RelayUrlInputProps {
        mut text,
        error,
        existing,
        placeholder,
        allow_insecure,
        on_add,
    } = props;
    let mut focused = use_signal(|| false);

    let existing_for_memo = existing.clone();
    let filtered = use_memo(move || {
        let query = text.read().clone();
        suggestions::filter_suggestions(
            &suggestions::RELAY_SUGGESTIONS.read(),
            &query,
            &existing_for_memo,
            MAX_DROPDOWN_SUGGESTIONS,
        )
    });

    let add_from_click = {
        let existing = existing.clone();
        move |_| {
            try_add(text, error, &existing, allow_insecure, on_add, focused)
        }
    };
    let handle_keydown = {
        let existing = existing.clone();
        move |evt: KeyboardEvent| match evt.key() {
            Key::Escape => focused.set(false),
            Key::Enter => try_add(text, error, &existing, allow_insecure, on_add, focused),
            _ => {}
        }
    };

    rsx! {
        div { class: "flex gap-2",
            div { class: "flex-1 relative",
                input {
                    class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                    r#type: "text",
                    placeholder: "{placeholder}",
                    value: "{text}",
                    autocomplete: "off",
                    onfocus: move |_| focused.set(true),
                    oninput: move |evt| {
                        focused.set(true);
                        text.set(evt.value());
                    },
                    onkeydown: handle_keydown,
                    onblur: move |_| focused.set(false),
                }
                if focused() && !filtered.read().is_empty() {
                    div { class: "absolute z-20 left-0 right-0 mt-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-600 rounded-lg shadow-lg max-h-60 overflow-y-auto",
                        for suggestion in filtered.read().iter() {
                            SuggestionRow {
                                key: "{suggestion.url}",
                                suggestion: suggestion.clone(),
                                on_select: move |url: String| {
                                    text.set(url);
                                    focused.set(false);
                                },
                            }
                        }
                    }
                }
            }
            button {
                class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium text-sm transition",
                onclick: add_from_click,
                "+ Add"
            }
        }
        if let Some(err) = error.read().as_ref() {
            div { class: "mt-2 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                "{err}"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SuggestionRowProps {
    suggestion: RelaySuggestion,
    on_select: Callback<String>,
}

/// One dropdown row. `onmousedown` prevents the input's blur so the click
/// lands before the dropdown unmounts.
#[component]
fn SuggestionRow(props: SuggestionRowProps) -> Element {
    let traffic = if props.suggestion.bytes_received > 0 {
        format!(
            "↓ {}",
            format_bytes(props.suggestion.bytes_received.min(usize::MAX as u64) as usize)
        )
    } else {
        String::new()
    };
    rsx! {
        button {
            class: "w-full text-left px-3 py-2 hover:bg-gray-100 dark:hover:bg-gray-700 text-sm flex items-center justify-between gap-2",
            onmousedown: move |evt| evt.prevent_default(),
            onclick: move |_| {
                props.on_select.call(props.suggestion.url.clone());
            },
            span { class: "font-mono text-gray-900 dark:text-white truncate", "{props.suggestion.label}" }
            span { class: "flex items-center gap-2 shrink-0 text-xs text-gray-500 dark:text-gray-400",
                if !traffic.is_empty() {
                    span { "{traffic}" }
                }
                if let Some(ms) = props.suggestion.rtt_open {
                    span { "{ms}ms" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_relay_url_validates_scheme() {
        assert_eq!(
            normalize_relay_url("wss://relay.example.com").unwrap(),
            "wss://relay.example.com/"
        );
        assert_eq!(
            normalize_relay_url("relay.example.com").unwrap(),
            "wss://relay.example.com/"
        );
        let err = normalize_relay_url("ws://relay.example.com").unwrap_err();
        assert!(err.contains("Insecure ws://"));
        assert!(normalize_relay_url("http://relay.example.com").is_err());
        assert!(normalize_relay_url("  ").is_err());
    }

    #[test]
    fn normalize_local_relay_url_infers_scheme() {
        assert_eq!(
            normalize_local_relay_url("localhost:7777").unwrap(),
            "ws://localhost:7777"
        );
        assert_eq!(
            normalize_local_relay_url("relay.example.com").unwrap(),
            "wss://relay.example.com"
        );
        assert_eq!(
            normalize_local_relay_url("ws://192.168.1.100:4869").unwrap(),
            "ws://192.168.1.100:4869"
        );
        assert!(normalize_local_relay_url("http://example.com").is_err());
    }

    #[test]
    fn normalize_for_local_requires_parseable_ws_url() {
        assert_eq!(
            normalize_for("localhost:7777", true).unwrap(),
            "ws://localhost:7777"
        );
        // scheme given but no host → rejected by final parse check
        assert!(normalize_for("ws://", true).is_err());
        // general sections reject ws://
        assert!(normalize_for("ws://localhost:7777", false).is_err());
    }
}
