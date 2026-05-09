use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::recipe::*;

#[component]
pub fn BabyVisual(blobbi: BlobbiCompanion, recipe: ComposableRecipe) -> Element {
    let base_color = &blobbi.visual_traits.base_color;
    let eye_color = &blobbi.visual_traits.eye_color;
    let animation_class = match recipe.animation {
        AnimationType::Excited => "animate-[blobbi-idle-bounce_1.5s_ease-in-out_infinite]",
        AnimationType::Sad => "animate-[blobbi-sad-breathe_3s_ease-in-out_infinite]",
        _ => "animate-[blobbi-idle-bounce_2.5s_ease-in-out_infinite]",
    };

    let use_gaze = !blobbi.is_sleeping()
        && matches!(
            recipe.eye_type,
            EyeType::Happy | EyeType::Excited | EyeType::Content | EyeType::Curious | EyeType::Bored
        );

    let (left_eye, right_eye) = render_eyes(recipe.eye_type, eye_color, use_gaze);
    let mouth = render_mouth(recipe.mouth_type);
    let effect = render_body_effects(&recipe.body_effects);
    let eyebrow_el = render_eyebrow(&recipe.eyebrow, 50.0, 90.0, 44.0);
    let extras_el = render_extras(&recipe.extras, recipe.eye_type);

    let eyelid_color = darken_hex(base_color, 30);

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
                if use_gaze {
                    clipPath { id: "baby-blink-clip-left",
                        rect {
                            class: "blobbi-blink-clip-rect",
                            x: "36", y: "44",
                            width: "28", height: "32",
                            "data-clip-top": "44",
                            "data-clip-height": "32",
                        }
                    }
                    clipPath { id: "baby-blink-clip-right",
                        rect {
                            class: "blobbi-blink-clip-rect",
                            x: "76", y: "44",
                            width: "28", height: "32",
                            "data-clip-top": "44",
                            "data-clip-height": "32",
                        }
                    }
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
            if use_gaze {
                ellipse {
                    class: "blobbi-eyelid",
                    cx: "50", cy: "60",
                    rx: "14", ry: "16",
                    fill: "{eyelid_color}",
                    opacity: "0",
                }
                ellipse {
                    class: "blobbi-eyelid",
                    cx: "90", cy: "60",
                    rx: "14", ry: "16",
                    fill: "{eyelid_color}",
                    opacity: "0",
                }
            }
            g {
                "clip-path": if use_gaze { "url(#baby-blink-clip-left)" } else { "" },
                ellipse {
                    cx: "50",
                    cy: "60",
                    rx: "14",
                    ry: "16",
                    fill: "white",
                }
                {left_eye}
            }
            g {
                "clip-path": if use_gaze { "url(#baby-blink-clip-right)" } else { "" },
                ellipse {
                    cx: "90",
                    cy: "60",
                    rx: "14",
                    ry: "16",
                    fill: "white",
                }
                {right_eye}
            }
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
            {eyebrow_el}
            {extras_el}
        }
    }
}

