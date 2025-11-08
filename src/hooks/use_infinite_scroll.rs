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

    #[cfg_attr(not(target_family = "wasm"), allow(unused_variables))]
    let last_check = use_signal(|| 0u64);

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

        log::debug!("Infinite scroll enabled, setting up IntersectionObserver");

        #[cfg(target_family = "wasm")]
        {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;

            let id = id_for_effect.clone();
            let cb_clone = cb.clone();

            spawn(async move {
                // Wait a bit for the DOM to be ready
                gloo_timers::future::TimeoutFuture::new(100).await;

                let window = match web_sys::window() {
                    Some(w) => w,
                    None => {
                        log::warn!("Failed to get window for IntersectionObserver");
                        return;
                    }
                };

                let document = match window.document() {
                    Some(d) => d,
                    None => {
                        log::warn!("Failed to get document for IntersectionObserver");
                        return;
                    }
                };

                let element = match document.get_element_by_id(&id) {
                    Some(e) => e,
                    None => {
                        log::debug!("Sentinel element not found yet, will retry");
                        return;
                    }
                };

                // Create IntersectionObserver callback
                let callback_ref = cb_clone.clone();
                let mut last_check_for_callback = last_check.clone();

                let callback = Closure::wrap(Box::new(move |entries: js_sys::Array| {
                    // Check if any entry is intersecting
                    for i in 0..entries.length() {
                        if let Some(entry) = entries.get(i).dyn_into::<web_sys::IntersectionObserverEntry>().ok() {
                            if entry.is_intersecting() {
                                // Debounce - only trigger once per second
                                let now = js_sys::Date::now() as u64;
                                let last = *last_check_for_callback.read();

                                if now - last > 1000 {
                                    log::info!("IntersectionObserver triggered - calling load_more callback");
                                    last_check_for_callback.set(now);
                                    callback_ref.borrow_mut()();
                                }
                                break;
                            }
                        }
                    }
                }) as Box<dyn FnMut(js_sys::Array)>);

                // Create IntersectionObserver with root margin for early triggering
                let mut options = web_sys::IntersectionObserverInit::new();
                options.set_root_margin("300px"); // Trigger 300px before element enters viewport

                let observer = match web_sys::IntersectionObserver::new_with_options(
                    callback.as_ref().unchecked_ref(),
                    &options
                ) {
                    Ok(obs) => obs,
                    Err(e) => {
                        log::error!("Failed to create IntersectionObserver: {:?}", e);
                        return;
                    }
                };

                // Start observing the sentinel element
                observer.observe(&element);
                log::debug!("IntersectionObserver now watching sentinel element");

                // Keep callback alive
                callback.forget();

                // Note: In a production app, you'd want to properly clean up the observer
                // when the component unmounts. For now, the observer will be garbage collected
                // when the effect re-runs or component unmounts.
            });
        }
    });

    sentinel_id
}
