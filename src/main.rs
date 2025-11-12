#![allow(non_snake_case)]

use dioxus::prelude::*;
use stores::{auth_store, nostr_client, theme_store, music_player, nwc_store};

// Modules
mod components;
mod context;
mod hooks;
mod routes;
mod services;
mod stores;
mod utils;

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
    // Initialize stores on mount
    use_effect(move || {
        theme_store::init_theme();
        auth_store::init_auth();
        music_player::init_player();

        // Initialize Nostr client
        spawn(async move {
            match nostr_client::initialize_client().await {
                Ok(_) => {
                    log::info!("Nostr client initialized");
                    // Restore signer from stored credentials
                    auth_store::restore_session_async().await;

                    // Restore NWC connection from LocalStorage
                    nwc_store::restore_connection().await;
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

    rsx! {
        ToastProvider {
            Router::<routes::Route> {}
        }
    }
}

