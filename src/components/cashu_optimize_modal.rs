use dioxus::prelude::*;
use crate::stores::cashu_wallet::{self, ConsolidationResult};

#[component]
pub fn CashuOptimizeModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut is_optimizing = use_signal(|| false);
    let mut results = use_signal(|| Vec::<(String, ConsolidationResult)>::new());
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut is_complete = use_signal(|| false);

    let mints = cashu_wallet::get_mints();

    // Get proof counts per mint
    let mint_proof_counts: Vec<(String, usize)> = mints.iter()
        .map(|m| (m.clone(), cashu_wallet::get_mint_proof_count(m)))
        .collect();

    let total_proofs: usize = mint_proof_counts.iter().map(|(_, c)| *c).sum();

    let on_optimize = move |_| {
        if *is_optimizing.read() {
            return;
        }

        is_optimizing.set(true);
        error_message.set(None);
        results.set(Vec::new());

        spawn(async move {
            match cashu_wallet::consolidate_all_mints().await {
                Ok(consolidation_results) => {
                    results.set(consolidation_results);
                    is_complete.set(true);
                    is_optimizing.set(false);
                }
                Err(e) => {
                    error_message.set(Some(format!("Optimization failed: {}", e)));
                    is_optimizing.set(false);
                }
            }
        });
    };

    // Calculate summary stats from results
    let total_before: usize = results.read().iter().map(|(_, r)| r.proofs_before).sum();
    let total_after: usize = results.read().iter().map(|(_, r)| r.proofs_after).sum();
    let total_fees: u64 = results.read().iter().map(|(_, r)| r.fee_paid).sum();

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| {
                if !*is_optimizing.read() {
                    on_close.call(());
                }
            },

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 {
                        class: "text-xl font-bold flex items-center gap-2",
                        span { "✨" }
                        "Optimize Wallet"
                    }
                    if !*is_optimizing.read() {
                        button {
                            class: "text-2xl text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| on_close.call(()),
                            "×"
                        }
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Error message
                    if let Some(msg) = error_message.read().as_ref() {
                        div {
                            class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div { class: "text-2xl", "!" }
                                p {
                                    class: "text-sm text-red-800 dark:text-red-200",
                                    "{msg}"
                                }
                            }
                        }
                    }

                    // Success message
                    if *is_complete.read() && !results.read().is_empty() {
                        div {
                            class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div { class: "text-2xl", "✨" }
                                div {
                                    p {
                                        class: "text-sm font-semibold text-green-800 dark:text-green-200",
                                        "Optimization complete!"
                                    }
                                    p {
                                        class: "text-xs text-green-700 dark:text-green-300 mt-1",
                                        "Consolidated {total_before} proofs to {total_after} proofs"
                                    }
                                    if total_fees > 0 {
                                        p {
                                            class: "text-xs text-green-700 dark:text-green-300 mt-1",
                                            "Fee paid: {total_fees} sats"
                                        }
                                    }
                                }
                            }
                        }

                        // Results per mint
                        div {
                            class: "space-y-2",
                            for (mint, result) in results.read().iter() {
                                div {
                                    class: "flex flex-col py-2 border-b border-border last:border-0",
                                    div {
                                        class: "flex justify-between items-center text-sm",
                                        span { class: "text-muted-foreground truncate max-w-[200px]", "{shorten_url(mint)}" }
                                        span { class: "font-mono", "{result.proofs_before} → {result.proofs_after}" }
                                    }
                                    if result.fee_paid > 0 {
                                        div {
                                            class: "text-xs text-muted-foreground mt-1",
                                            "Fee: {result.fee_paid} sats"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Pre-optimization info
                    if !*is_complete.read() {
                        div {
                            class: "bg-accent/50 rounded-lg p-4 space-y-2",
                            h4 {
                                class: "text-sm font-semibold mb-3",
                                "Current Proof Distribution"
                            }
                            div {
                                class: "space-y-2 text-sm",
                                for (mint, count) in mint_proof_counts.iter() {
                                    div {
                                        class: "flex justify-between",
                                        span { class: "text-muted-foreground truncate max-w-[200px]", "{shorten_url(mint)}" }
                                        span {
                                            class: if *count > 8 { "font-mono text-orange-500" } else { "font-mono text-green-500" },
                                            "{count} proofs"
                                        }
                                    }
                                }
                                div {
                                    class: "flex justify-between border-t border-border pt-2 mt-2",
                                    span { class: "font-semibold", "Total" }
                                    span { class: "font-mono font-semibold", "{total_proofs} proofs" }
                                }
                            }
                        }

                        // Explanation
                        div {
                            class: "text-sm text-muted-foreground",
                            p {
                                "Optimization consolidates many small proofs into fewer larger ones, "
                                "making your wallet more efficient for future transactions."
                            }
                        }

                        // Progress indicator
                        if *is_optimizing.read() {
                            div {
                                class: "flex items-center justify-center gap-2 py-4",
                                div { class: "animate-spin text-xl", "!" }
                                span { class: "text-sm text-muted-foreground", "Optimizing..." }
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "px-6 py-4 border-t border-border flex gap-3",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        disabled: *is_optimizing.read(),
                        onclick: move |_| on_close.call(()),
                        if *is_complete.read() { "Done" } else { "Cancel" }
                    }
                    if !*is_complete.read() {
                        button {
                            class: if *is_optimizing.read() || total_proofs <= 8 {
                                "flex-1 px-4 py-3 bg-purple-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                            } else {
                                "flex-1 px-4 py-3 bg-purple-500 hover:bg-purple-600 text-white font-semibold rounded-lg transition"
                            },
                            disabled: *is_optimizing.read() || total_proofs <= 8,
                            onclick: on_optimize,
                            if *is_optimizing.read() {
                                "Optimizing..."
                            } else if total_proofs <= 8 {
                                "Already Optimal"
                            } else {
                                "Optimize Now"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Shorten URL for display
fn shorten_url(url: &str) -> String {
    let url = url.trim_start_matches("https://").trim_start_matches("http://");
    if url.len() > 30 {
        format!("{}...", &url[..27])
    } else {
        url.to_string()
    }
}
