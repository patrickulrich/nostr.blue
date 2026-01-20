#![allow(non_snake_case)]

use dioxus::prelude::*;
use stores::{auth_store, nostr_client, theme_store, music_player, nwc_store, reactions_store, shop_store, sidebar_store};

// Modules
mod components;
mod context;
mod error;
mod hooks;
mod routes;
mod services;
mod stores;
mod utils;

// Re-export error types for convenience
pub use error::{NostrBlueError, Result};

use components::toast::ToastProvider;

fn main() {
    // Initialize panic hook for better error messages in browser console
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        // Set log level to INFO to filter out DEBUG messages from relay pool
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
    }

    log::info!("Starting nostr.blue Rust client");

    // Launch the Dioxus web app
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Start background scheduler tasks
    services::scheduler::use_background_scheduler();

    // Initialize stores on mount
    use_effect(move || {
        theme_store::init_theme();
        auth_store::init_auth();
        music_player::init_player();

        // Load cached preferences immediately (synchronous)
        // This provides instant UI before async client init
        sidebar_store::init_sidebar_from_cache();
        reactions_store::init_reactions_from_cache();

        // Initialize Nostr client (async)
        spawn(async move {
            match nostr_client::initialize_client().await {
                Ok(_) => {
                    log::info!("Nostr client initialized");
                    // Restore signer from stored credentials (must run first to check auth state)
                    auth_store::restore_session_async().await;

                    // Run all remaining operations in parallel for faster startup
                    futures::join!(
                        // Load user's preferred reactions from Nostr (NIP-78)
                        reactions_store::load_preferred_reactions(),
                        // Load user's sidebar preferences from Nostr (NIP-78)
                        sidebar_store::load_sidebar_preferences(),
                        // Restore NWC connection from LocalStorage
                        nwc_store::restore_connection(),
                        // Initialize shop store and restore persisted orders
                        async {
                            if let Err(e) = shop_store::init_shop_store().await {
                                log::warn!("Failed to initialize shop store: {}", e);
                            }
                        },
                    );
                }
                Err(e) => {
                    log::error!("Failed to initialize client: {}", e);
                    // Still mark as initialized to prevent infinite loading
                    // The app can work in read-only mode
                    *nostr_client::CLIENT_INITIALIZED.write() = true;
                }
            }
        });
    });

    // Reactive NIP-78 retry when relay connects
    use_effect(move || {
        // Track RELAY_CONNECTED - effect re-runs when this changes
        let connected = *nostr_client::RELAY_CONNECTED.read();

        if connected {
            // Use peek() to check state WITHOUT subscribing (avoid re-run loops)
            let sidebar_state = sidebar_store::SIDEBAR_STATE.peek();
            let reactions_state = reactions_store::REACTIONS_STATE.peek();

            let sidebar_failed = sidebar_state.is_failed();
            let reactions_failed = reactions_state.is_failed();

            if sidebar_failed || reactions_failed {
                log::info!("Relay connected, retrying failed NIP-78 loads");
                spawn(async move {
                    // The load functions have their own Loading guard
                    // to prevent duplicate concurrent fetches
                    if sidebar_failed {
                        sidebar_store::load_sidebar_preferences().await;
                    }
                    if reactions_failed {
                        reactions_store::load_preferred_reactions().await;
                    }
                });
            }
        }
    });

    rsx! {
        // Tailwind CSS (processed by Dioxus built-in Tailwind support)
        document::Stylesheet { href: asset!("/assets/tailwind.css") }

        ToastProvider {
            Router::<routes::Route> {}
        }
        // NIP-49 password modal for encrypted key unlock/migration
        components::password_modal::PasswordModal {}
    }
}

