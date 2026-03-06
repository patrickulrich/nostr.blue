#![allow(non_snake_case)]
use dioxus::prelude::*;
use stores::{
    auth_store, cashu, feed_cache, music_player, nostr_client, nwc_store,
    reactions_store, relay, settings_store, shop_store, sidebar_store, theme_store,
};

#[cfg(all(feature = "web", feature = "native"))]
compile_error!("Cannot enable both 'web' and 'native' features simultaneously");

#[cfg(not(any(feature = "web", feature = "native")))]
compile_error!("Must enable either 'web' or 'native' feature");

mod components;
mod context;
mod error;
mod hooks;
pub mod platform;
mod routes;
mod services;
mod stores;
mod utils;
pub use error::{NostrBlueError, Result};
use components::toast::ToastProvider;
fn main() {
    #[cfg(feature = "web")]
    {
        console_error_panic_hook::set_once();
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
    }
    #[cfg(feature = "native")]
    {
        let _ = env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .try_init();
    }
    log::info!("Starting nostr.blue Rust client");
    dioxus::launch(App);
}
#[component]
fn App() -> Element {
    services::scheduler::use_background_scheduler();
    use_effect(move || {
        theme_store::init_theme();
        auth_store::init_auth();
        music_player::init_player();
        sidebar_store::init_sidebar_from_cache();
        reactions_store::init_reactions_from_cache();
        relay::init_local_relays_from_cache();
        settings_store::init_settings_from_cache();
        spawn(async move {
            match nostr_client::initialize_client().await {
                Ok(_) => {
                    log::info!("Nostr client initialized");
                    auth_store::restore_session_async().await;
                    futures::join!(
                        reactions_store::load_preferred_reactions(),
                        sidebar_store::load_sidebar_preferences(),
                        nwc_store::restore_connection(), async { if let Err(e) =
                        shop_store::init_shop_store(). await {
                        log::warn!("Failed to initialize shop store: {}", e); } }, async
                        { if let Err(e) = feed_cache::init_feed_cache(). await {
                        log::warn!("Failed to initialize feed cache: {}", e); } }, async
                        { let settings = settings_store::SETTINGS.read().clone(); if
                        settings.cashu_wallet_auto_load { if auth_store::get_pubkey()
                        .is_none() {
                        log::debug!("Skipping Cashu auto-load: not authenticated");
                        return; } match cashu::check_terms_accepted(). await { Ok(true)
                        => { log::info!("Auto-loading Cashu wallet..."); if let Err(e) =
                        cashu::init_wallet(). await {
                        log::warn!("Failed to auto-load Cashu wallet: {}", e); } }
                        Ok(false) => {
                        log::debug!("Cashu terms not yet accepted, skipping auto-load");
                        } Err(e) => { log::warn!("Failed to check Cashu terms: {}", e); }
                        } } },
                    );
                }
                Err(e) => {
                    log::error!("Failed to initialize client: {}", e);
                    *nostr_client::CLIENT_INITIALIZED.write() = true;
                }
            }
        });
    });
    use_effect(move || {
        let connected = *nostr_client::RELAY_CONNECTED.read();
        if connected {
            let sidebar_state = sidebar_store::SIDEBAR_STATE.peek();
            let reactions_state = reactions_store::REACTIONS_STATE.peek();
            let sidebar_failed = sidebar_state.is_failed();
            let reactions_failed = reactions_state.is_failed();
            if sidebar_failed || reactions_failed {
                log::info!("Relay connected, retrying failed NIP-78 loads");
                spawn(async move {
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
        document::Stylesheet { href: asset!("/public/tailwind.css") }
        ToastProvider { Router::<routes::Route> {} }
        components::password_modal::PasswordModal {}
    }
}
