use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;
use std::cell::Cell;
thread_local! {
    static MODAL_ID_COUNTER: Cell<u32> = const { Cell::new(0) };
}
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
    let modal_id = use_signal(|| {
        MODAL_ID_COUNTER
            .with(|c| {
                let id = c.get();
                c.set(id.wrapping_add(1));
                id
            })
    });
    let title_id = format!("modal-title-{}", modal_id());
    let message_id = format!("modal-message-{}", modal_id());
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "bg-card border border-border rounded-xl max-w-sm w-full p-6 shadow-xl",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "{title_id}",
                aria_describedby: "{message_id}",
                tabindex: "-1",
                onmounted: move |_evt| {
                    #[cfg(feature = "web")]
                    {
                        if let Some(html_element) = _evt.data().downcast::<web_sys::HtmlElement>() {
                            let _ = html_element.focus();
                        }
                    }
                },
                onkeydown: move |evt: KeyboardEvent| {
                    if evt.key() == Key::Escape {
                        evt.stop_propagation();
                        on_cancel.call(());
                    }
                },
                onclick: move |e| e.stop_propagation(),
                h2 { class: "text-lg font-bold mb-2", id: "{title_id}", "{title}" }
                p {
                    class: "text-muted-foreground mb-6 whitespace-pre-line",
                    id: "{message_id}",
                    "{message}"
                }
                div { class: "flex gap-3 justify-end",
                    button {
                        class: "px-4 py-2 rounded-lg hover:bg-accent transition",
                        onclick: move |_| on_cancel.call(()),
                        {cancel_text.clone().unwrap_or_else(|| "Cancel".to_string())}
                    }
                    button {
                        class: "px-4 py-2 bg-destructive text-destructive-foreground rounded-lg hover:bg-destructive/90 transition",
                        onclick: move |_| on_confirm.call(()),
                        {confirm_text.clone().unwrap_or_else(|| "Confirm".to_string())}
                    }
                }
            }
        }
    }
}
