//! Keyboard shortcuts component for /code section
//!
//! Provides GitHub-style keyboard navigation:
//! - `g` then `i` → Global Issues
//! - `g` then `p` → Global Pull Requests
//! - `g` then `r` → Repositories
//! - `g` then `s` → Snippets
//! - `g` then `h` → Code Home
//! - `t` → Open file finder
//! - `?` → Show help modal

#[allow(unused_imports)]
use crate::routes::Route;
use dioxus::prelude::*;
use dioxus_core::use_drop;

#[component]
pub fn CodeKeyboardShortcuts() -> Element {
    #[allow(unused_variables, unused_mut)]
    let mut pending_g = use_signal(|| false);
    let mut show_help = use_signal(|| false);
    #[allow(unused_variables)]
    let nav = navigator();

    // Store cleanup function for event listener removal
    #[allow(unused_variables, unused_mut)]
    let mut cleanup_fn = use_signal(|| None::<(js_sys::Function, web_sys::Window)>);

    // Set up keyboard event listener once
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;

            let window = web_sys::window().expect("no global window");

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

                // Handle `?` for help
                if key == "?" {
                    event.prevent_default();
                    let current = *show_help.read();
                    show_help.set(!current);
                    return;
                }

                // Handle Escape to close help
                if key == "Escape" && *show_help.read() {
                    event.prevent_default();
                    show_help.set(false);
                    return;
                }

                // Handle `g` prefix
                if key == "g" && !*pending_g.read() {
                    event.prevent_default();
                    pending_g.set(true);

                    // Reset after 1 second timeout
                    let window_clone = web_sys::window().unwrap();
                    let callback = Closure::once(move || {
                        pending_g.set(false);
                    });
                    let _ = window_clone.set_timeout_with_callback_and_timeout_and_arguments_0(
                        callback.as_ref().unchecked_ref(),
                        1000
                    );
                    callback.forget();
                    return;
                }

                // Handle second key after `g`
                if *pending_g.read() {
                    event.prevent_default();
                    pending_g.set(false);

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
            cleanup_fn.set(Some((js_fn, window.clone())));
            closure.forget();
        }
    });

    use_drop(move || {
        #[cfg(target_arch = "wasm32")]
        if let Some((func, win)) = cleanup_fn.peek().as_ref() {
            win.remove_event_listener_with_callback("keydown", func).ok();
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
                    onclick: move |e| e.stop_propagation(),

                    h2 { class: "text-lg font-bold mb-4", "Keyboard Shortcuts" }

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
                        "Press any key combination or click outside to close"
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
