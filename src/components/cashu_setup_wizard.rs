use dioxus::prelude::*;
use crate::stores::cashu_wallet;

const DEFAULT_MINT_URL: &str = "https://mint.minibits.cash/Bitcoin";

#[component]
pub fn CashuSetupWizard(
    on_complete: EventHandler<()>,
) -> Element {
    let mut creating = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    rsx! {
        div {
            class: "max-w-2xl mx-auto p-4",

            div {
                class: "bg-card border border-border rounded-lg p-8 text-center",
                div {
                    class: "text-6xl mb-4",
                    "💰"
                }
                h2 {
                    class: "text-2xl font-bold mb-4",
                    "Create Cashu Wallet"
                }
                p {
                    class: "text-muted-foreground mb-6",
                    "Create a new Cashu wallet to store ecash tokens. Your wallet will be encrypted and stored on Nostr relays."
                }

                if *creating.read() {
                    div {
                        class: "text-4xl mb-4 animate-pulse",
                        "🔨"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Creating wallet..."
                    }
                } else if let Some(err) = error_msg.read().clone() {
                    div {
                        class: "p-3 bg-destructive/10 border border-destructive rounded-lg text-destructive text-sm mb-4",
                        {err}
                    }
                    button {
                        class: "px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition",
                        onclick: move |_| {
                            error_msg.set(None);
                            creating.set(false);
                        },
                        "Try Again"
                    }
                } else {
                    button {
                        class: "px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition",
                        onclick: move |_| {
                            creating.set(true);
                            error_msg.set(None); // Clear any previous error before starting
                            spawn(async move {
                                match cashu_wallet::create_wallet(vec![DEFAULT_MINT_URL.to_string()]).await {
                                    Ok(_) => {
                                        log::info!("Wallet created successfully");
                                        error_msg.set(None); // Clear error on success
                                        creating.set(false);
                                        on_complete.call(());
                                    }
                                    Err(e) => {
                                        log::error!("Failed to create wallet: {}", e);
                                        error_msg.set(Some(e));
                                        creating.set(false);
                                    }
                                }
                            });
                        },
                        "Create Wallet with Default Mint"
                    }
                    p {
                        class: "text-xs text-muted-foreground mt-4",
                        "Default mint: ", {DEFAULT_MINT_URL}
                    }
                }
            }
        }
    }
}
