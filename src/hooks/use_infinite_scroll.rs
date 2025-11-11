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

    // Trigger signal to communicate from IntersectionObserver (JS context) to Dioxus effect (Rust context)
    let trigger = use_signal(|| 0u64);

    // Store callback in hook so it persists across renders
    #[cfg_attr(not(target_family = "wasm"), allow(unused_variables))]
    let cb = use_hook(|| Rc::new(RefCell::new(callback)));

    // Clone sentinel_id for use in effect
    #[cfg_attr(not(target_family = "wasm"), allow(unused_variables))]
    let id_for_effect = sentinel_id.clone();

    // Effect to call the callback when trigger changes
    // This runs in Dioxus context, so spawn() is available
    use_effect(move || {
        let trigger_value = *trigger.read();

        log::info!("[InfiniteScroll] Trigger effect running - trigger value: {}", trigger_value);

        // Skip first render (trigger is 0)
        if trigger_value == 0 {
            log::info!("[InfiniteScroll] Skipping first render (trigger is 0)");
            return;
        }

        // Check if we should actually call the callback
        // This prevents infinite loops when loading state changes
        // Use peek() to avoid subscribing to these signals as dependencies
        let is_loading = *loading.peek();
        let has_more_items = *has_more.peek();

        log::info!("[InfiniteScroll] Guard check - is_loading: {}, has_more: {}", is_loading, has_more_items);

        if is_loading {
            log::info!("[InfiniteScroll] Trigger ignored - already loading");
            return;
        }

        if !has_more_items {
            log::info!("[InfiniteScroll] Trigger ignored - no more items");
            return;
        }

        log::info!("[InfiniteScroll] Trigger passed guards - calling callback");

        // Call the callback within Dioxus context
        if let Ok(mut callback) = cb.try_borrow_mut() {
            log::info!("[InfiniteScroll] Executing callback now");
            callback();
        } else {
            log::warn!("[InfiniteScroll] Callback already executing, skipping this trigger");
        }
    });

    // Setup observer once on mount with retries to find DOM element
    // Don't subscribe to signals - let callback guards handle enabled/disabled state
    #[cfg(target_family = "wasm")]
    use_hook(move || {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        log::info!("[InfiniteScroll] Setting up IntersectionObserver (one-time)");

        let id = id_for_effect.clone();
        let mut trigger_clone = trigger.clone();

        spawn(async move {
            log::info!("[InfiniteScroll] Async task started");

            let window = match web_sys::window() {
                Some(w) => w,
                None => {
                    log::error!("[InfiniteScroll] Failed to get window");
                    return;
                }
            };

            let document = match window.document() {
                Some(d) => d,
                None => {
                    log::error!("[InfiniteScroll] Failed to get document");
                    return;
                }
            };

            // Retry finding the element with exponential backoff
            let mut element = None;
            for attempt in 1..=10 {
                gloo_timers::future::TimeoutFuture::new(attempt * 100).await;

                if let Some(el) = document.get_element_by_id(&id) {
                    log::info!("[InfiniteScroll] Found sentinel element on attempt {}", attempt);
                    element = Some(el);
                    break;
                }
                log::debug!("[InfiniteScroll] Sentinel not found on attempt {}, retrying...", attempt);
            }

            let element = match element {
                Some(e) => e,
                None => {
                    log::warn!("[InfiniteScroll] Sentinel element never found after 10 attempts: {}", id);
                    return;
                }
            };

            // Create IntersectionObserver callback
            let mut last_check_for_callback = last_check.clone();

            let callback = Closure::wrap(Box::new(move |entries: js_sys::Array| {
                log::info!("[InfiniteScroll] IntersectionObserver callback fired, checking {} entries", entries.length());
                // Check if any entry is intersecting
                for i in 0..entries.length() {
                    if let Some(entry) = entries.get(i).dyn_into::<web_sys::IntersectionObserverEntry>().ok() {
                        let is_intersecting = entry.is_intersecting();
                        log::info!("[InfiniteScroll] Entry {} intersecting: {}", i, is_intersecting);

                        if is_intersecting {
                            // Debounce - only trigger once per second
                            let now = js_sys::Date::now() as u64;
                            let last = *last_check_for_callback.read();

                            log::info!("[InfiniteScroll] Debounce check - now: {}, last: {}, diff: {}", now, last, now - last);

                            if now - last > 1000 {
                                log::info!("[InfiniteScroll] Debounce passed - updating trigger signal to {}", now);
                                last_check_for_callback.set(now);

                                // Update trigger signal to invoke callback in Dioxus context
                                trigger_clone.set(now);
                                log::info!("[InfiniteScroll] Trigger signal updated successfully");
                            } else {
                                log::info!("[InfiniteScroll] Debounce blocked - too soon after last trigger");
                            }
                            break;
                        }
                    }
                }
            }) as Box<dyn FnMut(js_sys::Array)>);

            log::info!("[InfiniteScroll] Creating IntersectionObserver with 300px root margin");

            // Create IntersectionObserver with root margin for early triggering
            let options = web_sys::IntersectionObserverInit::new();
            options.set_root_margin("300px"); // Trigger 300px before element enters viewport

            let observer = match web_sys::IntersectionObserver::new_with_options(
                callback.as_ref().unchecked_ref(),
                &options
            ) {
                Ok(obs) => {
                    log::info!("[InfiniteScroll] IntersectionObserver created successfully");
                    obs
                },
                Err(e) => {
                    log::error!("[InfiniteScroll] Failed to create IntersectionObserver: {:?}", e);
                    return;
                }
            };

            // Start observing the sentinel element
            observer.observe(&element);
            log::info!("[InfiniteScroll] IntersectionObserver now watching sentinel element - setup complete");

            // Keep observer and callback alive for component lifetime
            std::mem::forget(observer);
            callback.forget();
        });

        // Return unit for use_hook
        ()
    });

    sentinel_id
}
