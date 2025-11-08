use dioxus::prelude::*;
use crate::stores::cashu_wallet;

#[component]
pub fn CashuSendModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut amount = use_signal(|| String::new());
    let mints = cashu_wallet::get_mints();
    let mut selected_mint = use_signal(|| mints.first().cloned().unwrap_or_default());

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 {
                        class: "text-xl font-bold",
                        "Send Tokens"
                    }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Amount input
                    div {
                        label {
                            class: "block text-sm font-semibold mb-2",
                            "Amount (sats)"
                        }
                        input {
                            class: "w-full px-4 py-3 bg-background border border-border rounded-lg text-lg",
                            r#type: "number",
                            placeholder: "0",
                            min: "1",
                            value: amount.read().clone(),
                            oninput: move |evt| amount.set(evt.value())
                        }
                    }

                    // Mint selection
                    if !mints.is_empty() {
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "Select Mint"
                            }
                            select {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg",
                                value: selected_mint.read().clone(),
                                onchange: move |evt| selected_mint.set(evt.value()),
                                for mint_url in mints.iter() {
                                    option {
                                        value: mint_url.clone(),
                                        "{shorten_url(mint_url)}"
                                    }
                                }
                            }
                        }
                    }

                    // Phase 2 notice
                    div {
                        class: "bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded-lg p-4",
                        div {
                            class: "flex items-start gap-3",
                            div {
                                class: "text-2xl",
                                "🚧"
                            }
                            div {
                                h4 {
                                    class: "font-semibold text-sm mb-1",
                                    "Phase 2 Feature"
                                }
                                p {
                                    class: "text-sm text-muted-foreground",
                                    "Sending tokens requires Cashu mint API integration, which will be added in Phase 2. For now, you can view your tokens and balance."
                                }
                            }
                        }
                    }

                    // Preview (Phase 2)
                    div {
                        class: "bg-accent/50 rounded-lg p-4",
                        h4 {
                            class: "text-sm font-semibold mb-2",
                            "Preview"
                        }
                        div {
                            class: "space-y-2 text-sm",
                            div {
                                class: "flex justify-between",
                                span { class: "text-muted-foreground", "Amount:" }
                                span { class: "font-mono", "{amount.read()} sats" }
                            }
                            div {
                                class: "flex justify-between",
                                span { class: "text-muted-foreground", "Mint:" }
                                span { class: "font-mono text-xs truncate max-w-[200px]", {selected_mint.read().as_str()} }
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "px-6 py-4 border-t border-border flex gap-3",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed",
                        disabled: true,
                        "Send (Phase 2)"
                    }
                }
            }
        }
    }
}

/// Shorten URL for display
fn shorten_url(url: &str) -> String {
    let url = url.trim_start_matches("https://").trim_start_matches("http://");
    if url.len() > 35 {
        format!("{}...", &url[..32])
    } else {
        url.to_string()
    }
}
