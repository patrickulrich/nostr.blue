use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct DraftDiscardModalProps {
    pub on_save: EventHandler<()>,
    pub on_discard: EventHandler<()>,
    pub on_continue: EventHandler<()>,
}

#[component]
pub fn DraftDiscardModal(props: DraftDiscardModalProps) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-[60] flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| props.on_continue.call(()),
            div {
                class: "bg-card border border-border rounded-lg shadow-xl max-w-sm w-full mx-4 p-6",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "text-lg font-semibold mb-2", "Unsaved Content" }
                p { class: "text-sm text-muted-foreground mb-6",
                    "You have content that will be lost. What would you like to do?"
                }
                div { class: "flex flex-col gap-2",
                    button {
                        class: "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition",
                        onclick: move |_| props.on_save.call(()),
                        "Save Draft"
                    }
                    button {
                        class: "w-full px-4 py-2 border border-border rounded-lg font-medium hover:bg-accent transition",
                        onclick: move |_| props.on_discard.call(()),
                        "Discard"
                    }
                    button {
                        class: "w-full px-4 py-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition",
                        onclick: move |_| props.on_continue.call(()),
                        "Continue Editing"
                    }
                }
            }
        }
    }
}
