use dioxus::prelude::*;
use std::rc::Rc;
use std::cell::RefCell;

/// Infinite scroll hook that automatically triggers loading when sentinel element enters viewport
///
/// Returns a unique ID that should be assigned to a sentinel element at the bottom of your scrollable content.
/// When this element comes into view, the callback will be triggered to load more content.
///
/// # Arguments
/// * `callback` - Function to call when more content should be loaded
/// * `has_more` - Signal indicating whether there's more content to load
/// * `loading` - Signal indicating whether content is currently loading
///
/// # Example
/// ```
/// let sentinel_id = use_infinite_scroll(
///     move || load_more(),
///     has_more,
///     loading
/// );
///
/// // In your rsx:
/// div { id: "{sentinel_id}", class: "h-4" }
/// ```
pub fn use_infinite_scroll<F>(
    callback: F,
    has_more: Signal<bool>,
    loading: Signal<bool>
) -> String
where
    F: FnMut() + 'static,
{
    let sentinel_id = use_hook(|| format!("scroll-sentinel-{}", uuid::Uuid::new_v4()));

    #[cfg_attr(not(target_family = "wasm"), allow(unused_mut, unused_variables))]
    let mut last_check = use_signal(|| 0u64);

    // Store callback in hook so it persists across renders
    #[cfg_attr(not(target_family = "wasm"), allow(unused_variables))]
    let cb = use_hook(|| Rc::new(RefCell::new(callback)));

    // Clone sentinel_id for use in effect
    #[cfg_attr(not(target_family = "wasm"), allow(unused_variables))]
    let id_for_effect = sentinel_id.clone();

    use_effect(move || {
        // Read signals to track them as dependencies
        let enabled = *has_more.read() && !*loading.read();

        if !enabled {
            log::debug!("Infinite scroll disabled: has_more={}, loading={}", has_more.read(), loading.read());
            return;
        }

        log::debug!("Infinite scroll enabled, starting polling loop");

        #[cfg(target_family = "wasm")]
        {
            let id = id_for_effect.clone();
            let cb_clone = cb.clone();

            spawn(async move {
                loop {
                    // Sleep for 300ms between checks
                    gloo_timers::future::TimeoutFuture::new(300).await;

                    // Check if element is in viewport
                    let should_load = {
                        use wasm_bindgen::JsCast;

                        if let Some(window) = web_sys::window() {
                            if let Some(document) = window.document() {
                                if let Some(element) = document.get_element_by_id(&id) {
                                    if let Ok(html_element) = element.dyn_into::<web_sys::HtmlElement>() {
                                        let rect = html_element.get_bounding_client_rect();
                                        let window_height = window.inner_height()
                                            .ok()
                                            .and_then(|h| h.as_f64())
                                            .unwrap_or(0.0);

                                        // Check if element is in viewport (with 300px threshold)
                                        let is_visible = rect.top() < window_height + 300.0 && rect.bottom() >= 0.0;

                                        if is_visible {
                                            log::debug!("Sentinel visible: top={}, window_height={}", rect.top(), window_height);
                                        }

                                        is_visible
                                    } else {
                                        false
                                    }
                                } else {
                                    // Element not found - might be removed because has_more became false
                                    // Break the loop so effect can restart if conditions change
                                    log::debug!("Sentinel element not found, stopping loop");
                                    return;
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    if should_load {
                        // Debounce - only trigger once per second
                        let now = js_sys::Date::now() as u64;
                        let last = *last_check.read();

                        if now - last > 1000 {
                            log::info!("Infinite scroll triggered - calling load_more callback");
                            last_check.set(now);
                            cb_clone.borrow_mut()();
                            // Continue polling to detect when more content should load
                        }
                    }
                }
            });
        }
    });

    sentinel_id
}
