#![allow(non_snake_case)]
use dioxus::prelude::*;
use stores::{
    auth_store, feed_cache, music_player, nostr_client, nwc_store, reactions_store, relay,
    settings_store, shop_store, sidebar_store, theme_store,
};
#[cfg(feature = "cashu")]
use stores::cashu;

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
use components::toast::ToastProvider;
pub use error::{NostrBlueError, Result};
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
    services::sync::use_sync_service();
    use_effect(move || {
        spawn(async move {
            stores::ui::scroll_restore::setup_popstate_flag().await;
            stores::ui::scroll_restore::setup_scroll_tracker().await;
            stores::ui::online_status::setup_online_status().await;
        });
    });
    use_effect(move || {
        theme_store::init_theme();
        auth_store::init_auth();
        music_player::init_player();
        sidebar_store::init_sidebar_from_cache();
        reactions_store::init_reactions_from_cache();
        relay::init_local_relays_from_cache();
        settings_store::init_settings_from_cache();
        stores::weather::location_store::init_from_cache();
        stores::weather::weather_store::init_from_cache();
        stores::weather::weather_settings::init_settings();
        spawn(async move {
            match nostr_client::initialize_client().await {
                Ok(_) => {
                    log::info!("Nostr client initialized");
                    if let Some(client) = nostr_client::get_client() {
                        relay::coverage::start_provenance_recorder(client);
                    }
                    auth_store::restore_session_async().await;
                    futures::join!(
                        nwc_store::restore_connection(),
                        async {
                            if let Err(e) = shop_store::init_shop_store().await {
                                log::warn!("Failed to initialize shop store: {}", e);
                            }
                        },
                        async {
                            if let Err(e) = feed_cache::init_feed_cache().await {
                                log::warn!("Failed to initialize feed cache: {}", e);
                            }
                        },
                        async {
                            let settings = settings_store::SETTINGS.read().clone();
                            #[cfg(feature = "cashu")]
                            if settings.cashu_wallet_auto_load {
                                if auth_store::get_pubkey().is_none() {
                                    log::debug!("Skipping Cashu auto-load: not authenticated");
                                    return;
                                }
                                match cashu::check_terms_accepted().await {
                                    Ok(true) => {
                                        log::info!("Auto-loading Cashu wallet...");
                                        if let Err(e) = cashu::init_wallet().await {
                                            log::warn!("Failed to auto-load Cashu wallet: {}", e);
                                        }
                                    }
                                    Ok(false) => {
                                        log::debug!(
                                            "Cashu terms not yet accepted, skipping auto-load"
                                        );
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to check Cashu terms: {}", e);
                                    }
                                }
                            }
                            let _ = settings;
                        },
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
            let sidebar_state = sidebar_store::SIDEBAR_STATE.read();
            let reactions_state = reactions_store::REACTIONS_STATE.read();
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
    use_effect(move || {
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        if is_authenticated {
            let sidebar_failed = sidebar_store::SIDEBAR_STATE.read().is_failed();
            let reactions_failed = reactions_store::REACTIONS_STATE.read().is_failed();
            if sidebar_failed || reactions_failed {
                spawn(async move {
                    if sidebar_store::SIDEBAR_STATE.peek().is_failed() {
                        sidebar_store::load_sidebar_preferences().await;
                    }
                    if reactions_store::REACTIONS_STATE.peek().is_failed() {
                        reactions_store::load_preferred_reactions().await;
                    }
                });
            }
        }
    });
    let tailwind_css: Option<Asset> = option_asset!("/public/tailwind.css");
    rsx! {
        if let Some(css) = tailwind_css {
            document::Stylesheet { href: css }
        }
        hooks::GlobalInteractionProcessor {}
        ToastProvider { Router::<routes::Route> {} }
        components::password_modal::PasswordModal {}
        components::MediaLightbox {}
    }
}
