use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::recipe::*;

#[component]
pub fn BabyVisual(blobbi: BlobbiCompanion, recipe: VisualRecipe) -> Element {
    let base_color = &blobbi.visual_traits.base_color;
    let eye_color = &blobbi.visual_traits.eye_color;
    let animation_class = match recipe.animation {
        AnimationType::Excited => "animate-[blobbi-idle-bounce_1.5s_ease-in-out_infinite]",
        AnimationType::Sad => "animate-[blobbi-sad-breathe_3s_ease-in-out_infinite]",
        _ => "animate-[blobbi-idle-bounce_2.5s_ease-in-out_infinite]",
    };

    let (left_eye, right_eye) = render_eyes(recipe.eye_type, eye_color);
    let mouth = render_mouth(recipe.mouth_type);
    let effect = render_body_effect(recipe.body_effect);

    rsx! {
        svg {
            class: "{animation_class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 140 140",
            width: "140",
            height: "140",
            defs {
                radialGradient { id: "baby-body-grad", cx: "40%", cy: "35%", r: "65%",
                    stop { offset: "0%", stop_color: "{lighten(base_color, 20)}" }
                    stop { offset: "100%", stop_color: "{base_color}" }
                }
                radialGradient { id: "baby-shine", cx: "35%", cy: "30%", r: "30%",
                    stop { offset: "0%", stop_color: "rgba(255,255,255,0.3)" }
                    stop { offset: "100%", stop_color: "rgba(255,255,255,0)" }
                }
            }
            ellipse {
                cx: "70",
                cy: "80",
                rx: "52",
                ry: "45",
                fill: "url(#baby-body-grad)",
            }
            ellipse {
                cx: "70",
                cy: "80",
                rx: "52",
                ry: "45",
                fill: "url(#baby-shine)",
            }
            ellipse {
                cx: "50",
                cy: "60",
                rx: "14",
                ry: "16",
                fill: "white",
            }
            ellipse {
                cx: "90",
                cy: "60",
                rx: "14",
                ry: "16",
                fill: "white",
            }
            {left_eye}
            {right_eye}
            if matches!(recipe.eye_type, EyeType::Love) {
                path {
                    d: "M 47 55 C 47 50 50 47 53 50 C 50 47 47 44 47 55",
                    fill: "#ef4444",
                }
                path {
                    d: "M 87 55 C 87 50 90 47 93 50 C 90 47 87 44 87 55",
                    fill: "#ef4444",
                }
            }
            {mouth}
            ellipse {
                cx: "42",
                cy: "75",
                rx: "8",
                ry: "5",
                fill: "rgba(255,150,150,0.3)",
            }
            ellipse {
                cx: "98",
                cy: "75",
                rx: "8",
                ry: "5",
                fill: "rgba(255,150,150,0.3)",
            }
            {effect}
        }
    }
}

fn render_eyes(eye_type: EyeType, eye_color: &str) -> (Element, Element) {
    match eye_type {
        EyeType::Happy => {
            let left = rsx! {
                path {
                    d: "M 43 60 Q 50 54 57 60",
                    fill: "none",
                    stroke: "{eye_color}",
                    stroke_width: "2.5",
                    stroke_linecap: "round",
                }
            };
            let right = rsx! {
                path {
                    d: "M 83 60 Q 90 54 97 60",
                    fill: "none",
                    stroke: "{eye_color}",
                    stroke_width: "2.5",
                    stroke_linecap: "round",
                }
            };
            (left, right)
        }
        EyeType::Sad => {
            let left = rsx! {
                circle { cx: "50", cy: "62", r: "5", fill: "{eye_color}" }
                line { x1: "43", y1: "53", x2: "56", y2: "56", stroke: "{eye_color}", stroke_width: "2", stroke_linecap: "round" }
            };
            let right = rsx! {
                circle { cx: "90", cy: "62", r: "5", fill: "{eye_color}" }
                line { x1: "84", y1: "56", x2: "97", y2: "53", stroke: "{eye_color}", stroke_width: "2", stroke_linecap: "round" }
            };
            (left, right)
        }
        EyeType::Tired => {
            let left = rsx! {
                ellipse { cx: "50", cy: "61", rx: "6", ry: "4", fill: "{eye_color}" }
                line { x1: "43", y1: "56", x2: "57", y2: "56", stroke: "{eye_color}", stroke_width: "1.5" }
            };
            let right = rsx! {
                ellipse { cx: "90", cy: "61", rx: "6", ry: "4", fill: "{eye_color}" }
                line { x1: "83", y1: "56", x2: "97", y2: "56", stroke: "{eye_color}", stroke_width: "1.5" }
            };
            (left, right)
        }
        EyeType::Hungry => {
            let left = rsx! {
                ellipse { cx: "50", cy: "60", rx: "7", ry: "9", fill: "{eye_color}" }
                ellipse { cx: "50", cy: "60", rx: "7", ry: "9", fill: "none", stroke: "rgba(0,0,0,0.1)", stroke_width: "0.5" }
            };
            let right = rsx! {
                ellipse { cx: "90", cy: "60", rx: "7", ry: "9", fill: "{eye_color}" }
                ellipse { cx: "90", cy: "60", rx: "7", ry: "9", fill: "none", stroke: "rgba(0,0,0,0.1)", stroke_width: "0.5" }
            };
            (left, right)
        }
        EyeType::Sleeping => {
            let left = rsx! {
                path {
                    d: "M 43 61 Q 50 58 57 61",
                    fill: "none",
                    stroke: "{eye_color}",
                    stroke_width: "2",
                    stroke_linecap: "round",
                }
            };
            let right = rsx! {
                path {
                    d: "M 83 61 Q 90 58 97 61",
                    fill: "none",
                    stroke: "{eye_color}",
                    stroke_width: "2",
                    stroke_linecap: "round",
                }
            };
            (left, right)
        }
        EyeType::Excited => {
            let left = rsx! {
                g {
                    circle { cx: "50", cy: "60", r: "6", fill: "{eye_color}" }
                    circle { cx: "48", cy: "58", r: "2", fill: "white" }
                    path { d: "M 44 70 Q 50 73 56 70", fill: "none", stroke: "{eye_color}", stroke_width: "1.5" }
                }
            };
            let right = rsx! {
                g {
                    circle { cx: "90", cy: "60", r: "6", fill: "{eye_color}" }
                    circle { cx: "88", cy: "58", r: "2", fill: "white" }
                    path { d: "M 84 70 Q 90 73 96 70", fill: "none", stroke: "{eye_color}", stroke_width: "1.5" }
                }
            };
            (left, right)
        }
        _ => {
            let left = rsx! {
                circle { cx: "50", cy: "60", r: "6", fill: "{eye_color}" }
            };
            let right = rsx! {
                circle { cx: "90", cy: "60", r: "6", fill: "{eye_color}" }
            };
            (left, right)
        }
    }
}

