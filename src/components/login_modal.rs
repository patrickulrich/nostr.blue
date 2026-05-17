use crate::routes::home::login::LoginSection;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct LoginModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn LoginModal(props: LoginModalProps) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "bg-card rounded-lg shadow-xl max-w-lg w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "sticky top-0 bg-card border-b border-border px-6 py-4 flex items-center justify-between",
                    h3 { class: "text-xl font-bold text-foreground", "Sign In" }
                    button {
                        class: "text-muted-foreground hover:text-foreground text-2xl",
                        onclick: move |_| props.on_close.call(()),
                        "×"
                    }
                }
                LoginSection {}
            }
        }
    }
}
