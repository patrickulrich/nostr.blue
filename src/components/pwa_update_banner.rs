//! PWA Update Banner
//! Shows a notification when a new service worker version is available

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

/// Guard struct that removes the event listener on drop
#[derive(Clone)]
struct SwUpdateGuard {
    callback: Signal<Option<wasm_bindgen::closure::Closure<dyn FnMut()>>>,
}

impl Drop for SwUpdateGuard {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.write().take() {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "sw-update-available",
                    callback.as_ref().unchecked_ref(),
                );
            }
        }
    }
}

/// PWA update banner that listens for service worker updates
#[component]
pub fn PwaUpdateBanner() -> Element {
    let mut update_available = use_signal(|| false);
    let mut sw_callback = use_signal(|| None::<wasm_bindgen::closure::Closure<dyn FnMut()>>);

    // Set up event listener for SW updates
    use_effect(move || {
        let Some(window) = web_sys::window() else {
            return;
        };

        // Create callback that sets update_available to true
        let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            log::info!("[PWA] Update available - showing banner");
            update_available.set(true);
        }) as Box<dyn FnMut()>);

        // Register the event listener
        window
            .add_event_listener_with_callback(
                "sw-update-available",
                callback.as_ref().unchecked_ref(),
            )
            .ok();

        // Store callback for cleanup
        sw_callback.set(Some(callback));
    });

    // Cleanup on unmount
    use_hook(move || SwUpdateGuard {
        callback: sw_callback,
    });

    // Handle refresh click
    let handle_refresh = move |_| {
        if let Some(window) = web_sys::window() {
            // Reload the page to activate the new service worker
            let _ = window.location().reload();
        }
    };

    // Handle dismiss click
    let handle_dismiss = move |_| {
        update_available.set(false);
    };

    if !*update_available.read() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "fixed bottom-20 left-1/2 -translate-x-1/2 z-50 px-4 py-3 bg-primary text-primary-foreground rounded-lg shadow-lg flex items-center gap-3 animate-in slide-in-from-bottom-4 duration-300",

            // Icon
            svg {
                class: "w-5 h-5 shrink-0",
                xmlns: "http://www.w3.org/2000/svg",
                fill: "none",
                view_box: "0 0 24 24",
                stroke: "currentColor",
                stroke_width: "2",
                path {
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                }
            }

            // Message
            span {
                class: "text-sm font-medium",
                "A new version is available"
            }

            // Refresh button
            button {
                class: "px-3 py-1 bg-primary-foreground text-primary text-sm font-medium rounded hover:opacity-90 transition",
                onclick: handle_refresh,
                "Refresh"
            }

            // Dismiss button
            button {
                class: "p-1 hover:opacity-70 transition",
                onclick: handle_dismiss,
                title: "Dismiss",
                svg {
                    class: "w-4 h-4",
                    xmlns: "http://www.w3.org/2000/svg",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M6 18L18 6M6 6l12 12"
                    }
                }
            }
        }
    }
}
