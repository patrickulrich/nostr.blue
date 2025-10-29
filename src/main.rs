#![allow(non_snake_case)]

use dioxus::prelude::*;
use stores::{auth_store, nostr_client, theme_store};

// Modules
mod components;
mod hooks;
mod routes;
mod services;
mod stores;
mod utils;

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
    #[cfg(target_arch = "wasm32")]
    dioxus::launch_cfg(
        App,
        dioxus::dioxus_core::prelude::Config::new().with_root_name("main")
    );

    #[cfg(not(target_arch = "wasm32"))]
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Initialize stores on mount
    use_effect(move || {
        theme_store::init_theme();
        auth_store::init_auth();

        // Initialize Nostr client
        spawn(async move {
            match nostr_client::initialize_client().await {
                Ok(_) => {
                    log::info!("Nostr client initialized");
                    // Restore signer from stored credentials
                    auth_store::restore_session_async().await;
                }
                Err(e) => log::error!("Failed to initialize client: {}", e),
            }
        });
    });

    rsx! {
        Router::<routes::Route> {}
    }
}

