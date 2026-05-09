use dioxus::prelude::*;

#[component]
pub fn RoomActionButton(
    icon: Element,
    label: String,
    color: String,
    glow_hex: String,
    onclick: EventHandler<()>,
    disabled: Option<bool>,
    loading: Option<bool>,
    badge: Option<Element>,
) -> Element {
    let disabled = disabled.unwrap_or(false);
    let loading = loading.unwrap_or(false);

    rsx! {
        button {
            class: if disabled {
                "flex flex-col items-center gap-1 transition-all duration-300 ease-out shrink-0 opacity-50 pointer-events-none"
            } else {
                "flex flex-col items-center gap-1 transition-all duration-300 ease-out shrink-0 hover:-translate-y-1 hover:scale-110 active:scale-95"
            },
            onclick: move |_| onclick.call(()),

            div { class: "relative",
                div {
                    class: "size-14 sm:size-20 rounded-full flex items-center justify-center {color}",
                    style: "background: radial-gradient(circle at 40% 35%, color-mix(in srgb, {glow_hex} 14%, transparent), color-mix(in srgb, {glow_hex} 4%, transparent) 70%)",
                    if loading {
                        div { class: "size-7 sm:size-9 border-2 border-current border-t-transparent rounded-full animate-spin" }
                    } else {
                        {icon}
                    }
                }
                if let Some(badge_content) = badge {
                    div { class: "absolute -top-0.5 -right-0.5",
                        {badge_content}
                    }
                }
            }
            span { class: "text-[10px] sm:text-xs font-medium text-muted-foreground",
                "{label}"
            }
        }
    }
}
