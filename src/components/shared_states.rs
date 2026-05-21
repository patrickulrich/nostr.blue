use crate::components::LoginModal;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ApiInitializingStateProps {
    pub item_label: &'static str,
}

#[component]
pub fn ApiInitializingState(props: ApiInitializingStateProps) -> Element {
    rsx! {
        div { class: "min-h-[calc(100vh-73px)] flex items-center justify-center p-4",
            div { class: "text-center max-w-md space-y-4",
                div { class: "w-16 h-16 mx-auto rounded-full bg-muted flex items-center justify-center text-2xl",
                    "⏳"
                }
                h2 { class: "font-semibold text-xl", "Preparing {props.item_label}" }
                p { class: "text-muted-foreground",
                    "nostr.blue is still initializing. This usually takes a moment after the app starts."
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ApiAuthRequiredStateProps {
    pub item_label: &'static str,
}

#[component]
pub fn ApiAuthRequiredState(props: ApiAuthRequiredStateProps) -> Element {
    let mut show_login_modal = use_signal(|| false);
    rsx! {
        div { class: "min-h-[calc(100vh-73px)] flex items-center justify-center p-4",
            div { class: "text-center max-w-md",
                div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        rect { x: "3", y: "11", width: "18", height: "10", rx: "2" }
                        path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
                    }
                }
                h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                p { class: "text-muted-foreground mb-6",
                    "Sign in with your Nostr identity to access {props.item_label}."
                }
                button {
                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                    onclick: move |_| show_login_modal.set(true),
                    "Sign In"
                }
            }
        }
        if *show_login_modal.read() {
            LoginModal {
                on_close: move |_| show_login_modal.set(false),
            }
        }
    }
}
