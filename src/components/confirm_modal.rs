use dioxus::prelude::*;

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
                aria_labelledby: "modal-title",
                aria_describedby: "modal-message",
                onclick: move |e| e.stop_propagation(),

                // Title
                h2 {
                    class: "text-lg font-bold mb-2",
                    id: "modal-title",
                    "{title}"
                }

                // Message
                p {
                    class: "text-muted-foreground mb-6",
                    id: "modal-message",
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
