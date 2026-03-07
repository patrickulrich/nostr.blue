//! Keyboard shortcuts component for /code section
//!
//! Provides GitHub-style keyboard navigation:
//! - `g` then `i` → Global Issues
//! - `g` then `p` → Global Pull Requests
//! - `g` then `r` → Repositories
//! - `g` then `s` → Snippets
//! - `g` then `h` → Code Home
//! - `?` → Show help modal

#[allow(unused_imports)]
use crate::routes::Route;
use dioxus::prelude::*;
use dioxus_core::use_drop;
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

#[component]
pub fn CodeKeyboardShortcuts() -> Element {
    #[allow(unused_variables, unused_mut)]
    let mut pending_g = use_signal(|| false);
    let mut show_help = use_signal(|| false);
    #[allow(unused_variables)]
    let nav = navigator();

    // Store JS function reference for event listener cleanup on unmount
    #[cfg(feature = "web")]
    let mut cleanup_fn = use_signal(|| None::<js_sys::Function>);
    // Store timeout ID for cleanup on unmount
    #[cfg(feature = "web")]
    let mut timeout_id = use_signal(|| None::<i32>);

    // Set up keyboard event listener once
    #[cfg(feature = "web")]
    use_effect(move || {
        {
            let Some(window) = web_sys::window() else { return; };

            let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                // Check if target is input/textarea/select - skip shortcuts if so
                if let Some(target) = event.target() {
                    if let Some(element) = target.dyn_ref::<web_sys::HtmlElement>() {
                        let tag = element.tag_name().to_lowercase();
                        if tag == "input" || tag == "textarea" || tag == "select" {
                            return;
                        }
                        // Also check contenteditable
                        if element.is_content_editable() {
                            return;
                        }
                    }
                }

                let key = event.key();

                // Skip shortcuts when modifier keys are held
                if event.ctrl_key() || event.meta_key() || event.alt_key() {
                    return;
                }

                // Handle `?` for help
                if key == "?" {
                    event.prevent_default();
                    let current = *show_help.read();
                    show_help.set(!current);
                    pending_g.set(false);
                    let tid = *timeout_id.peek();
                    if let Some(id) = tid {
                        if let Some(w) = web_sys::window() {
                            w.clear_timeout_with_handle(id);
                        }
                        timeout_id.set(None);
                    }
                    return;
                }

                // Handle Escape to close help
                if key == "Escape" && *show_help.read() {
                    event.prevent_default();
                    show_help.set(false);
                    pending_g.set(false);
                    let tid = *timeout_id.peek();
                    if let Some(id) = tid {
                        if let Some(w) = web_sys::window() {
                            w.clear_timeout_with_handle(id);
                        }
                        timeout_id.set(None);
                    }
                    return;
                }

                // Handle `g` prefix
                if key == "g" && !*pending_g.read() {
                    event.prevent_default();
                    pending_g.set(true);

                    // Reset after 1 second timeout, storing the ID for cleanup
                    let Some(window_clone) = web_sys::window() else { return; };
                    let callback = Closure::wrap(Box::new(move || {
                        pending_g.set(false);
                        timeout_id.set(None);
                    }) as Box<dyn FnMut()>);
                    if let Ok(id) = window_clone.set_timeout_with_callback_and_timeout_and_arguments_0(
                        callback.into_js_value().as_ref().unchecked_ref(),
                        1000
                    ) {
                        timeout_id.set(Some(id));
                    }
                    return;
                }

                // Handle second key after `g`
                if *pending_g.read() {
                    event.prevent_default();
                    pending_g.set(false);
                    // Clear the pending timeout to prevent stale firing
                    let pending_timeout = *timeout_id.peek();
                    if let Some(id) = pending_timeout {
                        if let Some(w) = web_sys::window() {
                            w.clear_timeout_with_handle(id);
                        }
                        timeout_id.set(None);
                    }

                    match key.as_str() {
                        "i" => {
                            nav.push(Route::CodeGlobalIssues {});
                        }
                        "p" => {
                            nav.push(Route::CodeGlobalPulls {});
                        }
                        "r" => {
                            nav.push(Route::CodeRepositories {});
                        }
                        "s" => {
                            nav.push(Route::CodeSnippets {});
                        }
                        "h" => {
                            nav.push(Route::CodeHome {});
                        }
                        _ => {}
                    }
                }
            }) as Box<dyn FnMut(_)>);

            let js_fn: js_sys::Function = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
            window.add_event_listener_with_callback("keydown", &js_fn).ok();
            cleanup_fn.set(Some(js_fn));
            closure.forget();
        }
    });

    #[cfg(feature = "web")]
    use_drop(move || {
        {
            if let Some(window) = web_sys::window() {
                if let Some(js_fn) = cleanup_fn.peek().as_ref() {
                    let _ = window.remove_event_listener_with_callback("keydown", js_fn);
                }
                if let Some(id) = *timeout_id.peek() {
                    window.clear_timeout_with_handle(id);
                }
            }
        }
    });

    // Render help modal if visible
    rsx! {
        if *show_help.read() {
            div {
                class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
                onclick: move |_| show_help.set(false),

                div {
                    class: "fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 bg-background border border-border rounded-lg p-6 max-w-lg w-full mx-4 shadow-xl z-50",
                    role: "dialog",
                    aria_modal: "true",
                    aria_labelledby: "keyboard-shortcuts-title",
                    onclick: move |e| e.stop_propagation(),

                    h2 { id: "keyboard-shortcuts-title", class: "text-lg font-bold mb-4", "Keyboard Shortcuts" }

                    div { class: "space-y-3",
                        ShortcutRow { keys: "g then i", description: "Go to Global Issues" }
                        ShortcutRow { keys: "g then p", description: "Go to Global Pull Requests" }
                        ShortcutRow { keys: "g then r", description: "Go to Repositories" }
                        ShortcutRow { keys: "g then s", description: "Go to Snippets" }
                        ShortcutRow { keys: "g then h", description: "Go to Code Home" }
                        ShortcutRow { keys: "?", description: "Show this help" }
                        ShortcutRow { keys: "Esc", description: "Close this help" }
                    }

                    div { class: "mt-6 pt-4 border-t border-border text-sm text-muted-foreground text-center",
                        "Press Esc or click outside to close"
                    }
                }
            }
        }
    }
}

#[component]
fn ShortcutRow(keys: &'static str, description: &'static str) -> Element {
    rsx! {
        div { class: "flex items-center justify-between py-2",
            div { class: "flex items-center gap-2",
                span { class: "font-mono text-xs bg-muted px-2 py-1 rounded border border-border",
                    "{keys}"
                }
            }
            span { class: "text-sm text-muted-foreground",
                "{description}"
            }
        }
    }
}
