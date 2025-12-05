//! Cashu Token Card Component
//!
//! Renders an interactive card for Cashu ecash tokens found in note content.
//! Supports both V3 (cashuA) and V4 (cashuB) token formats.

use dioxus::prelude::*;
use std::str::FromStr;
use std::time::Duration;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use cdk::nuts::CurrencyUnit;

use crate::stores::nostr_client::HAS_SIGNER;

/// State machine for token claim operations
#[derive(Clone, Debug, PartialEq)]
enum ClaimState {
    Idle,
    Claiming,
    Success(u64, String),  // (amount, unit_display)
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

/// Extract hostname from mint URL for display
fn extract_mint_hostname(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// Copy text to clipboard using Web API
async fn copy_to_clipboard(text: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
    let navigator = window.navigator();
    let clipboard = navigator.clipboard();
    wasm_bindgen_futures::JsFuture::from(clipboard.write_text(text))
        .await
        .map(|_| ())
}

/// Parsed token information
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

    let has_signer = *HAS_SIGNER.read();

    // Parse token once
    let parsed = parse_token(&token);

    // Handle claim action
    let handle_claim = {
        let token = token.clone();
        // Capture unit from parsed token for display in success message
        // Use same plural form as header for consistency (CDK's Display returns lowercase singular)
        let unit_for_claim = parsed.as_ref()
            .map(|info| match info.unit {
                CurrencyUnit::Sat => "sats",
                CurrencyUnit::Msat => "msats",
                CurrencyUnit::Usd => "USD",
                CurrencyUnit::Eur => "EUR",
                _ => "units",
            }.to_string())
            .unwrap_or_else(|| "sats".to_string());

        move |e: MouseEvent| {
            e.stop_propagation();

            // Only allow claiming from Idle or Failed state (enables retry)
            if !matches!(*claim_state.read(), ClaimState::Idle | ClaimState::Failed(_)) {
                return;
            }

            let token = token.clone();
            let unit = unit_for_claim.clone();
            claim_state.set(ClaimState::Claiming);

            // Use spawn_local so the task survives component unmount (consistent with comment publishing)
            spawn_local(async move {
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

    // Handle wallet button (open in external wallet)
    let handle_wallet = {
        let token = token.clone();
        move |e: MouseEvent| {
            e.stop_propagation();
            if let Some(window) = web_sys::window() {
                let url = format!("cashu://{}", token);
                // Use open() to avoid disrupting current page if no handler exists
                let _ = window.open_with_url_and_target(&url, "_blank");
            }
        }
    };

    // Handle copy button
    let handle_copy = {
        let token = token.clone();
        let toast_api = toast.clone();
        move |e: MouseEvent| {
            e.stop_propagation();

            let token = token.clone();
            let toast_api = toast_api.clone();
            spawn_local(async move {
                match copy_to_clipboard(&token).await {
                    Ok(_) => {
                        copied.set(true);
                        gloo_timers::future::TimeoutFuture::new(2000).await;
                        copied.set(false);
                    }
                    Err(e) => {
                        log::warn!("Failed to copy to clipboard: {:?}", e);
                        toast_api.error(
                            "Failed to copy".to_string(),
                            ToastOptions::new().duration(Duration::from_secs(2))
                        );
                    }
                }
            });
        }
    };

    // Render based on parse result
    if let Some(info) = parsed {
        let mint_display = extract_mint_hostname(&info.mint_url);
        let amount_display = format_amount(info.amount);
        let unit_display = match info.unit {
            CurrencyUnit::Sat => "sats",
            CurrencyUnit::Msat => "msats",
            CurrencyUnit::Usd => "USD",
            CurrencyUnit::Eur => "EUR",
            _ => "units",
        };

        rsx! {
            div {
                class: "my-2 p-4 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-700 rounded-xl",
                onclick: move |e: MouseEvent| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center gap-2 mb-3",
                    span { class: "text-lg", "🥜" }
                    span { class: "text-sm font-medium text-amber-800 dark:text-amber-200", "Cashu" }
                }

                // Amount
                div {
                    class: "text-center mb-3",
                    span {
                        class: "text-2xl font-bold text-amber-900 dark:text-amber-100",
                        "{amount_display} {unit_display}"
                    }
                }

                // Mint
                div {
                    class: "text-center mb-4",
                    span {
                        class: "text-xs text-amber-700 dark:text-amber-300",
                        "Mint: {mint_display}"
                    }
                }

                // Status messages
                if let ClaimState::Success(amount, unit) = &*claim_state.read() {
                    div {
                        class: "mb-3 p-2 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-200 rounded-lg text-center text-sm",
                        "Claimed {format_amount(*amount)} {unit}!"
                    }
                }
                if let ClaimState::Failed(error_msg) = &*claim_state.read() {
                    div {
                        class: "mb-3 p-2 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-200 rounded-lg text-center text-sm cursor-help",
                        title: "{error_msg}",  // Tooltip with detailed error message
                        "Failed to claim token. Please try again."
                    }
                }

                // Action buttons
                div {
                    class: "flex items-center justify-center gap-2",

                    // Claim button
                    match &*claim_state.read() {
                        ClaimState::Success(_, _) => rsx! {
                            // Already claimed - show disabled button
                            button {
                                class: "px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded-full text-sm font-medium cursor-not-allowed",
                                disabled: true,
                                "Claimed"
                            }
                        },
                        ClaimState::Claiming => rsx! {
                            // Currently claiming
                            button {
                                class: "px-4 py-2 bg-amber-500 text-white rounded-full text-sm font-medium opacity-75 cursor-wait",
                                disabled: true,
                                "Claiming..."
                            }
                        },
                        _ if !has_signer => rsx! {
                            // Not signed in
                            button {
                                class: "px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded-full text-sm font-medium cursor-not-allowed",
                                disabled: true,
                                title: "Sign in to claim",
                                "Claim"
                            }
                        },
                        _ => rsx! {
                            // Idle or Failed - can (re)try claiming
                            button {
                                class: "px-4 py-2 bg-amber-500 hover:bg-amber-600 text-white rounded-full text-sm font-medium transition",
                                onclick: handle_claim,
                                "Claim"
                            }
                        },
                    }

                    // Wallet button
                    button {
                        class: "px-4 py-2 bg-amber-100 dark:bg-amber-800/50 text-amber-800 dark:text-amber-200 hover:bg-amber-200 dark:hover:bg-amber-700/50 rounded-full text-sm font-medium transition",
                        onclick: handle_wallet,
                        "Wallet"
                    }

                    // Copy button
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
        // Invalid token - show minimal card with copy option
        rsx! {
            div {
                class: "my-2 p-4 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-700 rounded-xl",
                onclick: move |e: MouseEvent| e.stop_propagation(),

                // Header with error
                div {
                    class: "flex items-center gap-2 mb-3",
                    span { class: "text-lg", "🥜" }
                    span { class: "text-sm font-medium text-amber-800 dark:text-amber-200", "Cashu Token" }
                }

                div {
                    class: "text-center mb-3 text-sm text-amber-700 dark:text-amber-300",
                    "Unable to parse token"
                }

                // Only show copy button for invalid tokens
                div {
                    class: "flex items-center justify-center",
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
