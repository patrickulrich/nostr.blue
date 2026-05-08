use dioxus::prelude::*;

use crate::components::blobbi::companion::companion_state::CompanionState;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::blobbi_visual::BlobbiVisual;
use crate::components::blobbi::visual::status_reaction::{
    use_emotion_override_expiry, resolve_recipe_with_override,
};

#[component]
pub fn CompanionVisual(blobbi: BlobbiCompanion, state: CompanionState) -> Element {
    use_emotion_override_expiry();

    let _recipe = resolve_recipe_with_override(&blobbi);

    let is_egg = matches!(blobbi.stage, crate::components::blobbi::core::types::BlobbiStage::Egg);

    let reaction = crate::components::blobbi::rooms::reaction_state::BLOBBI_REACTION.read();
    let reaction_class = match *reaction {
        crate::components::blobbi::rooms::reaction_state::BlobbiReactionState::Singing => {
            "animate-[blobbi-idle-bounce_0.5s_ease-in-out_infinite]"
        }
        crate::components::blobbi::rooms::reaction_state::BlobbiReactionState::Listening
        | crate::components::blobbi::rooms::reaction_state::BlobbiReactionState::Swaying => {
            "animate-[blobbi-sway_1.2s_ease-in-out_infinite]"
        }
        _ => "",
    };

    let animation = if reaction_class.is_empty() {
        if is_egg {
            match state {
                CompanionState::Sleeping => "animate-[blobbi-sleep-breathe_3s_ease-in-out_infinite]",
                CompanionState::Attention | CompanionState::React => {
                    "animate-[blobbi-sway_1.5s_ease-in-out_infinite]"
                }
                _ => "animate-[blobbi-sway_3s_ease-in-out_infinite]",
            }
        } else {
            match state {
                CompanionState::Sleeping => "animate-[blobbi-sleep-breathe_3s_ease-in-out_infinite]",
                CompanionState::Attention => "animate-[blobbi-idle-bounce_1s_ease-in-out_infinite]",
                CompanionState::Dragging => "",
                CompanionState::Walking | CompanionState::Wander => {
                    "animate-[blobbi-idle-bounce_1.5s_ease-in-out_infinite]"
                }
                CompanionState::Watching => "animate-[blobbi-idle-bounce_2.5s_ease-in-out_infinite]",
                _ => "animate-[blobbi-idle-bounce_2.5s_ease-in-out_infinite]",
            }
        }
    } else if is_egg {
        "animate-[blobbi-sway_1.5s_ease-in-out_infinite]"
    } else {
        reaction_class
    };

    let companion_data = crate::components::blobbi::companion::companion_state::BLOBBI_COMPANION.read();
    let float_y = companion_data.float_y;
    let shadow_offset = (float_y.abs() / 20.0).min(1.0);
    let shadow_opacity = 0.10 * (1.0 - shadow_offset * 0.6);
    let shadow_scale = 1.0 - shadow_offset * 0.3;

    let facing_right = companion_data.facing_right;
    let walk_flip = if state == CompanionState::Walking && !facing_right {
        "scale-x-[-1]"
    } else {
        ""
    };

    let is_singing = matches!(
        *reaction,
        crate::components::blobbi::rooms::reaction_state::BlobbiReactionState::Singing
    );
    let is_listening = matches!(
        *reaction,
        crate::components::blobbi::rooms::reaction_state::BlobbiReactionState::Listening
            | crate::components::blobbi::rooms::reaction_state::BlobbiReactionState::Swaying
    );

    rsx! {
        div { class: "relative {animation}",
            BlobbiVisual {
                blobbi: blobbi.clone(),
                size: Some("64".to_string())
            }

            div {
                class: "absolute -bottom-2 left-1/2 -translate-x-1/2 w-14 h-3 bg-black rounded-full blur-sm transition-all duration-300 {shadow_scale}",
                style: "opacity: {shadow_opacity}; transform: translateX(-50%) scaleX({shadow_scale});",
            }

            if state == CompanionState::Sleeping {
                div { class: "absolute -top-3 -right-2 text-xs text-muted-foreground animate-[float-up_2s_ease-in-out_infinite]",
                    "Z"
                }
                div { class: "absolute -top-5 -right-0 text-[10px] text-muted-foreground animate-[float-up_2s_ease-in-out_infinite_0.5s]",
                    "z"
                }
            }

            if state == CompanionState::Attention {
                div { class: "absolute -top-2 left-1/2 -translate-x-1/2 w-4 h-4 bg-yellow-400 rounded-full flex items-center justify-center text-[8px] font-bold text-black animate-[blobbi-idle-bounce_1s_ease-in-out_infinite]",
                    "!"
                }
            }

            if state == CompanionState::React {
                div { class: "absolute -top-1 left-1/2 -translate-x-1/2 text-xs animate-[blobbi-idle-bounce_0.5s_ease-in-out_infinite]",
                    "✨"
                }
            }

            if state == CompanionState::Walking {
                div { class: "absolute bottom-0 left-1/2 -translate-x-1/2 w-8 h-1 bg-foreground/10 rounded-full blur-sm {walk_flip}" }
            }

            if is_singing {
                div { class: "absolute -top-1 left-1/2 -translate-x-1/2 text-xs animate-[blobbi-idle-bounce_0.5s_ease-in-out_infinite]",
                    "🎤"
                }
            }

            if is_listening {
                div { class: "absolute -top-1 left-1/3 text-xs animate-[float-up_3s_ease-in-out_infinite]",
                    "♪"
                }
                div { class: "absolute -top-2 left-2/3 text-xs animate-[float-up_3.5s_ease-in-out_infinite_0.5s]",
                    "♫"
                }
            }

            if is_egg && is_listening {
                div { class: "absolute inset-0 rounded-full bg-yellow-300/20 animate-[blobbi-sway_1.2s_ease-in-out_infinite]" }
            }

            if is_egg && state == CompanionState::React {
                div { class: "absolute -top-1 left-1/2 -translate-x-1/2 text-[10px] animate-[blobbi-idle-bounce_0.6s_ease-in-out_infinite]",
                    "💧"
                }
            }
        }
    }
}
