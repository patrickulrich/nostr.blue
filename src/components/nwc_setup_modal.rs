use dioxus::prelude::*;
use crate::stores::nwc_store;
use crate::components::icons::CheckIcon;

/// NWC setup modal for connecting wallet
#[component]
pub fn NwcSetupModal(
    /// Handler to close the modal
    on_close: EventHandler<()>,
) -> Element {
    let mut nwc_uri = use_signal(|| String::new());
    let mut is_connecting = use_signal(|| false);
    let mut connection_error = use_signal(|| Option::<String>::None);
    let mut connection_success = use_signal(|| false);

    // Handle connection
    let handle_connect = move |_| {
        spawn(async move {
            is_connecting.set(true);
            connection_error.set(None);
            connection_success.set(false);

            let uri = nwc_uri.read().clone();

            match nwc_store::connect_nwc(&uri).await {
                Ok(()) => {
                    log::info!("NWC connected successfully");
                    connection_success.set(true);
                    // Close modal after a short delay
                    spawn(async move {
                        gloo_timers::future::TimeoutFuture::new(1500).await;
                        on_close.call(());
                    });
                }
                Err(e) => {
                    log::error!("Failed to connect NWC: {}", e);
                    connection_error.set(Some(e));
                    is_connecting.set(false);
                }
            }
        });
    };

    // Handle backdrop click
    let handle_backdrop_click = move |event: Event<MouseData>| {
        if event.data().trigger_button().is_some() {
            on_close.call(());
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50 p-4",
            onclick: handle_backdrop_click,

            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full p-6",
                onclick: |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between mb-4",
                    h3 {
                        class: "text-xl font-bold text-gray-900 dark:text-white",
                        "⚡ Connect Nostr Wallet"
                    }
                    button {
                        class: "text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 text-2xl",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }

                // Info text
                div {
                    class: "mb-4 text-sm text-gray-600 dark:text-gray-400",
                    p { class: "mb-2", "Paste your Nostr Wallet Connect URI to enable lightning payments." }
                    p { class: "mb-2",
                        "Get your connection URI from wallets like "
                        strong { "Alby" }
                        ", "
                        strong { "Mutiny" }
                        ", or other NWC-compatible wallets."
                    }
                }

                // URI Input
                div {
                    class: "mb-4",
                    label {
                        class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                        "Connection URI"
                    }
                    textarea {
                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg
                                bg-white dark:bg-gray-700 text-gray-900 dark:text-white
                                focus:outline-none focus:ring-2 focus:ring-purple-500
                                font-mono text-sm",
                        rows: 4,
                        placeholder: "nostr+walletconnect://...",
                        value: "{nwc_uri}",
                        oninput: move |e| nwc_uri.set(e.value()),
                        disabled: is_connecting() || connection_success(),
                    }
                }

                // Error message
                if let Some(error) = connection_error() {
                    div {
                        class: "mb-4 p-3 bg-red-50 dark:bg-red-900/30 border border-red-200
                                dark:border-red-800 rounded-lg",
                        p {
                            class: "text-sm text-red-800 dark:text-red-200",
                            "{error}"
                        }
                    }
                }

                // Success message
                if connection_success() {
                    div {
                        class: "mb-4 p-3 bg-green-50 dark:bg-green-900/30 border border-green-200
                                dark:border-green-800 rounded-lg flex items-center gap-2",
                        CheckIcon { class: "w-5 h-5 text-green-600 dark:text-green-400" }
                        p {
                            class: "text-sm text-green-800 dark:text-green-200",
                            "Wallet connected successfully!"
                        }
                    }
                }

                // Buttons
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600
                                rounded-lg text-gray-700 dark:text-gray-300
                                hover:bg-gray-50 dark:hover:bg-gray-700
                                transition-colors",
                        onclick: move |_| on_close.call(()),
                        disabled: is_connecting(),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 bg-purple-600 text-white rounded-lg
                                hover:bg-purple-700 disabled:bg-gray-400 disabled:cursor-not-allowed
                                transition-colors font-medium",
                        onclick: handle_connect,
                        disabled: is_connecting() || nwc_uri.read().trim().is_empty() || connection_success(),

                        if is_connecting() {
                            span { "Connecting..." }
                        } else if connection_success() {
                            span { "Connected ✓" }
                        } else {
                            span { "Connect Wallet" }
                        }
                    }
                }

                // Privacy note
                div {
                    class: "mt-4 p-3 bg-blue-50 dark:bg-blue-900/30 border border-blue-200
                            dark:border-blue-800 rounded-lg",
                    p {
                        class: "text-xs text-blue-800 dark:text-blue-200",
                        "🔒 Your connection URI is stored locally in your browser and never sent to our servers."
                    }
                }
            }
        }
    }
}
