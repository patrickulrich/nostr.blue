use dioxus::prelude::*;
use crate::stores::cashu_wallet::{self, TokenData, MintInfoDisplay, WalletTokensStoreStoreExt};
use crate::utils::format_sats_with_separator;
use std::collections::HashMap;
use std::rc::Rc;

#[component]
fn MintRow(mint_url: String, tokens_for_mint: Rc<Vec<TokenData>>, is_expanded: bool, on_toggle: EventHandler<()>) -> Element {
    let mut is_cleaning = use_signal(|| false);
    let mut cleanup_message = use_signal(|| Option::<String>::None);
    let mut is_removing = use_signal(|| false);
    let mut show_confirm = use_signal(|| false);
    let mut remove_error = use_signal(|| Option::<String>::None);
    let mut show_mint_info = use_signal(|| false);
    let mut mint_info = use_signal(|| Option::<MintInfoDisplay>::None);
    let mut mint_info_loading = use_signal(|| false);
    let mut mint_info_error = use_signal(|| Option::<String>::None);

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
                onclick: move |_| on_toggle.call(()),

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
                            "{format_sats_with_separator(total_balance)} sats"
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

                    // Show empty state for mints with no tokens
                    if tokens_for_mint.is_empty() {
                        div {
                            class: "px-4 py-6 text-center",
                            p {
                                class: "text-muted-foreground text-sm",
                                "No tokens for this mint"
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-1",
                                "You can remove this mint or deposit tokens to it"
                            }
                        }
                    }

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

                    // Mint info display section
                    if *show_mint_info.read() {
                        div {
                            class: "px-4 py-3 border-t border-border bg-blue-50 dark:bg-blue-950/20",

                            // Loading state
                            if *mint_info_loading.read() {
                                div {
                                    class: "flex items-center justify-center gap-2 py-2",
                                    div { class: "animate-spin", "!" }
                                    span { class: "text-sm text-muted-foreground", "Loading mint info..." }
                                }
                            }

                            // Error state
                            if let Some(err) = mint_info_error.read().as_ref() {
                                div {
                                    class: "text-sm text-red-600 dark:text-red-400 text-center py-2",
                                    "Error: {err}"
                                }
                            }

                            // Info display
                            if let Some(info) = mint_info.read().as_ref() {
                                div {
                                    class: "space-y-2",

                                    // Header with close button
                                    div {
                                        class: "flex items-center justify-between",
                                        h4 {
                                            class: "text-sm font-semibold text-blue-800 dark:text-blue-200",
                                            "Mint Information"
                                        }
                                        button {
                                            class: "text-muted-foreground hover:text-foreground text-lg",
                                            onclick: move |_| show_mint_info.set(false),
                                            "×"
                                        }
                                    }

                                    // Name and version
                                    div {
                                        class: "flex justify-between text-sm",
                                        if let Some(name) = &info.name {
                                            span { class: "font-medium", "{name}" }
                                        } else {
                                            span { class: "text-muted-foreground", "Unknown Mint" }
                                        }
                                        if let Some(version) = &info.version {
                                            span { class: "text-xs text-muted-foreground font-mono", "v{version}" }
                                        }
                                    }

                                    // Description
                                    if let Some(desc) = &info.description {
                                        div {
                                            class: "text-sm text-muted-foreground",
                                            "{desc}"
                                        }
                                    }

                                    // Long description (if different from short)
                                    if let Some(desc_long) = &info.description_long {
                                        if info.description.as_ref() != Some(desc_long) {
                                            div {
                                                class: "text-xs text-muted-foreground mt-1 bg-background/30 rounded p-2",
                                                "{desc_long}"
                                            }
                                        }
                                    }

                                    // Supported NUTs
                                    if !info.supported_nuts.is_empty() {
                                        div {
                                            class: "text-sm",
                                            span { class: "text-muted-foreground", "NUTs: " }
                                            span { class: "font-mono text-xs",
                                                {info.supported_nuts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")}
                                            }
                                        }
                                    }

                                    // MOTD
                                    if let Some(motd) = &info.motd {
                                        div {
                                            class: "text-xs italic bg-background/50 rounded p-2 mt-2",
                                            "{motd}"
                                        }
                                    }

                                    // Contact
                                    if !info.contact.is_empty() {
                                        div {
                                            class: "text-xs text-muted-foreground mt-2",
                                            for (method, contact_info) in info.contact.iter() {
                                                span { "{method}: {contact_info} " }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Mint actions
                    div {
                        class: "px-4 py-3 border-t border-border bg-background/30 flex gap-2",

                        // Info button
                        button {
                            class: if *mint_info_loading.read() {
                                "px-3 py-2 text-sm bg-blue-500 text-white rounded-lg opacity-50 cursor-not-allowed"
                            } else if *show_mint_info.read() {
                                "px-3 py-2 text-sm bg-blue-600 text-white rounded-lg"
                            } else {
                                "px-3 py-2 text-sm bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition"
                            },
                            disabled: *mint_info_loading.read(),
                            onclick: {
                                let mint_url_clone = mint_url.clone();
                                move |_| {
                                    if *show_mint_info.read() {
                                        show_mint_info.set(false);
                                    } else {
                                        show_mint_info.set(true);
                                        // Only fetch if not already cached
                                        if mint_info.read().is_none() {
                                            let mint_url = mint_url_clone.clone();
                                            mint_info_loading.set(true);
                                            mint_info_error.set(None);
                                            spawn(async move {
                                                match cashu_wallet::get_mint_info(&mint_url).await {
                                                    Ok(info) => {
                                                        mint_info.set(Some(info));
                                                        mint_info_loading.set(false);
                                                    }
                                                    Err(e) => {
                                                        mint_info_error.set(Some(e));
                                                        mint_info_loading.set(false);
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            },
                            if *mint_info_loading.read() {
                                "..."
                            } else {
                                "i"
                            }
                        }

                        // Cleanup button
                        div {
                            class: "flex-1",
                            button {
                                class: if *is_cleaning.read() {
                                    "w-full px-3 py-2 text-sm bg-yellow-500 text-white rounded-lg opacity-50 cursor-not-allowed"
                                } else {
                                    "w-full px-3 py-2 text-sm bg-yellow-500 hover:bg-yellow-600 text-white rounded-lg transition"
                                },
                                disabled: *is_cleaning.read(),
                                onclick: {
                                    let mint_url_clone = mint_url.clone();
                                    move |_| {
                                        let mint_url = mint_url_clone.clone();
                                        is_cleaning.set(true);
                                        cleanup_message.set(None);
                                        spawn(async move {
                                            match cashu_wallet::cleanup_spent_proofs(mint_url).await {
                                                Ok((count, amount)) if count > 0 => {
                                                    cleanup_message.set(Some(format!("Cleaned {} proofs ({} sats)", count, amount)));
                                                    is_cleaning.set(false);
                                                }
                                                Ok(_) => {
                                                    cleanup_message.set(Some("No spent proofs found".to_string()));
                                                    is_cleaning.set(false);
                                                }
                                                Err(e) => {
                                                    cleanup_message.set(Some(format!("Error: {}", e)));
                                                    is_cleaning.set(false);
                                                }
                                            }
                                        });
                                    }
                                },
                                if *is_cleaning.read() {
                                    "Cleaning..."
                                } else {
                                    "Cleanup"
                                }
                            }
                            if let Some(msg) = cleanup_message.read().as_ref() {
                                div {
                                    class: "mt-1 text-xs text-center text-muted-foreground",
                                    "{msg}"
                                }
                            }
                        }

                        // Remove mint button
                        div {
                            class: "flex-1",
                            if *show_confirm.read() {
                                div {
                                    class: "flex flex-col gap-2",
                                    p {
                                        class: "text-xs text-destructive text-center",
                                        "Remove all tokens from this mint?"
                                    }
                                    // Display error message if removal failed
                                    if let Some(error) = remove_error.read().as_ref() {
                                        div {
                                            class: "text-xs text-destructive text-center bg-destructive/10 rounded p-2",
                                            "Failed: {error}"
                                        }
                                    }
                                    div {
                                        class: "flex gap-2",
                                        button {
                                            class: "flex-1 px-3 py-2 text-sm bg-destructive hover:bg-destructive/80 text-white rounded-lg transition",
                                            onclick: {
                                                let mint_url_clone = mint_url.clone();
                                                move |_| {
                                                    let mint_url = mint_url_clone.clone();
                                                    is_removing.set(true);
                                                    remove_error.set(None);
                                                    spawn(async move {
                                                        match cashu_wallet::remove_mint(mint_url).await {
                                                            Ok((count, amount)) => {
                                                                log::info!("Removed mint: {} events, {} sats", count, amount);
                                                                is_removing.set(false);
                                                                show_confirm.set(false);
                                                                remove_error.set(None);
                                                            }
                                                            Err(e) => {
                                                                log::error!("Failed to remove mint: {}", e);
                                                                is_removing.set(false);
                                                                // Keep dialog open and show error
                                                                remove_error.set(Some(e.to_string()));
                                                            }
                                                        }
                                                    });
                                                }
                                            },
                                            disabled: *is_removing.read(),
                                            if *is_removing.read() {
                                                "Removing..."
                                            } else {
                                                "Yes, Remove"
                                            }
                                        }
                                        button {
                                            class: "flex-1 px-3 py-2 text-sm bg-accent hover:bg-accent/80 rounded-lg transition",
                                            onclick: move |_| {
                                                show_confirm.set(false);
                                                remove_error.set(None);
                                            },
                                            "Cancel"
                                        }
                                    }
                                }
                            } else {
                                button {
                                    class: "w-full px-3 py-2 text-sm bg-destructive hover:bg-destructive/80 text-white rounded-lg transition",
                                    onclick: move |_| {
                                        show_confirm.set(true);
                                        remove_error.set(None);
                                    },
                                    "🗑️ Remove Mint"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TokenList() -> Element {
    let tokens = cashu_wallet::WALLET_TOKENS.read();
    let mut expanded_mints = use_signal(|| std::collections::HashSet::<String>::new());

    // Check if there are any tokens OR any mints (mints without tokens should still be shown)
    let has_tokens = !tokens.data().read().is_empty();
    let has_mints = !cashu_wallet::get_mints().is_empty();

    if !has_tokens && !has_mints {
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

    // Memoize the grouping and sorting to avoid recomputation on every render
    let grouped_mints = use_memo(move || {
        let tokens = cashu_wallet::WALLET_TOKENS.read();
        let data = tokens.data();
        let tokens_data = data.read();

        // Group tokens by mint
        let mut tokens_by_mint: HashMap<String, Vec<TokenData>> = HashMap::new();
        for token in tokens_data.iter() {
            tokens_by_mint.entry(token.mint.clone())
                .or_insert_with(Vec::new)
                .push(token.clone());
        }

        // Also include mints from WALLET_STATE that may have no tokens
        // This ensures users can manage (remove) mints even when they have no balance
        let wallet_mints = cashu_wallet::get_mints();
        for mint_url in wallet_mints {
            tokens_by_mint.entry(mint_url).or_insert_with(Vec::new);
        }

        // Sort mints by total balance (descending) and wrap in Rc
        let mut sorted_mints: Vec<(String, Rc<Vec<TokenData>>)> = tokens_by_mint
            .into_iter()
            .map(|(mint_url, tokens_vec)| (mint_url, Rc::new(tokens_vec)))
            .collect();

        sorted_mints.sort_by(|a, b| {
            let balance_a: u64 = a.1.iter().flat_map(|t| &t.proofs).map(|p| p.amount).sum();
            let balance_b: u64 = b.1.iter().flat_map(|t| &t.proofs).map(|p| p.amount).sum();
            balance_b.cmp(&balance_a)
        });

        sorted_mints
    });

    rsx! {
        div {
            class: "flex flex-col gap-3",

            for (mint_url, tokens_for_mint) in grouped_mints.read().iter() {
                {
                    let mint_url = mint_url.clone();
                    let tokens_rc = tokens_for_mint.clone();
                    let is_expanded = expanded_mints.read().contains(&mint_url);

                    rsx! {
                        MintRow {
                            key: "{mint_url}",
                            mint_url: mint_url.clone(),
                            tokens_for_mint: tokens_rc,
                            is_expanded: is_expanded,
                            on_toggle: move |_| {
                                let mut expanded = expanded_mints.write();
                                if expanded.contains(&mint_url) {
                                    expanded.remove(&mint_url);
                                } else {
                                    expanded.insert(mint_url.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
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
