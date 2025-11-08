use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client, cashu_wallet};
use crate::components::ClientInitializing;

#[component]
pub fn CashuWallet() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let wallet_status = cashu_wallet::WALLET_STATUS.read();
    let wallet_state = cashu_wallet::WALLET_STATE.read();
    let mut show_setup_wizard = use_signal(|| false);
    let mut show_send_modal = use_signal(|| false);
    let mut show_receive_modal = use_signal(|| false);

    // Initialize wallet on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !auth_store::is_authenticated() || !client_initialized {
            return;
        }

        // Only initialize if not already loaded
        if matches!(*cashu_wallet::WALLET_STATUS.read(), cashu_wallet::WalletStatus::Uninitialized) {
            spawn(async move {
                if let Err(e) = cashu_wallet::init_wallet().await {
                    log::error!("Failed to initialize wallet: {}", e);
                }
            });
        }
    });

    // Check if we should show setup wizard
    let should_show_wizard = wallet_state.as_ref()
        .map(|w| !w.initialized)
        .unwrap_or(false) || *show_setup_wizard.read();

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",
                    h2 {
                        class: "text-xl font-bold",
                        "💰 Cashu Wallet"
                    }

                    // Refresh button (only show when wallet is ready)
                    if matches!(*wallet_status, cashu_wallet::WalletStatus::Ready) && !should_show_wizard {
                        button {
                            class: "px-3 py-1 text-sm bg-accent hover:bg-accent/80 rounded-lg transition",
                            onclick: move |_| {
                                spawn(async move {
                                    if let Err(e) = cashu_wallet::refresh_wallet().await {
                                        log::error!("Failed to refresh wallet: {}", e);
                                    }
                                });
                            },
                            "🔄 Refresh"
                        }
                    }
                }
            }

            // Not authenticated state
            if !auth.is_authenticated {
                div {
                    class: "text-center py-12 px-4",
                    div {
                        class: "text-6xl mb-4",
                        "🔐"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "Sign in to access your wallet"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Connect your account to create or access your Cashu wallet"
                    }
                }
            } else if !*nostr_client::CLIENT_INITIALIZED.read() {
                // Client initializing
                ClientInitializing {}
            } else if matches!(*wallet_status, cashu_wallet::WalletStatus::Loading) {
                // Loading wallet
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-4xl mb-4 animate-pulse",
                        "💰"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Loading wallet..."
                    }
                }
            } else if let cashu_wallet::WalletStatus::Error(error_msg) = &*wallet_status {
                // Error state
                div {
                    class: "text-center py-12 px-4",
                    div {
                        class: "text-6xl mb-4",
                        "⚠️"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2 text-destructive",
                        "Error loading wallet"
                    }
                    p {
                        class: "text-muted-foreground mb-4",
                        "{error_msg}"
                    }

                    // Check if it's a login method error
                    if error_msg.contains("private key login") {
                        div {
                            class: "bg-card border border-border rounded-lg p-4 max-w-md mx-auto text-left",
                            p {
                                class: "text-sm mb-2",
                                "NIP-60 Cashu wallets require access to your private key for encryption."
                            }
                            p {
                                class: "text-sm mb-2",
                                "Please sign in with your private key (nsec) to use this feature."
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Browser extensions and remote signers are not currently supported."
                            }
                        }
                    }

                    button {
                        class: "mt-4 px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                        onclick: move |_| {
                            spawn(async move {
                                if let Err(e) = cashu_wallet::init_wallet().await {
                                    log::error!("Failed to retry wallet init: {}", e);
                                }
                            });
                        },
                        "Retry"
                    }
                }
            } else if should_show_wizard {
                // Setup wizard
                crate::components::CashuSetupWizard {
                    on_complete: move |_| {
                        show_setup_wizard.set(false);
                        // Refresh wallet after setup
                        spawn(async move {
                            if let Err(e) = cashu_wallet::init_wallet().await {
                                log::error!("Failed to reload wallet after setup: {}", e);
                            }
                        });
                    }
                }
            } else {
                // Main wallet view
                div {
                    class: "max-w-4xl mx-auto p-4 pb-20",

                    // Balance card
                    crate::components::WalletBalanceCard {
                        on_send: move |_| show_send_modal.set(true),
                        on_receive: move |_| show_receive_modal.set(true),
                    }

                    // Tokens section
                    div {
                        class: "mt-6",
                        h3 {
                            class: "text-lg font-bold mb-3",
                            "Tokens by Mint"
                        }
                        crate::components::TokenList {}
                    }

                    // Transaction history
                    div {
                        class: "mt-6",
                        h3 {
                            class: "text-lg font-bold mb-3",
                            "Transaction History"
                        }
                        crate::components::TransactionHistory {}
                    }
                }
            }

            // Send modal
            if *show_send_modal.read() {
                crate::components::CashuSendModal {
                    on_close: move |_| show_send_modal.set(false),
                }
            }

            // Receive modal
            if *show_receive_modal.read() {
                crate::components::CashuReceiveModal {
                    on_close: move |_| show_receive_modal.set(false),
                }
            }
        }
    }
}