fn render_mouth(mouth_type: MouthType) -> Element {
    match mouth_type {
        MouthType::Smile => rsx! {
            path {
                d: "M 58 85 Q 70 95 82 85",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                class: "text-foreground/60",
            }
        },
        MouthType::Grin => rsx! {
            path {
                d: "M 55 83 Q 70 98 85 83",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                class: "text-foreground/60",
            }
        },
        MouthType::Frown => rsx! {
            path {
                d: "M 58 90 Q 70 82 82 90",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                class: "text-foreground/60",
            }
        },
        MouthType::Open => rsx! {
            ellipse {
                cx: "70",
                cy: "88",
                rx: "8",
                ry: "10",
                fill: "currentColor",
                class: "text-foreground/40",
            }
        },
        MouthType::Sleeping => rsx! {
            text {
                x: "85",
                y: "50",
                font_size: "12",
                fill: "currentColor",
                class: "text-muted-foreground",
                "z",
            }
        },
        MouthType::Neutral => rsx! {
            line {
                x1: "60",
                y1: "87",
                x2: "80",
                y2: "87",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                class: "text-foreground/60",
            }
        },
    }
}

fn render_body_effect(effect: BodyEffect) -> Element {
    match effect {
        BodyEffect::Dirt => rsx! {
            g {
                circle { cx: "45", cy: "90", r: "3", fill: "rgba(139,92,42,0.4)" }
                circle { cx: "80", cy: "85", r: "2.5", fill: "rgba(139,92,42,0.3)" }
                circle { cx: "60", cy: "100", r: "2", fill: "rgba(139,92,42,0.35)" }
            }
        },
        BodyEffect::Stink => rsx! {
            g {
                path { d: "M 30 70 Q 25 65 28 60", fill: "none", stroke: "rgba(139,92,42,0.4)", stroke_width: "1.5" }
                path { d: "M 35 65 Q 30 58 34 52", fill: "none", stroke: "rgba(139,92,42,0.3)", stroke_width: "1.5" }
            }
        },
        BodyEffect::Sparkle => rsx! {
            g {
                text { x: "25", y: "55", font_size: "10", "✨" }
                text { x: "100", y: "45", font_size: "8", "✨" }
            }
        },
        BodyEffect::Sleeping => rsx! {
            g {
                text { x: "95", y: "40", font_size: "10", fill: "currentColor", class: "text-muted-foreground animate-[float-up_2s_ease-in-out_infinite]", "Z" }
                text { x: "105", y: "30", font_size: "8", fill: "currentColor", class: "text-muted-foreground animate-[float-up_2s_ease-in-out_infinite_0.5s]", "z" }
            }
        },
        _ => rsx! { g {} },
    }
}

fn lighten(hex: &str, percent: u32) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return "#ffffff".to_string();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    let f = percent as f32 / 100.0;
    let r = ((r as f32 + (255.0 - r as f32) * f).min(255.0)) as u8;
    let g = ((g as f32 + (255.0 - g as f32) * f).min(255.0)) as u8;
    let b = ((b as f32 + (255.0 - b as f32) * f).min(255.0)) as u8;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}