fn render_eyes(eye_type: EyeType, eye_color: &str, use_gaze: bool) -> (Element, Element) {
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
        EyeType::Watery => {
            let left = rsx! {
                g {
                    circle { cx: "50", cy: "60", r: "6", fill: "{eye_color}" }
                    circle { cx: "50", cy: "62", r: "6", fill: "rgba(100,180,255,0.25)" }
                }
            };
            let right = rsx! {
                g {
                    circle { cx: "90", cy: "60", r: "6", fill: "{eye_color}" }
                    circle { cx: "90", cy: "62", r: "6", fill: "rgba(100,180,255,0.25)" }
                }
            };
            (left, right)
        }
        EyeType::Star => {
            let left = rsx! {
                polygon {
                    points: "50,52 52,57 57,57 53,60 54,65 50,62 46,65 47,60 43,57 48,57",
                    fill: "#fbbf24",
                }
            };
            let right = rsx! {
                polygon {
                    points: "90,52 92,57 97,57 93,60 94,65 90,62 86,65 87,60 83,57 88,57",
                    fill: "#fbbf24",
                }
            };
            (left, right)
        }
        EyeType::Dizzy => {
            let left = rsx! {
                g {
                    path { d: "M 46 56 Q 54 64 46 64 Q 38 64 46 56", fill: "none", stroke: "{eye_color}", stroke_width: "2" }
                    circle { cx: "50", cy: "60", r: "1.5", fill: "{eye_color}" }
                }
            };
            let right = rsx! {
                g {
                    path { d: "M 86 56 Q 94 64 86 64 Q 78 64 86 56", fill: "none", stroke: "{eye_color}", stroke_width: "2" }
                    circle { cx: "90", cy: "60", r: "1.5", fill: "{eye_color}" }
                }
            };
            (left, right)
        }
        EyeType::SleepyBlink => {
            let left = rsx! {
                path {
                    d: "M 43 61 Q 50 58 57 61",
                    fill: "none",
                    stroke: "{eye_color}",
                    stroke_width: "2.5",
                    stroke_linecap: "round",
                }
            };
            let right = rsx! {
                path {
                    d: "M 83 61 Q 90 58 97 61",
                    fill: "none",
                    stroke: "{eye_color}",
                    stroke_width: "2.5",
                    stroke_linecap: "round",
                }
            };
            (left, right)
        }
        EyeType::Bored => {
            let left = rsx! {
                g {
                    ellipse { cx: "50", cy: "60", rx: "6", ry: "4", fill: "{eye_color}" }
                    line { x1: "43", y1: "57", x2: "57", y2: "57", stroke: "{eye_color}", stroke_width: "1.5" }
                }
            };
            let right = rsx! {
                g {
                    ellipse { cx: "90", cy: "60", rx: "6", ry: "4", fill: "{eye_color}" }
                    line { x1: "83", y1: "57", x2: "97", y2: "57", stroke: "{eye_color}", stroke_width: "1.5" }
                }
            };
            (left, right)
        }
        EyeType::Surprised => {
            let left = rsx! {
                g {
                    circle { cx: "50", cy: "60", r: "8", fill: "white", stroke: "{eye_color}", stroke_width: "2" }
                    circle { cx: "50", cy: "60", r: "4", fill: "{eye_color}" }
                    circle { cx: "48", cy: "58", r: "1.5", fill: "white" }
                }
            };
            let right = rsx! {
                g {
                    circle { cx: "90", cy: "60", r: "8", fill: "white", stroke: "{eye_color}", stroke_width: "2" }
                    circle { cx: "90", cy: "60", r: "4", fill: "{eye_color}" }
                    circle { cx: "88", cy: "58", r: "1.5", fill: "white" }
                }
            };
            (left, right)
        }
        EyeType::Curious => {
            let left = rsx! {
                g {
                    circle { cx: "50", cy: "60", r: "6", fill: "{eye_color}" }
                    circle { cx: "52", cy: "58", r: "2", fill: "white" }
                }
            };
            let right = rsx! {
                g {
                    circle { cx: "90", cy: "60", r: "6", fill: "{eye_color}" }
                    circle { cx: "88", cy: "58", r: "2", fill: "white" }
                }
            };
            (left, right)
        }
        _ => {
            if use_gaze {
                let left = rsx! {
                    g { class: "blobbi-eye-gaze",
                        circle { cx: "50", cy: "60", r: "6", fill: "{eye_color}" }
                        circle { cx: "48", cy: "58", r: "1.5", fill: "white" }
                    }
                };
                let right = rsx! {
                    g { class: "blobbi-eye-gaze",
                        circle { cx: "90", cy: "60", r: "6", fill: "{eye_color}" }
                        circle { cx: "88", cy: "58", r: "1.5", fill: "white" }
                    }
                };
                (left, right)
            } else {
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
        MouthType::Sad => rsx! {
            path {
                d: "M 58 92 Q 70 84 82 92",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                class: "text-foreground/60",
            }
        },
        MouthType::Droopy => rsx! {
            path {
                d: "M 58 89 Q 65 86 70 88 Q 75 86 82 89",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                class: "text-foreground/60",
            }
        },
        MouthType::Sleepy => rsx! {
            path {
                d: "M 60 87 Q 70 84 80 87",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                class: "text-foreground/40",
            }
        },
        MouthType::Round => rsx! {
            circle {
                cx: "70",
                cy: "88",
                r: "5",
                fill: "currentColor",
                class: "text-foreground/40",
            }
        },
        MouthType::Small => rsx! {
            path {
                d: "M 64 87 Q 70 91 76 87",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                class: "text-foreground/60",
            }
        },
        MouthType::Smirk => rsx! {
            path {
                d: "M 60 86 Q 68 92 80 86 Q 76 89 72 88",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                class: "text-foreground/60",
            }
        },
    }
}

pub(crate) fn render_body_effects(effects: &[BodyEffect]) -> Element {
    let parts: Vec<Element> = effects
        .iter()
        .filter(|e| !matches!(e, BodyEffect::None))
        .map(|effect| match effect {
            BodyEffect::Dirt => rsx! {
                g {
                    path { d: "M 28 82 C 28 78, 32 76, 35 79 C 38 76, 38 82, 35 85 C 32 88, 28 86, 28 82 Z", fill: "#8b7355", opacity: "0.40", transform: "rotate(15 32 82)" }
                    path { d: "M 62 80 C 62 77, 66 75, 68 78 C 70 76, 70 81, 67 84 C 64 87, 62 84, 62 80 Z", fill: "#8b7355", opacity: "0.35", transform: "rotate(-20 66 80)" }
                    path { d: "M 36 85 C 36 83, 38 83, 38 85 C 38 87, 36 87, 36 85 Z", fill: "#6b5340", opacity: "0.50" }
                    path { d: "M 58 83 C 58 82, 60 82, 60 83 C 60 84, 58 84, 58 83 Z", fill: "#6b5340", opacity: "0.45" }
                    circle { cx: "50", cy: "85", r: "8", fill: "#7a6b55", opacity: "0.08" }
                }
            },
            BodyEffect::Stink => rsx! {
                g {
                    g { class: "animate-[blobbi-wisp-rise_3.5s_ease-out_infinite]",
                        path { d: "M 28 68 C 31 62, 25 56, 28 50", fill: "none", stroke: "#7c9a5e", stroke_width: "1.2", stroke_linecap: "round", opacity: "0.7" }
                        circle { cx: "26", cy: "52", r: "2", fill: "#8fac6e", opacity: "0.25" }
                    }
                    g { class: "animate-[blobbi-wisp-rise_3.5s_ease-out_infinite_1s]",
                        path { d: "M 72 65 C 75 59, 69 53, 72 47", fill: "none", stroke: "#7c9a5e", stroke_width: "1.2", stroke_linecap: "round", opacity: "0.7" }
                        circle { cx: "74", cy: "49", r: "2", fill: "#8fac6e", opacity: "0.25" }
                    }
                    g { class: "animate-[blobbi-wisp-rise_3.5s_ease-out_infinite_2s]",
                        path { d: "M 50 45 C 53 39, 47 33, 50 27", fill: "none", stroke: "#7c9a5e", stroke_width: "1.2", stroke_linecap: "round", opacity: "0.5" }
                        circle { cx: "48", cy: "29", r: "1.5", fill: "#8fac6e", opacity: "0.20" }
                    }
                }
            },
            BodyEffect::StinkFlies => rsx! {
                g {
                    g { class: "blobbi-fly blobbi-fly-0",
                        circle { r: "0.8", fill: "#4a5240", opacity: "0.75",
                            animateMotion {
                                path: "M 53 80 A 5 3 0 1 1 63 80 A 5 3 0 1 1 53 80 Z",
                                dur: "2.2s",
                                repeat_count: "indefinite",
                            }
                        }
                    }
                    g { class: "blobbi-fly blobbi-fly-1",
                        circle { r: "0.8", fill: "#4a5240", opacity: "0.75",
                            animateMotion {
                                path: "M 33 78 A 4.5 2.5 0 1 1 42 78 A 4.5 2.5 0 1 1 33 78 Z",
                                dur: "2.8s",
                                begin: "0.5s",
                                repeat_count: "indefinite",
                            }
                        }
                    }
                }
            },
            BodyEffect::Sparkle => rsx! {
                g {
                    path { d: "M 25 50 L 26 53 L 29 53 L 27 55 L 28 58 L 25 56 L 22 58 L 23 55 L 21 53 L 24 53 Z", fill: "#fbbf24", opacity: "0",
                        animate { attribute_name: "opacity", values: "0;0.7;0", dur: "2.3s", begin: "0s", repeat_count: "indefinite" }
                    }
                    path { d: "M 100 40 L 101 43 L 104 43 L 102 45 L 103 48 L 100 46 L 97 48 L 98 45 L 96 43 L 99 43 Z", fill: "#fbbf24", opacity: "0",
                        animate { attribute_name: "opacity", values: "0;0.7;0", dur: "2.6s", begin: "0.8s", repeat_count: "indefinite" }
                    }
                    path { d: "M 70 30 L 71 33 L 74 33 L 72 35 L 73 38 L 70 36 L 67 38 L 68 35 L 66 33 L 69 33 Z", fill: "#fbbf24", opacity: "0",
                        animate { attribute_name: "opacity", values: "0;0.7;0", dur: "2.1s", begin: "1.6s", repeat_count: "indefinite" }
                    }
                }
            },
            BodyEffect::Sleeping => rsx! {
                g {
                    text { x: "98", y: "42", font_size: "10", fill: "currentColor", class: "text-muted-foreground animate-[float-up_2.5s_ease-in-out_infinite]", font_family: "system-ui, sans-serif", font_weight: "bold", "Z" }
                    text { x: "108", y: "32", font_size: "8", fill: "currentColor", class: "text-muted-foreground animate-[float-up_2.5s_ease-in-out_infinite_0.5s]", font_family: "system-ui, sans-serif", font_weight: "bold", "z" }
                    text { x: "116", y: "24", font_size: "6", fill: "currentColor", class: "text-muted-foreground animate-[float-up_2.5s_ease-in-out_infinite_1s]", font_family: "system-ui, sans-serif", font_weight: "bold", "z" }
                }
            },
            BodyEffect::Excited => rsx! {
                g {
                    text { x: "15", y: "50", font_size: "8", "⚡" }
                    text { x: "110", y: "40", font_size: "8", "⚡" }
                }
            },
            BodyEffect::Music => rsx! {
                g {
                    text { x: "15", y: "55", font_size: "8", "🎵" }
                    text { x: "108", y: "45", font_size: "7", "♪" }
                }
            },
            BodyEffect::Singing => rsx! {
                g {
                    text { x: "12", y: "50", font_size: "9", "🎵" }
                    text { x: "110", y: "40", font_size: "7", "♪" }
                    text { x: "18", y: "42", font_size: "6", "♫" }
                }
            },
            BodyEffect::Love => rsx! {
                g {
                    text { x: "20", y: "60", font_size: "8", "💕" }
                    text { x: "105", y: "50", font_size: "6", "♥" }
                }
            },
            BodyEffect::AngerRise => rsx! {
                g {
                    rect { x: "30", y: "70", width: "80", height: "40", rx: "20", fill: "rgba(220,38,38,0.10)", class: "animate-[blobbi-anger-rise_2.5s_ease-in-out_infinite]" }
                    rect { x: "35", y: "78", width: "70", height: "28", rx: "14", fill: "rgba(220,38,38,0.06)" }
                }
            },
            BodyEffect::Food => rsx! {
                g {
                    text { x: "20", y: "95", font_size: "8", "🍞" }
                }
            },
            _ => rsx! { g {} },
        })
        .collect();
    if parts.is_empty() {
        rsx! { g {} }
    } else {
        rsx! { g { {parts.into_iter()} } }
    }
}

use super::utils::lighten;

fn darken_hex(hex: &str, amount: u32) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return "#333333".to_string();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0).saturating_sub(amount as u8);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0).saturating_sub(amount as u8);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0).saturating_sub(amount as u8);
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

