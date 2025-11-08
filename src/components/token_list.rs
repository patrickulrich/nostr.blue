use dioxus::prelude::*;
use crate::stores::cashu_wallet::{self, TokenData};
use std::collections::HashMap;

#[component]
pub fn TokenList() -> Element {
    let tokens = cashu_wallet::WALLET_TOKENS.read();
    let mut expanded_mints = use_signal(|| std::collections::HashSet::<String>::new());

    if tokens.is_empty() {
        return rsx! {
            div {
                class: "bg-card border border-border rounded-lg p-8 text-center",
                div {
                    class: "text-4xl mb-3",
                    "🪙"
                }
                p {
                    class: "text-muted-foreground",
                    "No tokens yet"
                }
                p {
                    class: "text-sm text-muted-foreground mt-1",
                    "Receive tokens to get started"
                }
            }
        };
    }

    // Group tokens by mint
    let mut tokens_by_mint: HashMap<String, Vec<&TokenData>> = HashMap::new();
    for token in tokens.iter() {
        tokens_by_mint.entry(token.mint.clone())
            .or_insert_with(Vec::new)
            .push(token);
    }

    // Sort mints by total balance (descending)
    let mut sorted_mints: Vec<_> = tokens_by_mint.iter().collect();
    sorted_mints.sort_by(|a, b| {
        let balance_a: u64 = a.1.iter().flat_map(|t| &t.proofs).map(|p| p.amount).sum();
        let balance_b: u64 = b.1.iter().flat_map(|t| &t.proofs).map(|p| p.amount).sum();
        balance_b.cmp(&balance_a)
    });

    rsx! {
        div {
            class: "flex flex-col gap-3",

            for (mint_url, tokens_for_mint) in sorted_mints {
                {
                    let mint_url = mint_url.clone();
                    let is_expanded = expanded_mints.read().contains(&mint_url);

                    // Calculate total for this mint
                    let total_balance: u64 = tokens_for_mint.iter()
                        .flat_map(|t| &t.proofs)
                        .map(|p| p.amount)
                        .sum();

                    let proof_count: usize = tokens_for_mint.iter()
                        .map(|t| t.proofs.len())
                        .sum();

                    rsx! {
                        div {
                            key: "{mint_url}",
                            class: "bg-card border border-border rounded-lg overflow-hidden",

                            // Mint header (clickable to expand/collapse)
                            button {
                                class: "w-full px-4 py-3 flex items-center justify-between hover:bg-accent transition text-left",
                                onclick: {
                                    let mint_url_clone = mint_url.clone();
                                    move |_| {
                                        let mut expanded = expanded_mints.write();
                                        if expanded.contains(&mint_url_clone) {
                                            expanded.remove(&mint_url_clone);
                                        } else {
                                            expanded.insert(mint_url_clone.clone());
                                        }
                                    }
                                },

                                div {
                                    class: "flex-1 min-w-0",
                                    div {
                                        class: "font-semibold text-sm truncate",
                                        title: "{mint_url}",
                                        "{shorten_mint_url(&mint_url)}"
                                    }
                                    div {
                                        class: "text-xs text-muted-foreground mt-1",
                                        "{proof_count} proofs"
                                    }
                                }

                                div {
                                    class: "flex items-center gap-3",
                                    div {
                                        class: "text-right",
                                        div {
                                            class: "font-bold",
                                            "{format_sats(total_balance)} sats"
                                        }
                                    }
                                    div {
                                        class: "text-muted-foreground",
                                        if is_expanded { "▼" } else { "▶" }
                                    }
                                }
                            }

                            // Token details (expanded)
                            if is_expanded {
                                div {
                                    class: "border-t border-border",
                                    for (i, token) in tokens_for_mint.iter().enumerate() {
                                        div {
                                            key: "{token.event_id}",
                                            class: if i > 0 { "border-t border-border/50 px-4 py-3" } else { "px-4 py-3" },

                                            div {
                                                class: "flex items-start justify-between mb-2",
                                                div {
                                                    class: "text-xs text-muted-foreground",
                                                    "Token Event"
                                                }
                                                div {
                                                    class: "text-xs font-mono text-muted-foreground",
                                                    "{&token.event_id[..12]}..."
                                                }
                                            }

                                            // List of proofs
                                            div {
                                                class: "space-y-2",
                                                for (proof_idx, proof) in token.proofs.iter().enumerate() {
                                                    div {
                                                        key: "{proof_idx}",
                                                        class: "bg-background/50 rounded p-2 text-xs",
                                                        div {
                                                            class: "flex justify-between items-center",
                                                            div {
                                                                class: "font-mono text-muted-foreground",
                                                                "Proof #{proof_idx + 1}"
                                                            }
                                                            div {
                                                                class: "font-bold",
                                                                "{proof.amount} sats"
                                                            }
                                                        }
                                                        div {
                                                            class: "mt-1 text-muted-foreground truncate",
                                                            "ID: {&proof.id}"
                                                        }
                                                    }
                                                }
                                            }

                                            // Token metadata
                                            div {
                                                class: "mt-3 text-xs text-muted-foreground",
                                                "Total: {token.proofs.iter().map(|p| p.amount).sum::<u64>()} sats • {token.proofs.len()} proofs"
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
    }
}

/// Format satoshi amount with thousands separator
fn format_sats(sats: u64) -> String {
    let s = sats.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

/// Shorten mint URL for display
fn shorten_mint_url(url: &str) -> String {
    let url = url.trim_start_matches("https://").trim_start_matches("http://");
    if url.len() > 40 {
        format!("{}...", &url[..37])
    } else {
        url.to_string()
    }
}
