//! Nutzap Settings Modal Component
//!
//! UI for configuring nutzap receiving (NIP-61).
//! Allows users to:
//! - Toggle nutzap receiving on/off
//! - Select which mints to accept
//! - Configure delivery relays
//! - Toggle auto-redemption
//! - Publish/update kind:10019 event

use dioxus::prelude::*;
use crate::stores::cashu;
use crate::utils::shorten_url;

#[component]
pub fn NutzapSettingsModal(on_close: EventHandler<()>) -> Element {
    // Current settings
    let my_info = cashu::MY_NUTZAP_INFO.read().clone();
    let nutzap_enabled = *cashu::NUTZAP_ENABLED.read();
    let auto_redeem = *cashu::NUTZAP_AUTO_REDEEM.read();

    // Form state
    let mut selected_mints = use_signal(|| {
        my_info
            .as_ref()
            .map(|info| info.mints.iter().map(|m| m.url.clone()).collect())
            .unwrap_or_else(cashu::get_mints)
    });

    let mut relay_input = use_signal(|| {
        my_info
            .as_ref()
            .map(|info| info.relays.join("\n"))
            .unwrap_or_else(|| {
                // Default to common relays
                "wss://relay.damus.io\nwss://nos.lol".to_string()
            })
    });

    let mut auto_redeem_enabled = use_signal(|| auto_redeem);
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut success_message = use_signal(|| Option::<String>::None);

    // Get all available mints from wallet
    let all_mints = cashu::get_mints();

    // P2PK pubkey (display only)
    let p2pk_pubkey = cashu::get_nutzap_p2pk_pubkey().ok();

    // Toggle mint selection
    // Normalize URLs for consistent comparison (handles trailing slashes, scheme differences)
    let mut toggle_mint = move |mint_url: String| {
        let normalized = cashu::normalize_mint_url(&mint_url);
        let mut mints = selected_mints.write();
        // Use mint_matches helper for comparison when removing
        if mints.iter().any(|m| cashu::mint_matches(m, &normalized)) {
            mints.retain(|m| !cashu::mint_matches(m, &normalized));
        } else {
            mints.push(normalized);
        }
    };

    // Publish nutzap info
    let on_publish = move |_| {
        if *is_publishing.read() {
            return;
        }

        // Filter selected mints against current wallet mints to avoid stale entries
        // Normalize URLs first to handle trailing slashes/scheme differences (CDK MintUrl pattern)
        // Deduplication: track seen URLs to avoid duplicates after normalization
        let current_wallet_mints = cashu::get_mints();
        let mut seen_mints = std::collections::HashSet::new();
        let mints: Vec<cashu::NutzapMint> = selected_mints
            .read()
            .iter()
            .map(|url| cashu::normalize_mint_url(url))  // Normalize first
            .filter(|normalized_url| seen_mints.insert(normalized_url.clone()))  // Deduplicate
            .filter(|normalized_url| current_wallet_mints.iter().any(|m| cashu::mint_matches(m, normalized_url)))
            .map(|normalized_url| cashu::NutzapMint {
                url: normalized_url,  // Store normalized URL
                unit: "sat".to_string(),
            })
            .collect();

        if mints.is_empty() {
            error_message.set(Some("Please select at least one mint".to_string()));
            return;
        }

        // Validate relay URLs using RelayUrl::parse (nostr-sdk pattern)
        // This performs multi-step validation: scheme check, URL structure, ws/wss only
        let (relays, rejected): (Vec<String>, Vec<String>) = relay_input
            .read()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .partition(|l| nostr_sdk::RelayUrl::parse(l).is_ok());

        // Log warning for rejected entries
        if !rejected.is_empty() {
            log::warn!("Rejected relay URLs (invalid scheme): {:?}", rejected);
        }

        // Deduplicate relays after validation
        let relays: Vec<String> = relays
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if relays.is_empty() {
            error_message.set(Some("Please enter at least one valid relay URL (wss:// or ws://)".to_string()));
            return;
        }

        let auto_redeem_setting = *auto_redeem_enabled.read();

        is_publishing.set(true);
        error_message.set(None);
        success_message.set(None);

        spawn(async move {
            match cashu::publish_nutzap_info(mints, relays).await {
                Ok(event_id) => {
                    // Update auto-redeem setting
                    *cashu::NUTZAP_AUTO_REDEEM.write() = auto_redeem_setting;

                    // Persist to localStorage (Dioxus pattern for persistence)
                    #[cfg(target_arch = "wasm32")]
                    {
                        use gloo_storage::{LocalStorage, Storage};
                        let _ = LocalStorage::set("nostr_nutzap_auto_redeem", auto_redeem_setting);
                    }

                    success_message.set(Some(format!(
                        "Nutzap info published! Event: {}...",
                        &event_id[..12.min(event_id.len())]
                    )));

                    // Start subscription if not already active
                    if let Err(e) = cashu::start_nutzap_subscription().await {
                        log::warn!("Failed to start nutzap subscription: {}", e);
                    }
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                }
            }
            is_publishing.set(false);
        });
    };

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg max-w-lg w-full shadow-xl max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between sticky top-0 bg-card",
                    h3 {
                        class: "text-xl font-bold",
                        "Nutzap Settings"
                    }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "x"
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-6",

                    // Status indicator
                    div {
                        class: "flex items-center justify-between p-4 bg-accent/50 rounded-lg",
                        div {
                            h4 { class: "font-semibold", "Nutzap Receiving" }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Allow others to send you ecash tips via Nostr"
                            }
                        }
                        div {
                            class: if nutzap_enabled {
                                "px-3 py-1 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400 rounded-full text-sm font-medium"
                            } else {
                                "px-3 py-1 bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400 rounded-full text-sm font-medium"
                            },
                            if nutzap_enabled { "Enabled" } else { "Disabled" }
                        }
                    }

                    // P2PK Pubkey (display only)
                    if let Some(pubkey) = &p2pk_pubkey {
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "Your P2PK Pubkey"
                            }
                            div {
                                class: "px-4 py-2 bg-background border border-border rounded-lg font-mono text-xs break-all",
                                "{pubkey}"
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-1",
                                "Others will use this pubkey to lock tokens to you"
                            }
                        }
                    }

                    // Mint selection
                    div {
                        label {
                            class: "block text-sm font-semibold mb-2",
                            "Accepted Mints"
                        }
                        p {
                            class: "text-xs text-muted-foreground mb-3",
                            "Select mints you want to accept nutzaps from. Senders must have tokens at one of these mints."
                        }
                        div {
                            class: "space-y-2",
                            if all_mints.is_empty() {
                                p {
                                    class: "text-sm text-amber-600 dark:text-amber-400",
                                    "No mints configured. Add a mint to your wallet first."
                                }
                            } else {
                                for mint_url in all_mints.iter() {
                                    {
                                        let is_selected = selected_mints.read().contains(mint_url);
                                        let mint_url_clone = mint_url.clone();
                                        rsx! {
                                            button {
                                                // Use stable key from mint URL (Dioxus pattern)
                                                key: "mint-{mint_url}",
                                                class: if is_selected {
                                                    "w-full px-4 py-3 text-left rounded-lg border-2 border-blue-500 bg-blue-50 dark:bg-blue-900/20 transition"
                                                } else {
                                                    "w-full px-4 py-3 text-left rounded-lg border border-border hover:border-blue-300 bg-background transition"
                                                },
                                                onclick: move |_| toggle_mint(mint_url_clone.clone()),
                                                div {
                                                    class: "flex items-center gap-3",
                                                    div {
                                                        class: if is_selected {
                                                            "w-5 h-5 rounded border-2 border-blue-500 bg-blue-500 flex items-center justify-center text-white text-xs"
                                                        } else {
                                                            "w-5 h-5 rounded border-2 border-gray-300 dark:border-gray-600"
                                                        },
                                                        if is_selected { "v" }
                                                    }
                                                    span {
                                                        class: "font-mono text-sm truncate",
                                                        {shorten_url(mint_url, 40)}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Relay configuration
                    div {
                        label {
                            class: "block text-sm font-semibold mb-2",
                            "Delivery Relays"
                        }
                        p {
                            class: "text-xs text-muted-foreground mb-3",
                            "Relays where senders should publish nutzaps (one per line)"
                        }
                        textarea {
                            class: "w-full px-4 py-3 bg-background border border-border rounded-lg font-mono text-sm min-h-[100px]",
                            placeholder: "wss://relay.damus.io\nwss://nos.lol",
                            value: relay_input.read().clone(),
                            oninput: move |evt| relay_input.set(evt.value())
                        }
                    }

                    // Auto-redemption toggle
                    div {
                        class: "flex items-center justify-between py-3",
                        div {
                            label {
                                class: "text-sm font-semibold",
                                "Auto-redeem nutzaps"
                            }
                            p {
                                class: "text-xs text-muted-foreground",
                                "Automatically redeem incoming nutzaps to your wallet"
                            }
                        }
                        button {
                            role: "switch",
                            aria_checked: if *auto_redeem_enabled.read() { "true" } else { "false" },
                            aria_label: "Toggle automatic nutzap redemption",
                            class: if *auto_redeem_enabled.read() {
                                "w-12 h-6 rounded-full bg-blue-500 relative transition-colors"
                            } else {
                                "w-12 h-6 rounded-full bg-gray-300 dark:bg-gray-600 relative transition-colors"
                            },
                            onclick: move |_| {
                                let current = *auto_redeem_enabled.read();
                                auto_redeem_enabled.set(!current);
                            },
                            div {
                                class: if *auto_redeem_enabled.read() {
                                    "w-5 h-5 rounded-full bg-white absolute top-0.5 right-0.5 transition-all"
                                } else {
                                    "w-5 h-5 rounded-full bg-white absolute top-0.5 left-0.5 transition-all"
                                }
                            }
                        }
                    }

                    // Error message
                    if let Some(msg) = error_message.read().as_ref() {
                        div {
                            class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            p {
                                class: "text-sm text-red-800 dark:text-red-200",
                                "{msg}"
                            }
                        }
                    }

                    // Success message
                    if let Some(msg) = success_message.read().as_ref() {
                        div {
                            class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            p {
                                class: "text-sm text-green-800 dark:text-green-200",
                                "{msg}"
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "px-6 py-4 border-t border-border flex gap-3 sticky bottom-0 bg-card",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    {
                        let is_disabled = *is_publishing.read() || all_mints.is_empty();
                        rsx! {
                            button {
                                class: if is_disabled {
                                    "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                } else {
                                    "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                                },
                                disabled: is_disabled,
                                onclick: on_publish,
                                if *is_publishing.read() {
                                    "Publishing..."
                                } else if nutzap_enabled {
                                    "Update Settings"
                                } else {
                                    "Enable Nutzaps"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
