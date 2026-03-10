//! Cashu Token Card Component
//!
//! Renders an interactive card for Cashu ecash tokens found in note content.
//! Supports both V3 (cashuA) and V4 (cashuB) token formats.
use crate::stores::nostr_client::HAS_SIGNER;
use cdk::nuts::CurrencyUnit;
use dioxus::prelude::*;
use dioxus_core::spawn_forever;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::str::FromStr;
use std::time::Duration;
/// State machine for token claim operations
#[derive(Clone, Debug, PartialEq)]
enum ClaimState {
    Idle,
    Claiming,
    Success(u64, String),
    Failed(String),
}
/// Format amount with thousands separators (works for any currency unit)
fn format_amount(amount: u64) -> String {
    let s = amount.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}

/// Returns pluralized display string for currency unit.
/// CDK's CurrencyUnit::Display returns lowercase singular (sat, msat, usd, eur)
/// but nostr.blue uses plural forms for sats/msats.
fn unit_display(unit: &CurrencyUnit) -> &'static str {
    match unit {
        CurrencyUnit::Sat => "sats",
        CurrencyUnit::Msat => "msats",
        CurrencyUnit::Usd => "USD",
        CurrencyUnit::Eur => "EUR",
        CurrencyUnit::Auth => "auth",
        CurrencyUnit::Custom(_) => "tokens",
        _ => "units",
    }
}