fn render_eyebrow(config: &Option<EyebrowConfig>, left_x: f64, right_x: f64, y: f64) -> Element {
    match config {
        Some(brow) => {
            let angle = brow.angle;
            let oy = brow.offset_y;
            let by = y + oy;
            let color = "currentColor";
            let opacity = if brow.worried { "0.7" } else { "0.5" };
            let curve = if brow.worried { 1.5 } else { 0.0 };
            let half_len = 10.0;
            rsx! {
                g {
                    path {
                        d: "M {left_x - half_len} {by + angle * 0.3} Q {left_x} {by + angle * 0.3 - curve - 2.5} {left_x + half_len} {by - angle * 0.3}",
                        fill: "none",
                        stroke: "{color}",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        class: "text-foreground",
                        opacity: "{opacity}",
                    }
                    path {
                        d: "M {right_x - half_len} {by - angle * 0.3} Q {right_x} {by - angle * 0.3 - curve - 2.5} {right_x + half_len} {by + angle * 0.3}",
                        fill: "none",
                        stroke: "{color}",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        class: "text-foreground",
                        opacity: "{opacity}",
                    }
                }
            }
        }
        None => rsx! { g {} },
    }
}

fn render_extras(extras: &Extras, _eye_type: EyeType) -> Element {
    let mut parts: Vec<Element> = Vec::new();

    if let Some(tears) = &extras.tears {
        let (left_tears, right_tears) = match tears.eye {
            TearEye::Both => (true, true),
            TearEye::Left => (true, false),
            TearEye::Right => (false, true),
            TearEye::Alternating => (true, true),
        };
        let alt_class = matches!(tears.eye, TearEye::Alternating)
            .then(|| " animate-[blobbi-tear-alt_1.5s_ease-in-out_infinite]")
            .unwrap_or("");
        if left_tears {
            parts.push(rsx! {
                g { class: "animate-[blobbi-tear-fall_2s_ease-in_infinite]{alt_class}",
                    ellipse { cx: "48", cy: "72", rx: "2", ry: "3", fill: "rgba(100,180,255,0.6)" }
                    ellipse { cx: "47", cy: "80", rx: "1.5", ry: "2", fill: "rgba(100,180,255,0.4)" }
                }
            });
        }
        if right_tears {
            let delay = if matches!(tears.eye, TearEye::Alternating) {
                " animation-delay: 0.75s;"
            } else {
                ""
            };
            parts.push(rsx! {
                g { class: "animate-[blobbi-tear-fall_2s_ease-in_infinite]",
                    style: "{delay}",
                    ellipse { cx: "92", cy: "72", rx: "2", ry: "3", fill: "rgba(100,180,255,0.6)" }
                    ellipse { cx: "93", cy: "80", rx: "1.5", ry: "2", fill: "rgba(100,180,255,0.4)" }
                }
            });
        }
    }

    if extras.drool {
        parts.push(rsx! {
            g { class: "animate-[blobbi-drool_3s_ease-in-out_infinite]",
                ellipse { cx: "72", cy: "95", rx: "2", ry: "4", fill: "rgba(100,200,255,0.5)" }
            }
        });
    }

    if extras.food_icon {
        parts.push(rsx! {
            text {
                x: "100",
                y: "98",
                font_size: "10",
                "🍽️",
            }
        });
    }

    if parts.is_empty() {
        rsx! { g {} }
    } else {
        rsx! { g { {parts.into_iter()} } }
    }
}
