//! Modal for accepting Mostro P2P trade terms.
//!
//! Mirrors the structure of `components/cashu/terms_modal.rs` but with
//! Mostro-specific copy. The modal is non-dismissible: the only way forward
//! is to accept (or navigate away from /p2p).

use crate::stores::social::mostro::nip78;
use dioxus::prelude::*;

/// Non-dismissible modal asking the user to accept the Mostro P2P trade
/// disclaimer. On accept, publishes a NIP-78 event and fires `on_accept`.
#[component]
pub fn MostroTermsModal(on_accept: EventHandler<()>) -> Element {
    let mut is_accepting = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);

    let handle_accept = move |_| {
        if *is_accepting.read() {
            return;
        }
        is_accepting.set(true);
        error_message.set(None);
        spawn(async move {
            match nip78::accept_p2p_terms().await {
                Ok(()) => on_accept.call(()),
                Err(e) => {
                    error_message.set(Some(e));
                    is_accepting.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black/70 z-50 flex items-center justify-center p-4",
            div {
                class: "bg-card border border-border rounded-xl max-w-md w-full p-6 shadow-xl",
                onclick: move |e| e.stop_propagation(),
                div { class: "text-center text-5xl mb-4", "🤝" }
                h2 {
                    class: "text-xl font-bold text-center mb-4 text-foreground",
                    "P2P Trading — Terms of Use"
                }
                div { class: "space-y-3 text-sm text-muted-foreground mb-6",
                    p {
                        "P2P trades on nostr.blue occur "
                        strong { class: "text-foreground", "directly between you and other users" }
                        ", facilitated by a Mostro node you connect to."
                    }
                    p {
                        "nostr.blue is "
                        strong { class: "text-foreground", "not a counterparty" }
                        " to any trade. We only render Mostro protocol data and relay GiftWrapped messages."
                    }
                    p {
                        "nostr.blue "
                        strong { class: "text-foreground", "cannot reverse, cancel, or refund" }
                        " trades. Hold invoices, bonds, and escrow are managed by the Mostro node."
                    }
                    div { class: "bg-amber-500/10 border border-amber-500/30 rounded-lg p-3",
                        p { class: "text-amber-700 dark:text-amber-300 font-medium text-xs",
                            "By proceeding you trade at your own risk. Start with small amounts while you learn the protocol."
                        }
                    }
                }
                if let Some(err) = error_message.read().as_ref() {
                    div { class: "mb-4 p-3 bg-destructive/10 border border-destructive/30 rounded-lg",
                        p { class: "text-destructive text-sm", "{err}" }
                    }
                }
                button {
                    class: "w-full py-3 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-semibold transition disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: *is_accepting.read(),
                    onclick: handle_accept,
                    if *is_accepting.read() {
                        "Saving Agreement..."
                    } else {
                        "I Understand & Accept"
                    }
                }
            }
        }
    }
}