/// Safely truncate error message respecting UTF-8 char boundaries.
fn truncate_error(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len.saturating_sub(3);
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}
/// Extract hostname from mint URL for display
fn extract_mint_hostname(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}
use crate::utils::clipboard::copy_to_clipboard;
/// Parsed token information
#[derive(Clone, PartialEq)]
struct ParsedTokenInfo {
    amount: u64,
    mint_url: String,
    unit: CurrencyUnit,
}
/// Parse a Cashu token string
fn parse_token(token: &str) -> Option<ParsedTokenInfo> {
    use cdk::nuts::Token;
    let parsed = Token::from_str(token).ok()?;
    let amount = parsed.value().ok()?;
    let mint_url = parsed.mint_url().ok()?.to_string();
    let unit = parsed.unit().unwrap_or_default();
    Some(ParsedTokenInfo {
        amount: u64::from(amount),
        mint_url,
        unit,
    })
}
/// Cashu Token Card Component
///
/// Renders an interactive card for Cashu ecash tokens with:
/// - Amount and mint display
/// - Claim button (redeems to user's NIP-60 wallet)
/// - Wallet button (opens in external wallet)
/// - Copy button (copies token to clipboard)
#[component]
pub fn CashuTokenCard(token: String) -> Element {
    let mut claim_state = use_signal(|| ClaimState::Idle);
    let mut copied = use_signal(|| false);
    let toast = consume_toast();
    let has_signer = use_memo(move || *HAS_SIGNER.read());
    let token_for_memo = token.clone();
    let parsed = use_memo(move || parse_token(&token_for_memo));
    let handle_claim = {
        let token = token.clone();
        let unit_for_claim = parsed
            .read()
            .as_ref()
            .map(|info| unit_display(&info.unit).to_string())
            .unwrap_or_else(|| "sats".to_string());
        move |e: MouseEvent| {
            e.stop_propagation();
            if !matches!(
                *claim_state.read(),
                ClaimState::Idle | ClaimState::Failed(_)
            ) {
                return;
            }
            let token = token.clone();
            let unit = unit_for_claim.clone();
            claim_state.set(ClaimState::Claiming);
            spawn_forever(async move {
                match crate::stores::cashu::receive_tokens(token).await {
                    Ok(amount) => {
                        log::info!("Successfully claimed {} {}", amount, unit);
                        claim_state.set(ClaimState::Success(amount, unit));
                    }
                    Err(e) => {
                        log::error!("Failed to claim token: {}", e);
                        claim_state.set(ClaimState::Failed(e));
                    }
                }
            });
        }
    };
    let handle_wallet = {
        let token = token.clone();
        #[allow(unused_variables)]
        let toast_api = toast;
        move |e: MouseEvent| {
            e.stop_propagation();
            #[cfg(feature = "web")]
            {
                if let Some(window) = web_sys::window() {
                    let url = format!("cashu://{}", token);
                    let _ = window.open_with_url_and_target(&url, "_blank");
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = &token;
                toast_api.error(
                    "Open wallet not supported on this platform".to_string(),
                    ToastOptions::new()
                        .duration(Duration::from_secs(3))
                        .permanent(false),
                );
            }
        }
    };
    let handle_copy = {
        let token = token.clone();
        let toast_api = toast;
        move |e: MouseEvent| {
            e.stop_propagation();
            let token = token.clone();
            spawn_forever(async move {
                match copy_to_clipboard(&token).await {
                    Ok(_) => {
                        copied.set(true);
                        crate::platform::timer::sleep_ms(2000).await;
                        copied.set(false);
                    }
                    Err(e) => {
                        log::warn!("Failed to copy to clipboard: {:?}", e);
                        toast_api.error(
                            "Failed to copy".to_string(),
                            ToastOptions::new().duration(Duration::from_secs(2)),
                        );
                    }
                }
            });
        }
    };
    let parsed_info = parsed.read().clone();
    if let Some(info) = parsed_info {
        let mint_display = extract_mint_hostname(&info.mint_url);
        let amount_display = format_amount(info.amount);
        let unit_str = unit_display(&info.unit);
        rsx! {
            div {
                class: "my-2 p-4 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-700 rounded-xl",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div { class: "flex items-center gap-2 mb-3",
                    span { class: "text-lg", "🥜" }
                    span { class: "text-sm font-medium text-amber-800 dark:text-amber-200",
                        "Cashu"
                    }
                }
                div { class: "text-center mb-3",
                    span { class: "text-2xl font-bold text-amber-900 dark:text-amber-100",
                        "{amount_display} {unit_str}"
                    }
                }
                div { class: "text-center mb-4",
                    span { class: "text-xs text-amber-700 dark:text-amber-300", "Mint: {mint_display}" }
                }
                if let ClaimState::Success(amount, unit) = &*claim_state.read() {
                    div { class: "mb-3 p-2 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-200 rounded-lg text-center text-sm",
                        "Claimed {format_amount(*amount)} {unit}!"
                    }
                }
                if let ClaimState::Failed(error_msg) = &*claim_state.read() {
                    {
                        let truncated = truncate_error(error_msg, 50);
                        rsx! {
                            div {
                                class: "mb-3 p-2 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-200 rounded-lg text-center text-sm cursor-help",
                                title: "{error_msg}",
                                p { class: "font-medium", "Failed to claim token" }
                                p { class: "text-xs opacity-75 mt-1", "{truncated}" }
                            }
                        }
                    }
                }
                div { class: "flex items-center justify-center gap-2",
                    match &*claim_state.read() {
                        ClaimState::Success(_, _) => rsx! {
                            button {
                                class: "px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded-full text-sm font-medium cursor-not-allowed",
                                disabled: true,
                                "Claimed"
                            }
                        },
                        ClaimState::Claiming => rsx! {
                            button {
                                class: "px-4 py-2 bg-amber-500 text-white rounded-full text-sm font-medium opacity-75 cursor-wait",
                                disabled: true,
                                "Claiming..."
                            }
                        },
                        _ if !*has_signer.read() => rsx! {
                            button {
                                class: "px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded-full text-sm font-medium cursor-not-allowed",
                                disabled: true,
                                title: "Sign in to claim",
                                "Claim"
                            }
                        },
                        _ => rsx! {
                            button {
                                class: "px-4 py-2 bg-amber-500 hover:bg-amber-600 text-white rounded-full text-sm font-medium transition",
                                onclick: handle_claim,
                                "Claim"
                            }
                        },
                    }
                    button {
                        class: "px-4 py-2 bg-amber-100 dark:bg-amber-800/50 text-amber-800 dark:text-amber-200 hover:bg-amber-200 dark:hover:bg-amber-700/50 rounded-full text-sm font-medium transition",
                        onclick: handle_wallet,
                        title: if cfg!(feature = "web") { "" } else { "Not supported on this platform" },
                        "Wallet"
                    }
                    button {
                        class: "px-4 py-2 bg-amber-100 dark:bg-amber-800/50 text-amber-800 dark:text-amber-200 hover:bg-amber-200 dark:hover:bg-amber-700/50 rounded-full text-sm font-medium transition",
                        onclick: handle_copy,
                        if *copied.read() {
                            "Copied!"
                        } else {
                            "Copy"
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "my-2 p-4 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-700 rounded-xl",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div { class: "flex items-center gap-2 mb-3",
                    span { class: "text-lg", "🥜" }
                    span { class: "text-sm font-medium text-amber-800 dark:text-amber-200",
                        "Cashu Token"
                    }
                }
                div { class: "text-center mb-3 text-sm text-amber-700 dark:text-amber-300",
                    "Unable to parse token"
                }
                div { class: "flex items-center justify-center",
                    button {
                        class: "px-4 py-2 bg-amber-100 dark:bg-amber-800/50 text-amber-800 dark:text-amber-200 hover:bg-amber-200 dark:hover:bg-amber-700/50 rounded-full text-sm font-medium transition",
                        onclick: handle_copy,
                        if *copied.read() {
                            "Copied!"
                        } else {
                            "Copy"
                        }
                    }
                }
            }
        }
    }
}
