#![allow(non_snake_case)]
use dioxus::prelude::*;
use stores::{
    auth_store, feed_cache, music_player, nostr_client, nwc_store, reactions_store, relay,
    settings_store, shop_store, sidebar_store, theme_store,
};
use stores::mostro;

#[cfg(all(feature = "web", feature = "native"))]
compile_error!("Cannot enable both 'web' and 'native' features simultaneously");

#[cfg(not(any(feature = "web", feature = "native")))]
compile_error!("Must enable either 'web' or 'native' feature");

mod components;
mod context;
mod error;
mod feeds;
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
        install_web_panic_hook();
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

#[cfg(feature = "web")]
static PANIC_HOOK_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "web")]
fn install_web_panic_hook() {
    console_error_panic_hook::set_once();
    let prev_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        // Re-entrancy guard: a panic raised *inside* this hook (e.g. the
        // `web_sys::console::error_1` call below failing because the externref
        // table is exhausted) would otherwise turn into a double-panic and
        // abort the WASM instance with `unreachable` before any catch_unwind
        // boundary can recover the original panic. Bail silently on re-entry.
        if PANIC_HOOK_IN_PROGRESS.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        // Defense-in-depth: even if the guard races or a JS interop call below
        // panics for some other reason, catch_unwind ensures we never escalate
        // a recoverable panic into a double-panic abort.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prev_hook(info);
            persist_panic_to_local_storage(info);
        }));
        PANIC_HOOK_IN_PROGRESS.store(false, std::sync::atomic::Ordering::SeqCst);
    }));
}

#[cfg(feature = "web")]
fn persist_panic_to_local_storage(info: &std::panic::PanicHookInfo<'_>) {
    let msg = info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
        .unwrap_or("<non-string panic>");
    let loc = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let full = format!("RUST PANIC: {} at {}", msg, loc);
    let js_val: wasm_bindgen::JsValue = full.clone().into();
    web_sys::console::error_1(&js_val);
    web_sys::console::log_1(&js_val);
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item("__rust_panic__", &full);
        }
    }
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
        mostro::init_node_config_from_cache();
        mostro::init();
        mostro::init_trades_from_cache();
        mostro::init_restore_from_cache();
        stores::ui::p2p_settings::init_from_cache();
        // Phase 12: detect browser locale for P2P i18n.
        stores::mostro::i18n::detect_locale();
        spawn(async move {
            match nostr_client::initialize_client().await {
                Ok(_) => {
                    log::info!("Nostr client initialized");
                    if let Some(client) = nostr_client::get_client() {
                        relay::coverage::start_provenance_recorder(client);
                    }
                    auth_store::restore_session_async().await;
                    // Mostro + Cashu terms checks moved into `run_post_login_init`
                    // (after `wait_for_user_relays`) so the kind 30078 fetches hit
                    // the user's NIP-65 outbox relays. Running them here raced the
                    // relay-pool population in `set_signer` and could return false
                    // negatives on NIP-46/55 logins, forcing a re-prompt.
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
                    );
                }
                Err(e) => {
                    log::error!("Failed to initialize client: {}", e);
                    *nostr_client::CLIENT_INITIALIZED.write() = true;
                }
            }
        });
    });
    use_effect(use_reactive(
        (
            &*nostr_client::RELAY_CONNECTED.read(),
            &auth_store::AUTH_STATE.read().is_authenticated,
        ),
        move |(connected, is_authenticated)| {
            if !connected || !is_authenticated {
                return;
            }
            // Use peek() (not read()) so these checks don't subscribe the
            // effect to the state signals. Otherwise every state transition
            // (Loading → Failed → Loading → ...) re-triggers the effect,
            // creating a tight retry loop.
            let sidebar_failed = sidebar_store::SIDEBAR_STATE.peek().is_failed();
            let reactions_failed = reactions_store::REACTIONS_STATE.peek().is_failed();
            let settings_failed = settings_store::SETTINGS_STATE.peek().is_failed();
            let p2p_failed = stores::ui::p2p_settings::MOSTRO_SETTINGS_STATE
                .peek()
                .is_failed();
            let ai_failed = stores::ui::ai_provider_store::AI_PROVIDER_STATE
                .peek()
                .is_failed();
            if sidebar_failed
                || reactions_failed
                || settings_failed
                || p2p_failed
                || ai_failed
            {
                // Backoff: don't retry more than once per 60s, even if
                // RELAY_CONNECTED flaps from the health poll.
                use std::sync::atomic::{AtomicU64, Ordering};
                static LAST_RETRY_MS: AtomicU64 = AtomicU64::new(0);
                let now = crate::platform::timestamp::now_millis();
                let last = LAST_RETRY_MS.load(Ordering::Relaxed);
                if now.wrapping_sub(last) < 60_000 {
                    return;
                }
                LAST_RETRY_MS.store(now, Ordering::Relaxed);

                log::info!("Retrying failed NIP-78 loads");
                spawn(async move {
                    if sidebar_store::SIDEBAR_STATE.peek().is_failed() {
                        sidebar_store::load_sidebar_preferences().await;
                    }
                    if reactions_store::REACTIONS_STATE.peek().is_failed() {
                        reactions_store::load_preferred_reactions().await;
                    }
                    if settings_store::SETTINGS_STATE.peek().is_failed() {
                        let _ = settings_store::load_settings().await;
                    }
                    if stores::ui::p2p_settings::MOSTRO_SETTINGS_STATE
                        .peek()
                        .is_failed()
                    {
                        let _ = stores::ui::p2p_settings::load_settings().await;
                    }
                    if stores::ui::ai_provider_store::AI_PROVIDER_STATE
                        .peek()
                        .is_failed()
                    {
                        stores::ui::ai_provider_store::sync_provider_state_from_relays().await;
                    }
                });
            }
        },
    ));
    let tailwind_css: Option<Asset> = option_asset!("/public/tailwind.css");
    rsx! {
        if let Some(css) = tailwind_css {
            document::Stylesheet { href: css }
        }
        hooks::GlobalInteractionProcessor {}
        ToastProvider {
            Router::<routes::Route> {}
        }
        components::password_modal::PasswordModal {}
        components::MediaLightbox {}
        components::mostro_toast_drainer::MostroBackgroundToastDrainer {}
        components::mostro_deeplink_handler::MostroDeepLinkHandler {}
    }
}
