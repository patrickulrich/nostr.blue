use crate::stores::cashu;
use dioxus::prelude::*;
/// Modal for accepting Cashu wallet terms and disclaimer
/// This modal is non-dismissible - users must accept to proceed
#[component]
pub fn CashuTermsModal(on_accept: EventHandler<()>) -> Element {
    let mut is_accepting = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let handle_accept = move |_| {
        if *is_accepting.read() {
            return;
        }
        is_accepting.set(true);
        error_message.set(None);
        spawn(async move {
            match cashu::accept_terms().await {
                Ok(()) => {
                    on_accept.call(());
                }
                Err(e) => {
                    error_message.set(Some(e));
                    is_accepting.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "fixed inset-0 bg-black/70 z-50 flex items-center justify-center p-4",
            div {
                class: "bg-card border border-border rounded-xl max-w-md w-full p-6 shadow-xl",
                onclick: move |e| e.stop_propagation(),
                div { class: "text-center text-6xl mb-4", "⚠️" }
                h2 { class: "text-xl font-bold text-center mb-4 text-destructive",
                    "Experimental Wallet - Read Carefully"
                }
                div { class: "space-y-4 text-sm text-muted-foreground mb-6",
                    p {
                        "This Cashu wallet is a "
                        strong { class: "text-foreground", "work in progress" }
                        " that has been \"vibe coded\" and "
                        strong { class: "text-destructive", "may lose your funds" }
                        "."
                    }
                    p {
                        "You should "
                        strong { class: "text-foreground", "only interact with disposable amounts" }
                        " that you fully intend to lose."
                    }
                    div { class: "bg-destructive/10 border border-destructive/30 rounded-lg p-3",
                        p { class: "text-destructive font-medium",
                            "Do NOT use this with accounts containing NIP-60 balances you cannot afford to lose."
                        }
                    }
                }
                if let Some(err) = error_message.read().as_ref() {
                    div { class: "mb-4 p-3 bg-destructive/10 border border-destructive/30 rounded-lg",
                        p { class: "text-destructive text-sm", "{err}" }
                    }
                }
                button {
                    class: "w-full py-3 bg-destructive hover:bg-destructive/90 text-destructive-foreground rounded-lg font-semibold transition disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: *is_accepting.read(),
                    onclick: handle_accept,
                    if *is_accepting.read() {
                        "Saving Agreement..."
                    } else {
                        "I Understand & Accept the Risks"
                    }
                }
            }
        }
    }
}
