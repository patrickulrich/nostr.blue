use crate::components::icons::{EyeIcon, EyeOffIcon};
use crate::stores::ui::settings_store;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SensitiveContentProps {
    pub reason: Option<String>,
    pub children: Element,
}

#[component]
pub fn SensitiveContent(props: SensitiveContentProps) -> Element {
    let mut revealed = use_signal(|| false);
    let show_all = settings_store::SETTINGS.read().show_sensitive_content;

    if show_all {
        return rsx! { {props.children} };
    }

    rsx! {
        div { class: "relative overflow-hidden rounded-lg",
            div {
                filter: if !revealed() { "blur(20px)" } else { "none" },
                class: "transition-[filter] duration-300",
                {props.children}
            }
            if !revealed() {
                div {
                    class: "absolute inset-0 bg-black/40 flex flex-col items-center justify-center gap-2 cursor-pointer z-10",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        revealed.set(true);
                    },
                    EyeOffIcon { class: "w-6 h-6 text-white".to_string() }
                    p { class: "text-white text-sm font-medium",
                        if let Some(ref reason) = props.reason {
                            "{reason}"
                        } else {
                            "Sensitive Content"
                        }
                    }
                    span { class: "text-white/70 text-xs", "Click to reveal" }
                }
            }
            if revealed() {
                button {
                    class: "absolute bottom-1 right-1 text-xs text-muted-foreground hover:text-foreground px-2 py-0.5 bg-background/80 rounded z-10",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        revealed.set(false);
                    },
                    EyeIcon { class: "w-3 h-3 inline".to_string() }
                    " Hide"
                }
            }
        }
    }
}
