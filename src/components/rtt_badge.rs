use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct RttBadgeProps {
    pub label: String,
    pub ms: Option<u64>,
}

fn rtt_color_class(ms: u64) -> &'static str {
    if ms < 200 {
        "bg-green-500/20 text-green-400 border-green-500/30"
    } else if ms < 500 {
        "bg-amber-500/20 text-amber-400 border-amber-500/30"
    } else {
        "bg-red-500/20 text-red-400 border-red-500/30"
    }
}

#[component]
pub fn RttBadge(props: RttBadgeProps) -> Element {
    match props.ms {
        Some(ms) => rsx! {
            span {
                class: "inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-mono border {rtt_color_class(ms)}",
                span { class: "text-muted-foreground", "{props.label}" }
                "{ms}ms"
            }
        },
        None => rsx! {
            span {
                class: "inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-mono border bg-muted/50 text-muted-foreground border-border",
                span { "{props.label}" }
                "—"
            }
        },
    }
}
