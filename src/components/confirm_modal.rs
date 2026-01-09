use dioxus::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global counter for generating unique modal IDs
static MODAL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Reusable confirmation modal component
#[component]
pub fn ConfirmModal(
    title: String,
    message: String,
    confirm_text: Option<String>,
    cancel_text: Option<String>,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    // Generate unique ID suffix for this modal instance
    let modal_id = use_signal(|| MODAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed));

    let title_id = format!("modal-title-{}", modal_id());
    let message_id = format!("modal-message-{}", modal_id());

    rsx! {
        // Modal overlay - clicking outside cancels
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_cancel.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-xl max-w-sm w-full p-6 shadow-xl",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "{title_id}",
                aria_describedby: "{message_id}",
                onclick: move |e| e.stop_propagation(),

                // Title
                h2 {
                    class: "text-lg font-bold mb-2",
                    id: "{title_id}",
                    "{title}"
                }

                // Message
                p {
                    class: "text-muted-foreground mb-6 whitespace-pre-line",
                    id: "{message_id}",
                    "{message}"
                }

                // Buttons
                div {
                    class: "flex gap-3 justify-end",

                    // Cancel button
                    button {
                        class: "px-4 py-2 rounded-lg hover:bg-accent transition",
                        onclick: move |_| on_cancel.call(()),
                        { cancel_text.clone().unwrap_or_else(|| "Cancel".to_string()) }
                    }

                    // Confirm button (destructive style)
                    button {
                        class: "px-4 py-2 bg-destructive text-destructive-foreground rounded-lg hover:bg-destructive/90 transition",
                        onclick: move |_| on_confirm.call(()),
                        { confirm_text.clone().unwrap_or_else(|| "Confirm".to_string()) }
                    }
                }
            }
        }
    }
}
