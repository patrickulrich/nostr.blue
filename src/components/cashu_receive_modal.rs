use dioxus::prelude::*;

#[component]
pub fn CashuReceiveModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut token_string = use_signal(|| String::new());

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
                        "Receive Tokens"
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

                    // Token input
                    div {
                        label {
                            class: "block text-sm font-semibold mb-2",
                            "Paste Token String"
                        }
                        textarea {
                            class: "w-full px-4 py-3 bg-background border border-border rounded-lg font-mono text-sm min-h-[120px]",
                            placeholder: "cashuA...",
                            value: token_string.read().clone(),
                            oninput: move |evt| token_string.set(evt.value())
                        }
                        p {
                            class: "text-xs text-muted-foreground mt-2",
                            "Paste a Cashu token string to receive ecash"
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
                                    "Receiving tokens requires Cashu mint API integration, which will be added in Phase 2. This will enable redeeming tokens and updating your balance."
                                }
                            }
                        }
                    }

                    // Info box
                    div {
                        class: "bg-accent/50 rounded-lg p-4",
                        h4 {
                            class: "text-sm font-semibold mb-2",
                            "How it works:"
                        }
                        ul {
                            class: "text-sm text-muted-foreground space-y-1",
                            li { "1. Paste the token string from sender" }
                            li { "2. Token is validated and decoded" }
                            li { "3. Proofs are redeemed at the mint" }
                            li { "4. New token event is created (kind 7375)" }
                            li { "5. Balance is updated" }
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
                        "Receive (Phase 2)"
                    }
                }
            }
        }
    }
}
