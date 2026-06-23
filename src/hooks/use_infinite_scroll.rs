use dioxus::prelude::*;
#[cfg(feature = "mobile_platform")]
use dioxus_core::{use_drop, Task};
use std::cell::RefCell;
use std::rc::Rc;
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
pub fn use_infinite_scroll<F>(callback: F, has_more: Signal<bool>, loading: Signal<bool>) -> String
where
    F: FnMut() + 'static,
{
    let sentinel_id = use_hook(|| format!("scroll-sentinel-{}", uuid::Uuid::new_v4()));
    #[cfg_attr(not(any(feature = "web", feature = "mobile_platform")), allow(unused_variables))]
    let last_check = use_signal(|| 0u64);
    let trigger = use_signal(|| 0u64);
    #[cfg_attr(not(any(feature = "web", feature = "mobile_platform")), allow(unused_variables))]
    let cb = use_hook(|| Rc::new(RefCell::new(callback)));
    #[cfg_attr(not(any(feature = "web", feature = "mobile_platform")), allow(unused_variables))]
    let id_for_effect = sentinel_id.clone();
    use_effect(move || {
        let trigger_value = *trigger.read();
        log::info!(
            "[InfiniteScroll] Trigger effect running - trigger value: {}",
            trigger_value
        );
        if trigger_value == 0 {
            log::info!("[InfiniteScroll] Skipping first render (trigger is 0)");
            return;
        }
        let is_loading = *loading.peek();
        let has_more_items = *has_more.peek();
        log::info!(
            "[InfiniteScroll] Guard check - is_loading: {}, has_more: {}",
            is_loading,
            has_more_items
        );
        if is_loading {
            log::info!("[InfiniteScroll] Trigger ignored - already loading");
            return;
        }
        if !has_more_items {
            log::info!("[InfiniteScroll] Trigger ignored - no more items");
            return;
        }
        log::info!("[InfiniteScroll] Trigger passed guards - calling callback");
        if let Ok(mut callback) = cb.try_borrow_mut() {
            log::info!("[InfiniteScroll] Executing callback now");
            callback();
        } else {
            log::warn!("[InfiniteScroll] Callback already executing, skipping this trigger");
        }
    });
    #[cfg(feature = "web")]
    {
        let observer_handles = use_hook(|| {
            Rc::new(RefCell::new(
                None::<(
                    web_sys::IntersectionObserver,
                    wasm_bindgen::closure::Closure<dyn FnMut(js_sys::Array)>,
                )>,
            ))
        });
        let setup_task =
            use_hook(|| Rc::new(RefCell::new(None::<dioxus_core::Task>)));
        let handles_for_drop = observer_handles.clone();
        let task_for_drop = setup_task.clone();
        let mut observer_setup_done = use_signal(|| false);
        use_effect(move || {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;
            let has_more_value = *has_more.read();
            if !has_more_value {
                log::debug!("[InfiniteScroll] has_more is false, skipping observer setup");
                if let Some(task) = setup_task.borrow_mut().take() {
                    task.cancel();
                }
                if let Some((observer, _)) = observer_handles.borrow_mut().take() {
                    observer.disconnect();
                    log::debug!("[InfiniteScroll] Disconnected existing observer");
                }
                observer_setup_done.set(false);
                return;
            }
            if *observer_setup_done.peek() {
                log::debug!("[InfiniteScroll] Observer already set up, skipping");
                return;
            }
            observer_setup_done.set(true);
            log::info!("[InfiniteScroll] Setting up IntersectionObserver (has_more became true)");
            let id = id_for_effect.clone();
            let mut trigger_clone = trigger;
            let observer_handles_clone = observer_handles.clone();
            let mut last_check_for_callback = last_check;
            let mut observer_setup_done_for_reset = observer_setup_done;
            let setup_task_for_spawn = setup_task.clone();
            let task = spawn(async move {
                log::info!("[InfiniteScroll] Async task started");
                let window = match web_sys::window() {
                    Some(w) => w,
                    None => {
                        log::error!("[InfiniteScroll] Failed to get window");
                        observer_setup_done_for_reset.set(false);
                        return;
                    }
                };
                let document = match window.document() {
                    Some(d) => d,
                    None => {
                        log::error!("[InfiniteScroll] Failed to get document");
                        observer_setup_done_for_reset.set(false);
                        return;
                    }
                };
                let mut element = None;
                for attempt in 1..=20 {
                    crate::platform::timer::sleep_ms(attempt * 50).await;
                    if let Some(el) = document.get_element_by_id(&id) {
                        log::info!(
                            "[InfiniteScroll] Found sentinel element on attempt {}",
                            attempt
                        );
                        element = Some(el);
                        break;
                    }
                    log::debug!(
                        "[InfiniteScroll] Sentinel not found on attempt {}, retrying...",
                        attempt
                    );
                }
                let element = match element {
                    Some(e) => e,
                    None => {
                        log::warn!(
                            "[InfiniteScroll] Sentinel element never found after 20 attempts: {}",
                            id
                        );
                        observer_setup_done_for_reset.set(false);
                        return;
                    }
                };
                let callback = Closure::wrap(Box::new(move |entries: js_sys::Array| {
                    log::debug!(
                        "[InfiniteScroll] IntersectionObserver callback fired, checking {} entries",
                        entries.length()
                    );
                    for i in 0..entries.length() {
                        if let Ok(entry) = entries
                            .get(i)
                            .dyn_into::<web_sys::IntersectionObserverEntry>()
                        {
                            let is_intersecting = entry.is_intersecting();
                            log::debug!(
                                "[InfiniteScroll] Entry {} intersecting: {}",
                                i,
                                is_intersecting
                            );
                            if is_intersecting {
                                let now = crate::platform::timestamp::now_millis();
                                let last = *last_check_for_callback.peek();
                                log::debug!(
                                    "[InfiniteScroll] Debounce check - now: {}, last: {}, diff: {}",
                                    now,
                                    last,
                                    now - last
                                );
                                if now - last > 1000 {
                                    last_check_for_callback.set(now);
                                    trigger_clone.set(now);
                                    log::info!("[InfiniteScroll] Triggered load more");
                                } else {
                                    log::debug!(
                                            "[InfiniteScroll] Debounce blocked - too soon after last trigger"
                                        );
                                }
                                break;
                            }
                        }
                    }
                }) as Box<dyn FnMut(js_sys::Array)>);
                log::info!("[InfiniteScroll] Creating IntersectionObserver with 300px root margin");
                let options = web_sys::IntersectionObserverInit::new();
                options.set_root_margin("300px");
                let observer = match web_sys::IntersectionObserver::new_with_options(
                    callback.as_ref().unchecked_ref(),
                    &options,
                ) {
                    Ok(obs) => {
                        log::info!("[InfiniteScroll] IntersectionObserver created successfully");
                        obs
                    }
                    Err(e) => {
                        log::error!(
                            "[InfiniteScroll] Failed to create IntersectionObserver: {:?}",
                            e
                        );
                        observer_setup_done_for_reset.set(false);
                        return;
                    }
                };
                observer.observe(&element);
                log::info!(
                    "[InfiniteScroll] IntersectionObserver now watching sentinel element - setup complete"
                );
                *observer_handles_clone.borrow_mut() = Some((observer, callback));
            });
            *setup_task_for_spawn.borrow_mut() = Some(task);
        });
        use_drop(move || {
            if let Some(task) = task_for_drop.borrow_mut().take() {
                task.cancel();
            }
            if let Some((observer, _closure)) = handles_for_drop.borrow_mut().take() {
                observer.disconnect();
            }
        });
    }
    #[cfg(feature = "mobile_platform")]
    {
        let mut polling_generation = use_signal(|| 0u64);
        let mut polling_task: Signal<Option<Task>> = use_signal(|| None);

        use_effect(move || {
            let has_more_value = *has_more.read();

            polling_generation.with_mut(|generation| *generation = generation.wrapping_add(1));
            if let Some(task) = polling_task.write().take() {
                task.cancel();
            }

            if !has_more_value {
                log::debug!("[InfiniteScroll] has_more is false, skipping mobile polling");
                return;
            }

            let generation = *polling_generation.peek();
            let id = id_for_effect.clone();
            let mut trigger_clone = trigger;
            let mut last_check_for_polling = last_check;
            let polling_generation_for_task = polling_generation;

            let task = spawn(async move {
                log::info!(
                    "[InfiniteScroll] Starting mobile polling for sentinel {}",
                    id
                );

                loop {
                    if *polling_generation_for_task.peek() != generation {
                        log::debug!(
                            "[InfiniteScroll] Stopping stale mobile polling task for sentinel {}",
                            id
                        );
                        break;
                    }

                    let id_json = serde_json::to_string(&id).unwrap_or_default();
                    let script = format!(
                        r#"
                        return (() => {{
                            const el = document.getElementById({id});
                            if (!el) return false;
                            const rect = el.getBoundingClientRect();
                            const viewportHeight = window.innerHeight || document.documentElement?.clientHeight || 0;
                            return rect.top <= viewportHeight + 300;
                        }})()
                        "#,
                        id = id_json,
                    );

                    match document::eval(&script).await {
                        Ok(result) => {
                            if result.as_bool().unwrap_or(false) {
                                let now = crate::platform::timestamp::now_millis();
                                let last = *last_check_for_polling.peek();
                                if now - last > 1000 {
                                    last_check_for_polling.set(now);
                                    trigger_clone.set(now);
                                    log::info!(
                                        "[InfiniteScroll] Mobile polling detected sentinel near viewport"
                                    );
                                }
                            }
                        }
                        Err(err) => {
                            log::debug!(
                                "[InfiniteScroll] Mobile polling eval failed for {}: {:?}",
                                id,
                                err
                            );
                        }
                    }

                    crate::platform::timer::sleep_ms(1000).await;
                }
            });

            polling_task.set(Some(task));
        });

        use_drop(move || {
            polling_generation.with_mut(|generation| *generation = generation.wrapping_add(1));
            if let Some(task) = polling_task.write().take() {
                task.cancel();
            }
        });
    }
    sentinel_id
}
