use dioxus::prelude::*;

use crate::components::blobbi::companion::companion_state::CompanionState;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::utils::nip_bb::BlobbiStage;

#[component]
pub fn CompanionVisual(blobbi: BlobbiCompanion, state: CompanionState) -> Element {
    let size = 64.0_f32;
    let animation = match state {
        CompanionState::Sleeping => "animate-[blobbi-sleep-breathe_3s_ease-in-out_infinite]",
        CompanionState::Attention => "animate-[blobbi-idle-bounce_1s_ease-in-out_infinite]",
        CompanionState::Dragging => "",
        _ => "animate-[blobbi-idle-bounce_2.5s_ease-in-out_infinite]",
    };

    let base_color = &blobbi.visual_traits.base_color;

    rsx! {
        div {
            class: "relative {animation}",

            match blobbi.stage {
                BlobbiStage::Egg => rsx! {
                    div {
                        class: "rounded-full shadow-lg",
                        style: "width: {size}px; height: {size * 0.8}px; background: {base_color};",
                        div {
                            class: "w-full h-full rounded-full",
                            style: "background: radial-gradient(circle at 35% 35%, rgba(255,255,255,0.3), transparent 60%);",
                        }
                    }
                },
                _ => rsx! {
                    div {
                        class: "rounded-full shadow-lg flex items-center justify-center",
                        style: "width: {size}px; height: {size}px; background: {base_color};",
                        div {
                            class: "flex gap-0.5",
                            if state == CompanionState::Sleeping {
                                span { class: "text-[10px]", "💤" }
                            } else {
                                span { class: "text-sm",
                                    if blobbi.stats.happiness > 70.0 { "😊" }
                                    else if blobbi.stats.happiness > 40.0 { "😐" }
                                    else { "😢" }
                                }
                            }
                        }
                    }
                },
            }

            if state == CompanionState::Sleeping {
                div { class: "absolute -top-2 -right-1 text-xs animate-[float-up_2s_ease-in-out_infinite]",
                    "Z"
                }
            }

            if blobbi.is_egg() {
                div { class: "absolute -bottom-1 -right-1 w-4 h-4 bg-blue-500 rounded-full flex items-center justify-center text-[8px] text-white",
                    "🥚"
                }
            }
        }
    }
}
