#[cfg(feature = "cashu")]
use crate::stores::{auth_store, cashu, nostr_client};
use dioxus::prelude::*;

#[cfg(feature = "cashu")]
#[component]
pub fn CashuWallet() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let wallet_status = cashu::WALLET_STATUS.read();
    let wallet_state = cashu::WALLET_STATE.read();
    let terms_status = cashu::TERMS_ACCEPTED.read();
    let mut show_setup_wizard = use_signal(|| false);
    let mut show_send_modal = use_signal(|| false);
    let mut show_receive_modal = use_signal(|| false);
    let mut show_lightning_deposit_modal = use_signal(|| false);
    let mut show_lightning_withdraw_modal = use_signal(|| false);
    let mut show_optimize_modal = use_signal(|| false);
    let mut show_add_mint_modal = use_signal(|| false);
    let mut show_discover_modal = use_signal(|| false);
    let mut show_transfer_modal = use_signal(|| false);
    let mut show_create_request_modal = use_signal(|| false);
    let mut show_pay_request_modal = use_signal(|| false);
    let mut show_nutzap_settings_modal = use_signal(|| false);
    let mut show_nutzap_inbox_modal = use_signal(|| false);
    let mut init_started = use_signal(|| false);
    use_effect(move || {
        if *init_started.peek() {
            return;
        }
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !auth_store::is_authenticated() || !client_initialized {
            return;
        }
        let terms = *cashu::TERMS_ACCEPTED.read();
        if terms.is_none() {
            init_started.set(true);
            spawn(async move {
                match cashu::check_terms_accepted().await {
                    Ok(accepted) => {
                        if accepted {
                            if let Err(e) = cashu::init_wallet().await {
                                log::error!("Failed to initialize wallet: {}", e);
                            }
                        }
                    }
                    Err(e) => log::error!("Failed to check terms: {}", e),
                }
            });
            return;
        }
        if terms == Some(true)
            && matches!(
                *cashu::WALLET_STATUS.read(),
                cashu::WalletStatus::Uninitialized
            )
        {
            init_started.set(true);
            spawn(async move {
                if let Err(e) = cashu::init_wallet().await {
                    log::error!("Failed to initialize wallet: {}", e);
                }
            });
        }
    });
    let should_show_wizard = wallet_state
        .as_ref()
        .map(|w| !w.initialized)
        .unwrap_or(false)
        || *show_setup_wizard.read();
    let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
    rsx! {
        div { class: "min-h-screen bg-background",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    h2 { class: "text-xl font-bold", "💰 Cashu Wallet" }
                    if matches!(*wallet_status, cashu::WalletStatus::Ready) && !should_show_wizard {
                        button {
                            class: "px-3 py-1 text-sm bg-accent hover:bg-accent/80 rounded-lg transition",
                            onclick: move |_| {
                                spawn(async move {
                                    if let Err(e) = cashu::refresh_wallet().await {
                                        log::error!("Failed to refresh wallet: {}", e);
                                    }
                                });
                            },
                            "🔄 Refresh"
                        }
                    }
                }
            }
            if !auth.is_authenticated {
                div { class: "text-center py-12 px-4",
                    div { class: "text-6xl mb-4", "🔐" }
                    h3 { class: "text-xl font-semibold mb-2", "Sign in to access your wallet" }
                    p { class: "text-muted-foreground",
                        "Connect your account to create or access your Cashu wallet"
                    }
                }
            } else if *terms_status == Some(false) {
                crate::components::CashuTermsModal {
                    on_accept: move |_| {
                        spawn(async move {
                            if let Err(e) = cashu::init_wallet().await {
                                log::error!("Failed to initialize wallet: {}", e);
                            }
                        });
                    },
                }
            } else if !client_initialized || terms_status.is_none()
                || matches!(*wallet_status, cashu::WalletStatus::Loading)
            {
                div { class: "flex flex-col items-center justify-center py-20",
                    div { class: "mb-6 motion-safe:animate-bounce",
                        div { class: "w-20 h-20 flex items-center justify-center rounded-xl bg-gradient-to-br from-purple-500 to-pink-500 shadow-lg",
                            span { class: "text-5xl font-bold text-white", "N" }
                        }
                    }
                    div { class: "text-center",
                        h2 { class: "text-xl font-semibold text-foreground mb-2",
                            if !client_initialized {
                                "Client Initializing"
                            } else if terms_status.is_none() {
                                "Checking Access"
                            } else {
                                "Loading Wallet"
                            }
                        }
                        p { class: "text-sm text-muted-foreground",
                            if !client_initialized {
                                "Connecting to the Nostr network..."
                            } else if terms_status.is_none() {
                                "Verifying wallet terms acceptance..."
                            } else {
                                "Fetching your Cashu wallet..."
                            }
                        }
                    }
                    div { class: "flex gap-2 mt-6",
                        for (i , delay) in [0.0, 0.2, 0.4].iter().enumerate() {
                            div {
                                key: "{i}",
                                class: "w-3 h-3 rounded-full bg-purple-500",
                                style: "animation: wallet-pulse 1.5s ease-in-out {delay}s infinite;",
                            }
                        }
                    }
                }
            } else if let cashu::WalletStatus::Error(error_msg) = &*wallet_status {
                div { class: "text-center py-12 px-4",
                    div { class: "text-6xl mb-4", "⚠️" }
                    h3 { class: "text-xl font-semibold mb-2 text-destructive", "Error loading wallet" }
                    p { class: "text-muted-foreground mb-4", "{error_msg}" }
                    if error_msg.contains("private key login") {
                        div { class: "bg-card border border-border rounded-lg p-4 max-w-md mx-auto text-left",
                            p { class: "text-sm mb-2",
                                "NIP-60 Cashu wallets require access to your private key for encryption."
                            }
                            p { class: "text-sm mb-2",
                                "Please sign in with your private key (nsec) to use this feature."
                            }
                            p { class: "text-sm text-muted-foreground",
                                "If you're using a browser extension or remote signer, please authorize it or try reconnecting so it can decrypt."
                            }
                        }
                    }
                    button {
                        class: "mt-4 px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                        onclick: move |_| {
                            spawn(async move {
                                if let Err(e) = cashu::init_wallet().await {
                                    log::error!("Failed to retry wallet init: {}", e);
                                }
                            });
                        },
                        "Retry"
                    }
                }
            } else if should_show_wizard {
                crate::components::CashuSetupWizard {
                    on_complete: move |_| {
                        show_setup_wizard.set(false);
                        spawn(async move {
                            if let Err(e) = cashu::init_wallet().await {
                                log::error!("Failed to reload wallet after setup: {}", e);
                            }
                        });
                    },
                }
            } else {
                div { class: "max-w-4xl mx-auto p-4 pb-20",
                    crate::components::WalletBalanceCard {
                        on_send: move |_| show_send_modal.set(true),
                        on_receive: move |_| show_receive_modal.set(true),
                        on_lightning_deposit: move |_| show_lightning_deposit_modal.set(true),
                        on_lightning_withdraw: move |_| show_lightning_withdraw_modal.set(true),
                        on_optimize: move |_| show_optimize_modal.set(true),
                        on_transfer: move |_| show_transfer_modal.set(true),
                        on_create_request: move |_| show_create_request_modal.set(true),
                        on_pay_request: move |_| show_pay_request_modal.set(true),
                        on_nutzap_settings: move |_| show_nutzap_settings_modal.set(true),
                        on_nutzap_inbox: move |_| show_nutzap_inbox_modal.set(true),
                    }
                    div { class: "mt-6",
                        div { class: "flex items-center justify-between mb-3",
                            h3 { class: "text-lg font-bold", "Tokens by Mint" }
                            div { class: "flex items-center gap-2",
                                button {
                                    class: "px-3 py-1 text-sm bg-purple-500/20 hover:bg-purple-500/30 text-purple-600 dark:text-purple-400 rounded-lg transition flex items-center gap-1",
                                    onclick: move |_| show_discover_modal.set(true),
                                    span { "!" }
                                    "Discover"
                                }
                                button {
                                    class: "px-3 py-1 text-sm bg-accent hover:bg-accent/80 rounded-lg transition flex items-center gap-1",
                                    onclick: move |_| show_add_mint_modal.set(true),
                                    span { "+" }
                                    "Add Mint"
                                }
                            }
                        }
                        crate::components::TokenList {}
                    }
                    div { class: "mt-6",
                        h3 { class: "text-lg font-bold mb-3", "Transaction History" }
                        crate::components::TransactionHistory {}
                    }
                }
            }
            if *show_send_modal.read() {
                crate::components::CashuSendModal { on_close: move |_| show_send_modal.set(false) }
            }
            if *show_receive_modal.read() {
                crate::components::CashuReceiveModal { on_close: move |_| show_receive_modal.set(false) }
            }
            if *show_lightning_deposit_modal.read() {
                crate::components::CashuReceiveLightningModal { on_close: move |_| show_lightning_deposit_modal.set(false) }
            }
            if *show_lightning_withdraw_modal.read() {
                crate::components::CashuSendLightningModal { on_close: move |_| show_lightning_withdraw_modal.set(false) }
            }
            if *show_optimize_modal.read() {
                crate::components::CashuOptimizeModal { on_close: move |_| show_optimize_modal.set(false) }
            }
            if *show_add_mint_modal.read() {
                crate::components::CashuAddMintModal {
                    on_close: move |_| show_add_mint_modal.set(false),
                    on_mint_added: move |_| {
                        spawn(async move {
                            if let Err(e) = cashu::refresh_wallet().await {
                                log::error!("Failed to refresh after adding mint: {}", e);
                            }
                        });
                    },
                }
            }
            if *show_discover_modal.read() {
                crate::components::CashuMintDiscoveryModal {
                    on_close: move |_| show_discover_modal.set(false),
                    on_mint_selected: move |_| {
                        spawn(async move {
                            if let Err(e) = cashu::refresh_wallet().await {
                                log::error!("Failed to refresh after adding mint: {}", e);
                            }
                        });
                    },
                }
            }
            if *show_transfer_modal.read() {
                crate::components::CashuTransferModal { on_close: move |_| show_transfer_modal.set(false) }
            }
            if *show_create_request_modal.read() {
                crate::components::CashuCreateRequestModal { on_close: move |_| show_create_request_modal.set(false) }
            }
            if *show_pay_request_modal.read() {
                crate::components::CashuPayRequestModal { on_close: move |_| show_pay_request_modal.set(false) }
            }
            if *show_nutzap_settings_modal.read() {
                crate::components::cashu::NutzapSettingsModal { on_close: move |_| show_nutzap_settings_modal.set(false) }
            }
            if *show_nutzap_inbox_modal.read() {
                crate::components::cashu::NutzapInbox { on_close: move |_| show_nutzap_inbox_modal.set(false) }
            }
        }
    }
}

#[cfg(not(feature = "cashu"))]
#[component]
pub fn CashuWallet() -> Element {
    use dioxus::prelude::*;
    rsx! {
        div { class: "min-h-screen bg-background flex items-center justify-center",
            div { class: "text-center py-12 px-4",
                div { class: "text-6xl mb-4", "🔒" }
                h3 { class: "text-xl font-semibold mb-2", "Not Available" }
                p { class: "text-muted-foreground",
                    "Wallet is not available on this platform."
                }
            }
        }
    }
}
