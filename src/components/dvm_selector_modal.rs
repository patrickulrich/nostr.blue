//! DVM Selector Modal Component
//!
//! Shared modal component for selecting a DVM (Data Vending Machine) provider
//! for content discovery. Used by both the DVM page and Explore page.

use dioxus::prelude::*;
use nostr_sdk::PublicKey;
use crate::stores::dvm_store::{DVM_PROVIDERS, DVM_PROVIDERS_LOADING, SELECTED_DVM_PROVIDER};

/// Modal for selecting DVM provider
#[component]
pub fn DvmSelectorModal(
    on_close: EventHandler<()>,
    on_select: EventHandler<Option<PublicKey>>,
) -> Element {
    let providers = DVM_PROVIDERS.read().clone();
    let loading = *DVM_PROVIDERS_LOADING.read();
    let selected = SELECTED_DVM_PROVIDER.read().clone();

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center p-4 z-50",
            onclick: move |_| on_close.call(()),
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    on_close.call(());
                }
            },

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg shadow-xl max-w-md w-full max-h-[80vh] overflow-hidden",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-4 py-3 border-b border-border flex items-center justify-between",
                    h3 {
                        class: "text-lg font-semibold",
                        "Select DVM Provider"
                    }
                    button {
                        class: "p-1 hover:bg-accent rounded",
                        onclick: move |_| on_close.call(()),
                        "aria-label": "Close dialog",
                        "\u{2715}"
                    }
                }

                // Content
                div {
                    class: "overflow-y-auto max-h-[60vh]",

                    // Default option
                    button {
                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between border-b border-border",
                        onclick: move |_| on_select.call(None),
                        div {
                            div {
                                class: "font-medium",
                                "Default (Snort's DVM)"
                            }
                            div {
                                class: "text-xs text-muted-foreground",
                                "Trending content discovery"
                            }
                        }
                        if selected.is_none() {
                            span {
                                class: "text-green-500",
                                "\u{2713}"
                            }
                        }
                    }

                    // Loading state
                    if loading {
                        div {
                            class: "px-4 py-6 text-center text-muted-foreground",
                            div {
                                class: "flex items-center justify-center gap-2",
                                span {
                                    class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin"
                                }
                                "Loading DVMs..."
                            }
                        }
                    } else if providers.is_empty() {
                        div {
                            class: "px-4 py-6 text-center text-muted-foreground text-sm",
                            "No additional content discovery DVMs found"
                        }
                    } else {
                        // Provider list
                        for provider in providers.iter() {
                            {
                                let is_selected = selected.as_ref() == Some(&provider.pubkey);
                                let provider_pubkey = provider.pubkey;
                                rsx! {
                                    button {
                                        key: "{provider.pubkey.to_hex()}",
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center gap-3 border-b border-border",
                                        onclick: move |_| on_select.call(Some(provider_pubkey)),

                                        // Avatar
                                        div {
                                            class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center overflow-hidden flex-shrink-0",
                                            if let Some(picture) = &provider.picture {
                                                img {
                                                    src: "{picture}",
                                                    class: "w-full h-full object-cover",
                                                    alt: "{provider.name}"
                                                }
                                            } else {
                                                span { class: "text-lg", "\u{26A1}" }
                                            }
                                        }

                                        // Info
                                        div {
                                            class: "flex-1 min-w-0",
                                            div {
                                                class: "font-medium truncate",
                                                "{provider.name}"
                                            }
                                            if let Some(about) = &provider.about {
                                                div {
                                                    class: "text-xs text-muted-foreground line-clamp-1",
                                                    "{about}"
                                                }
                                            }
                                        }

                                        // Selected indicator
                                        if is_selected {
                                            span {
                                                class: "text-green-500 flex-shrink-0",
                                                "\u{2713}"
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
                    class: "px-4 py-3 border-t border-border bg-muted/30",
                    p {
                        class: "text-xs text-muted-foreground text-center",
                        "DVMs provide AI-powered content discovery on Nostr"
                    }
                }
            }
        }
    }
}
