//! Mostro deep link handler component.
//!
//! Phase 11: mounted at the app root. Does two things:
//!
//! 1. **Detection**: checks `window.location.hash` on initial load and
//!    listens for `hashchange` events for `mostro:` prefixed URLs. When
//!    found, parses the URL and dispatches `handle_mostro_deep_link`.
//!
//! 2. **Navigation**: watches `PENDING_DEEP_LINK` and navigates to
//!    `P2POrderDetail` when a pending deep link is set.

use dioxus::prelude::*;

use crate::routes::Route;
use crate::services::mostro_deeplink;

/// Root-level component that handles Mostro deep links.
/// Renders nothing — it's a side-effect component.
#[component]
pub fn MostroDeepLinkHandler() -> Element {
    // Phase 11.2: on mount, check window.location.hash for a `mostro:`
    // deep link and listen for hashchange events.
    use_future(move || async move {
        #[cfg(feature = "web")]
        {
            // Check initial hash.
            check_hash_for_deep_link();

            // Listen for hashchange events.
            loop {
                crate::platform::timer::sleep(std::time::Duration::from_millis(500)).await;
                check_hash_for_deep_link();
            }
        }
        #[cfg(not(feature = "web"))]
        {
            loop {
                crate::platform::timer::sleep(std::time::Duration::from_secs(1)).await;
                if let Ok(Some(url)) = crate::platform::storage::get::<Option<String>>("mostro_pending_deeplink") {
                    let _ = crate::platform::storage::delete("mostro_pending_deeplink");
                    process_mostro_url(&url).await;
                }
            }
        }
    });

    // Phase 11.3: navigate when a pending deep link is set.
    let _pending = crate::stores::mostro::deeplink::PENDING_DEEP_LINK();
    use_effect(move || {
        if let Some(naddr) = crate::stores::mostro::deeplink::take_pending_deep_link() {
            let _ = navigator().replace(Route::MostroOrderDetail { naddr });
        }
    });

    rsx! {}
}

#[cfg(feature = "web")]
fn check_hash_for_deep_link() {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let hash = window.location().hash().unwrap_or_default();
    let hash = hash.strip_prefix('#').unwrap_or(&hash);
    if hash.starts_with("mostro:") {
        // Clear the hash so we don't re-trigger on every poll.
        if let Ok(h) = window.history() {
            let _ = h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", None);
        }
        let url = hash.to_string();
        spawn(async move {
            process_mostro_url(&url).await;
        });
    }
}

async fn process_mostro_url(url: &str) {
    let link = match mostro_deeplink::parse_mostro_url(url) {
        Some(l) => l,
        None => {
            log::debug!("Unparseable mostro deep link: {url}");
            return;
        }
    };

    log::info!(
        "Mostro deep link: order {} on daemon {}",
        link.order_id,
        link.mostro_pubkey.to_hex()
    );

    // Handle the link (may switch daemon + set pending deep link).
    if let Err(e) = mostro_deeplink::handle_mostro_deep_link(&link).await {
        log::warn!("Deep link handling failed: {e}");
        crate::stores::mostro::enqueue_background_toast(
            "Deep link failed".to_string(),
            e,
        );
    }
}
