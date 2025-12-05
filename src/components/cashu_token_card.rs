//! Cashu Token Card Component
//!
//! Renders an interactive card for Cashu ecash tokens found in note content.
//! Supports both V3 (cashuA) and V4 (cashuB) token formats.

use dioxus::prelude::*;
use std::str::FromStr;
use wasm_bindgen::JsValue;

use crate::stores::nostr_client::HAS_SIGNER;

/// Format sats amount with thousands separators
fn format_sats(amount: u64) -> String {
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
    unit: String,
}

/// Parse a Cashu token string
fn parse_token(token: &str) -> Option<ParsedTokenInfo> {
    use cdk::nuts::Token;

    let parsed = Token::from_str(token).ok()?;
    let amount = parsed.value().ok()?;
    let mint_url = parsed.mint_url().ok()?.to_string();
    let unit = parsed.unit().map(|u| format!("{:?}", u).to_lowercase()).unwrap_or_else(|| "sat".to_string());

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
    let mut is_claiming = use_signal(|| false);
    let mut claim_result = use_signal(|| None::<Result<u64, String>>);
    let mut copied = use_signal(|| false);

    let has_signer = *HAS_SIGNER.read();

    // Parse token once
    let parsed = parse_token(&token);

    // Handle claim action
    let handle_claim = {
        let token = token.clone();
        move |e: MouseEvent| {
            e.stop_propagation();

            if *is_claiming.read() || claim_result.read().is_some() {
                return;
            }

            let token = token.clone();
            is_claiming.set(true);

            spawn(async move {
                match crate::stores::cashu::receive_tokens(token).await {
                    Ok(amount) => {
                        log::info!("Successfully claimed {} sats", amount);
                        claim_result.set(Some(Ok(amount)));
                    }
                    Err(e) => {
                        log::error!("Failed to claim token: {}", e);
                        claim_result.set(Some(Err(e)));
                    }
                }
                is_claiming.set(false);
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
                let _ = window.location().set_href(&url);
            }
        }
    };

    // Handle copy button
    let handle_copy = {
        let token = token.clone();
        move |e: MouseEvent| {
            e.stop_propagation();

            let token = token.clone();
            spawn(async move {
                if copy_to_clipboard(&token).await.is_ok() {
                    copied.set(true);
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    copied.set(false);
                }
            });
        }
    };

    // Render based on parse result
    if let Some(info) = parsed {
        let mint_display = extract_mint_hostname(&info.mint_url);
        let amount_display = format_sats(info.amount);
        let unit_display = if info.unit == "sat" { "sats" } else { &info.unit };

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
                if let Some(result) = claim_result.read().as_ref() {
                    match result {
                        Ok(amount) => rsx! {
                            div {
                                class: "mb-3 p-2 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-200 rounded-lg text-center text-sm",
                                "Claimed {format_sats(*amount)} sats!"
                            }
                        },
                        Err(error) => rsx! {
                            div {
                                class: "mb-3 p-2 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-200 rounded-lg text-center text-sm",
                                "{error}"
                            }
                        },
                    }
                }

                // Action buttons
                div {
                    class: "flex items-center justify-center gap-2",

                    // Claim button
                    if claim_result.read().as_ref().map(|r| r.is_ok()).unwrap_or(false) {
                        // Already claimed - show disabled button
                        button {
                            class: "px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded-full text-sm font-medium cursor-not-allowed",
                            disabled: true,
                            "Claimed"
                        }
                    } else if !has_signer {
                        // Not signed in
                        button {
                            class: "px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded-full text-sm font-medium cursor-not-allowed",
                            disabled: true,
                            title: "Sign in to claim",
                            "Claim"
                        }
                    } else {
                        // Can claim
                        button {
                            class: if *is_claiming.read() {
                                "px-4 py-2 bg-amber-500 text-white rounded-full text-sm font-medium opacity-75 cursor-wait"
                            } else {
                                "px-4 py-2 bg-amber-500 hover:bg-amber-600 text-white rounded-full text-sm font-medium transition"
                            },
                            disabled: *is_claiming.read(),
                            onclick: handle_claim,
                            if *is_claiming.read() {
                                "Claiming..."
                            } else {
                                "Claim"
                            }
                        }
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
