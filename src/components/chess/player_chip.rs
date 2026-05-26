use dioxus::prelude::*;
use nostr_sdk::PublicKey;

#[derive(Props, Clone, PartialEq)]
pub struct PlayerChipProps {
    pub pubkey: PublicKey,
    #[props(default)]
    pub label: Option<String>,
    #[props(default)]
    pub color_indicator: Option<String>,
}

#[component]
pub fn PlayerChip(props: PlayerChipProps) -> Element {
    let display_name = props
        .label
        .clone()
        .unwrap_or_else(|| crate::utils::format::truncate_pubkey(&props.pubkey.to_hex()));

    rsx! {
        div { class: "flex items-center gap-2",
            if let Some(color) = props.color_indicator.as_ref() {
                div {
                    class: "w-3 h-3 rounded-full",
                    style: "background-color: {color}",
                }
            }
            span { class: "text-sm text-foreground truncate",
                {display_name}
            }
        }
    }
}
