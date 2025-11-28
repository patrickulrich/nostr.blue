use dioxus::prelude::*;
use crate::stores::cashu_wallet::{self, DiscoveredMint};

#[component]
pub fn CashuMintDiscoveryModal(
    on_close: EventHandler<()>,
    on_mint_selected: EventHandler<String>,
) -> Element {
    let mut is_loading = use_signal(|| true);
    let mut discovered_mints = use_signal(|| Vec::<DiscoveredMint>::new());
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut selected_mint = use_signal(|| Option::<String>::None);
    let mut is_adding = use_signal(|| false);

    // Fetch mints on mount
    use_effect(move || {
        spawn(async move {
            match cashu_wallet::discover_mints().await {
                Ok(mints) => {
                    discovered_mints.set(mints);
                    is_loading.set(false);
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to discover mints: {}", e)));
                    is_loading.set(false);
                }
            }
        });
    });

    let mut on_add_mint = move |url: String| {
        is_adding.set(true);
        error_message.set(None);

        spawn(async move {
            match cashu_wallet::add_mint(url.clone()).await {
                Ok(_) => {
                    is_adding.set(false);
                    on_mint_selected.call(url);
                    on_close.call(());
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to add mint: {}", e)));
                    is_adding.set(false);
                }
            }
        });
    };

    // Get existing mints to filter out already added ones
    let existing_mints = cashu_wallet::get_mints();

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| {
                if !*is_loading.read() && !*is_adding.read() {
                    on_close.call(());
                }
            },

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg max-w-lg w-full shadow-xl max-h-[80vh] flex flex-col",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between shrink-0",
                    h3 {
                        class: "text-xl font-bold flex items-center gap-2",
                        span { class: "text-2xl", "!" }
                        "Discover Mints"
                    }
                    if !*is_loading.read() && !*is_adding.read() {
                        button {
                            class: "text-2xl text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| on_close.call(()),
                            "x"
                        }
                    }
                }

                // Body
                div {
                    class: "p-6 overflow-y-auto flex-1",

                    // Error message
                    if let Some(msg) = error_message.read().as_ref() {
                        div {
                            class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4 mb-4",
                            p {
                                class: "text-sm text-red-800 dark:text-red-200",
                                "{msg}"
                            }
                        }
                    }

                    // Loading state
                    if *is_loading.read() {
                        div {
                            class: "flex flex-col items-center justify-center py-12",
                            div { class: "animate-spin text-3xl mb-4", "!" }
                            p { class: "text-muted-foreground", "Discovering mints via NIP-87..." }
                        }
                    } else if discovered_mints.read().is_empty() {
                        // No mints found
                        div {
                            class: "text-center py-12",
                            div { class: "text-4xl mb-4", "!" }
                            h4 { class: "text-lg font-semibold mb-2", "No mints discovered" }
                            p {
                                class: "text-muted-foreground text-sm",
                                "No Cashu mint announcements found on the network. Try adding a mint manually."
                            }
                        }
                    } else {
                        // Mint list
                        div {
                            class: "space-y-3",

                            p {
                                class: "text-sm text-muted-foreground mb-4",
                                "Mints discovered via NIP-87. Sorted by recommendation count."
                            }

                            for mint in discovered_mints.read().iter() {
                                {
                                    let url = mint.url.clone();
                                    let is_already_added = existing_mints.contains(&url);
                                    let is_selected = selected_mint.read().as_ref() == Some(&url);
                                    let is_mainnet = mint.network.as_ref().map(|n| n == "mainnet").unwrap_or(true);

                                    rsx! {
                                        div {
                                            key: "{url}",
                                            class: if is_selected {
                                                "bg-accent border-2 border-blue-500 rounded-lg p-4 cursor-pointer transition"
                                            } else if is_already_added {
                                                "bg-accent/30 border border-border rounded-lg p-4 opacity-60"
                                            } else {
                                                "bg-accent/50 border border-border rounded-lg p-4 cursor-pointer hover:border-blue-400 transition"
                                            },
                                            onclick: {
                                                let url = url.clone();
                                                move |_| {
                                                    if !is_already_added {
                                                        selected_mint.set(Some(url.clone()));
                                                    }
                                                }
                                            },

                                            // Header row
                                            div {
                                                class: "flex items-start justify-between gap-2",
                                                div {
                                                    class: "flex-1 min-w-0",
                                                    // Name
                                                    h4 {
                                                        class: "font-semibold truncate",
                                                        if let Some(name) = &mint.name {
                                                            "{name}"
                                                        } else {
                                                            // Extract domain from URL
                                                            {
                                                                url::Url::parse(&mint.url)
                                                                    .ok()
                                                                    .and_then(|u| u.host_str().map(|h| h.to_string()))
                                                                    .unwrap_or_else(|| mint.url.clone())
                                                            }
                                                        }
                                                    }
                                                    // URL
                                                    p {
                                                        class: "text-xs text-muted-foreground truncate",
                                                        "{mint.url}"
                                                    }
                                                }

                                                // Badges
                                                div {
                                                    class: "flex items-center gap-2 shrink-0",
                                                    // Network badge
                                                    if !is_mainnet {
                                                        span {
                                                            class: "px-2 py-0.5 text-xs bg-yellow-500/20 text-yellow-600 dark:text-yellow-400 rounded",
                                                            {mint.network.clone().unwrap_or_default()}
                                                        }
                                                    }
                                                    // Already added badge
                                                    if is_already_added {
                                                        span {
                                                            class: "px-2 py-0.5 text-xs bg-green-500/20 text-green-600 dark:text-green-400 rounded",
                                                            "Added"
                                                        }
                                                    }
                                                }
                                            }

                                            // Description
                                            if let Some(desc) = &mint.description {
                                                p {
                                                    class: "text-sm text-muted-foreground mt-2 line-clamp-2",
                                                    "{desc}"
                                                }
                                            }

                                            // Footer row
                                            div {
                                                class: "flex items-center justify-between mt-3 pt-2 border-t border-border/50",
                                                // Recommendations
                                                div {
                                                    class: "flex items-center gap-1 text-sm",
                                                    if mint.recommendation_count > 0 {
                                                        {
                                                            let count = mint.recommendation_count;
                                                            let suffix = if count == 1 { "" } else { "s" };
                                                            rsx! {
                                                                span { class: "text-green-500", "!" }
                                                                span { class: "text-muted-foreground",
                                                                    "{count} recommendation{suffix}"
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        span { class: "text-muted-foreground text-xs", "No recommendations yet" }
                                                    }
                                                }
                                                // NUTs
                                                if let Some(nuts) = &mint.nuts {
                                                    span {
                                                        class: "text-xs text-muted-foreground",
                                                        "NUTs: {nuts}"
                                                    }
                                                }
                                            }

                                            // Announcement metadata (author and mint pubkey)
                                            div {
                                                class: "flex flex-wrap gap-x-3 gap-y-1 mt-2 text-xs text-muted-foreground",
                                                // Author who published the announcement
                                                span {
                                                    title: "{mint.author_pubkey}",
                                                    "Announced by: {shorten_pubkey(&mint.author_pubkey)}"
                                                }
                                                // Mint's own pubkey if available
                                                if let Some(mint_pk) = &mint.mint_pubkey {
                                                    span {
                                                        title: "{mint_pk}",
                                                        "Mint key: {shorten_pubkey(mint_pk)}"
                                                    }
                                                }
                                            }

                                            // Show individual recommendations with comments
                                            if !mint.recommendations.is_empty() {
                                                {
                                                    // Filter to only show recommendations with content
                                                    let recs_with_content: Vec<_> = mint.recommendations.iter()
                                                        .filter(|r| !r.content.trim().is_empty())
                                                        .collect();

                                                    if !recs_with_content.is_empty() {
                                                        rsx! {
                                                            div {
                                                                class: "mt-3 pt-2 border-t border-border/30",
                                                                p {
                                                                    class: "text-xs font-semibold text-muted-foreground mb-2",
                                                                    "Reviews:"
                                                                }
                                                                div {
                                                                    class: "space-y-2",
                                                                    for rec in recs_with_content.iter().take(3) {
                                                                        div {
                                                                            class: "bg-background/50 rounded p-2",
                                                                            p {
                                                                                class: "text-xs text-foreground line-clamp-2",
                                                                                "\"{rec.content}\""
                                                                            }
                                                                            p {
                                                                                class: "text-xs text-muted-foreground mt-1",
                                                                                title: "{rec.recommender}",
                                                                                "— {shorten_pubkey(&rec.recommender)}"
                                                                            }
                                                                        }
                                                                    }
                                                                    if recs_with_content.len() > 3 {
                                                                        p {
                                                                            class: "text-xs text-muted-foreground",
                                                                            "+{recs_with_content.len() - 3} more reviews"
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {}
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

                // Footer
                div {
                    class: "px-6 py-4 border-t border-border flex gap-3 shrink-0",
                    // Cancel button
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        disabled: *is_adding.read(),
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }

                    // Add selected mint button
                    if let Some(url) = selected_mint.read().clone() {
                        button {
                            class: if *is_adding.read() {
                                "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                            } else {
                                "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                            },
                            disabled: *is_adding.read(),
                            onclick: move |_| on_add_mint(url.clone()),
                            if *is_adding.read() { "Adding..." } else { "Add Mint" }
                        }
                    }
                }
            }
        }
    }
}

/// Shorten pubkey for display
fn shorten_pubkey(pubkey: &str) -> String {
    if pubkey.len() > 16 {
        format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-4..])
    } else {
        pubkey.to_string()
    }
}
