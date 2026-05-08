use dioxus::prelude::*;

use super::utils::lighten as lighten_color;

#[component]
pub fn EggVisual(
    base_color: String,
    crack_level: Option<u32>,
    egg_temperature: Option<f64>,
    special_mark: Option<String>,
    is_divine: Option<bool>,
    reaction: Option<String>,
    is_dirty: Option<bool>,
    is_sick: Option<bool>,
    is_happy: Option<bool>,
    is_wiggling: Option<bool>,
) -> Element {
    let level = crack_level.unwrap_or(0);
    let has_cracks = crack_level.is_some();
    let temperature = egg_temperature.unwrap_or(50.0);
    let divine = is_divine.unwrap_or(false);
    let dirty = is_dirty.unwrap_or(false);
    let sick = is_sick.unwrap_or(false);
    let happy = is_happy.unwrap_or(false);
    let wiggling = is_wiggling.unwrap_or(false);
    let reaction_str = reaction.unwrap_or_else(|| "idle".to_string());
    let mark = special_mark.unwrap_or_default();

    let warmth_color = if divine {
        "rgba(34,197,94,0.45)"
    } else if temperature < 30.0 {
        "rgba(59,130,246,0.4)"
    } else if temperature < 50.0 {
        "rgba(147,197,253,0.35)"
    } else if temperature < 70.0 {
        "rgba(250,204,21,0.35)"
    } else if temperature < 85.0 {
        "rgba(249,115,22,0.4)"
    } else {
        "rgba(239,68,68,0.45)"
    };

    let reaction_class = match reaction_str.as_str() {
        "swaying" => "animate-[blobbi-sway_2s_ease-in-out_infinite]",
        "singing" => "animate-[blobbi-idle-bounce_1s_ease-in-out_infinite]",
        "happy" => "animate-[blobbi-play-bounce_1.5s_ease-in-out_infinite]",
        _ => "animate-[blobbi-incubation-glow_2s_ease-in-out_infinite]",
    };

    let wiggle_class = if wiggling { "egg-tap-wiggle" } else { "" };

    rsx! {
        div { class: "{wiggle_class}",
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                view_box: "0 0 120 160",
                width: "120",
                height: "160",
                class: "{reaction_class}",

                defs {
                    radialGradient { id: "egg-gradient", cx: "40%", cy: "35%", r: "60%",
                        stop { offset: "0%", stop_color: "{lighten_color(&base_color, 30)}" }
                        stop { offset: "100%", stop_color: "{base_color}" }
                    }
                    radialGradient { id: "egg-shine", cx: "35%", cy: "30%", r: "25%",
                        stop { offset: "0%", stop_color: "rgba(255,255,255,0.4)" }
                        stop { offset: "100%", stop_color: "rgba(255,255,255,0)" }
                    }
                    radialGradient { id: "egg-warmth", cx: "50%", cy: "50%", r: "55%",
                        stop { offset: "0%", stop_color: "{warmth_color}" }
                        stop { offset: "100%", stop_color: "rgba(0,0,0,0)" }
                    }
                }

                ellipse {
                    cx: "60", cy: "85", rx: "58", ry: "72",
                    fill: "url(#egg-warmth)",
                    class: "egg-warmth-glow",
                }

                if divine {
                    ellipse {
                        cx: "60", cy: "85", rx: "55", ry: "70",
                        fill: "none",
                        stroke: "rgba(34,197,94,0.3)",
                        stroke_width: "3",
                        class: "egg-divine-pulse",
                    }
                    ellipse {
                        cx: "60", cy: "85", rx: "60", ry: "74",
                        fill: "none",
                        stroke: "rgba(34,197,94,0.15)",
                        stroke_width: "2",
                        class: "egg-divine-pulse",
                        opacity: "0.6",
                    }
                }

                ellipse {
                    cx: "60", cy: "85", rx: "48", ry: "62",
                    fill: "url(#egg-gradient)",
                    stroke: "rgba(0,0,0,0.1)",
                    stroke_width: "1",
                    animate {
                        attribute_name: "ry",
                        values: "62;61;62",
                        dur: "3s",
                        repeat_count: "indefinite",
                    }
                }
                ellipse {
                    cx: "60", cy: "85", rx: "48", ry: "62",
                    fill: "url(#egg-shine)",
                }
                ellipse {
                    cx: "60", cy: "85", rx: "48", ry: "62",
                    fill: "none",
                    stroke: "rgba(255,255,255,0.15)",
                    stroke_width: "0.5",
                    transform: "rotate(-15 60 85)",
                }

                if has_cracks {
                    path { d: "M57 83 L60 80 L63 83", fill: "none", stroke: "rgba(0,0,0,0.2)", stroke_width: "0.8", stroke_linecap: "round" }
                    path { d: "M55 86 L59 84", fill: "none", stroke: "rgba(0,0,0,0.15)", stroke_width: "0.6", stroke_linecap: "round" }
                    path { d: "M61 84 L65 86", fill: "none", stroke: "rgba(0,0,0,0.15)", stroke_width: "0.6", stroke_linecap: "round" }
                }

                if has_cracks && level >= 1 {
                    path {
                        d: "M42 63 L48 59 L55 64 L60 58 L66 63 L72 59 L78 63",
                        fill: "none", stroke: "rgba(0,0,0,0.55)", stroke_width: "1.5", stroke_linecap: "round",
                    }
                    path { d: "M48 59 L46 55", fill: "none", stroke: "rgba(0,0,0,0.35)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M55 64 L53 68", fill: "none", stroke: "rgba(0,0,0,0.35)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M60 58 L62 54", fill: "none", stroke: "rgba(0,0,0,0.35)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M66 63 L68 67", fill: "none", stroke: "rgba(0,0,0,0.35)", stroke_width: "1", stroke_linecap: "round" }
                    path {
                        d: "M44 64 L50 60 L56 65 L62 59 L68 64 L74 60",
                        fill: "none", stroke: "rgba(255,255,255,0.12)", stroke_width: "1", stroke_linecap: "round",
                    }
                }

                if has_cracks && level >= 2 {
                    path {
                        d: "M30 62 L38 58 L45 64 L52 57 L58 65 L65 58 L72 64 L78 57 L85 63 L90 60",
                        fill: "none", stroke: "rgba(0,0,0,0.6)", stroke_width: "2", stroke_linecap: "round",
                    }
                    path { d: "M38 58 L35 53", fill: "none", stroke: "rgba(0,0,0,0.4)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M45 64 L42 69", fill: "none", stroke: "rgba(0,0,0,0.4)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M52 57 L50 52", fill: "none", stroke: "rgba(0,0,0,0.4)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M58 65 L56 70", fill: "none", stroke: "rgba(0,0,0,0.4)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M65 58 L68 53", fill: "none", stroke: "rgba(0,0,0,0.4)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M72 64 L75 69", fill: "none", stroke: "rgba(0,0,0,0.4)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M78 57 L80 52", fill: "none", stroke: "rgba(0,0,0,0.4)", stroke_width: "1", stroke_linecap: "round" }
                    path { d: "M32 57 L34 62", fill: "none", stroke: "rgba(0,0,0,0.3)", stroke_width: "0.8", stroke_linecap: "round" }
                    path { d: "M88 58 L86 63", fill: "none", stroke: "rgba(0,0,0,0.3)", stroke_width: "0.8", stroke_linecap: "round" }
                    path {
                        d: "M32 63 L40 59 L48 65 L55 58 L62 66 L68 59 L75 65 L82 58 L88 64",
                        fill: "none", stroke: "rgba(255,255,255,0.12)", stroke_width: "1.2", stroke_linecap: "round",
                    }
                }

                if has_cracks && level >= 3 {
                    path {
                        d: "M15 62 L23 59 L32 64 L40 58 L50 65 L60 57 L70 64 L80 58 L88 63 L96 59 L105 64",
                        fill: "none", stroke: "rgba(0,0,0,0.65)", stroke_width: "2.5", stroke_linecap: "round",
                    }
                    path { d: "M23 59 L20 53", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M32 64 L29 70", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M40 58 L37 52", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M50 65 L47 71", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M60 57 L63 51", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M70 64 L73 70", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M80 58 L83 52", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M88 63 L91 69", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M96 59 L99 53", fill: "none", stroke: "rgba(0,0,0,0.45)", stroke_width: "1.2", stroke_linecap: "round" }
                    path { d: "M18 56 L20 61", fill: "none", stroke: "rgba(0,0,0,0.3)", stroke_width: "0.8", stroke_linecap: "round" }
                    path { d: "M45 54 L48 58", fill: "none", stroke: "rgba(0,0,0,0.3)", stroke_width: "0.8", stroke_linecap: "round" }
                    path { d: "M75 53 L78 57", fill: "none", stroke: "rgba(0,0,0,0.3)", stroke_width: "0.8", stroke_linecap: "round" }
                    path { d: "M102 56 L100 61", fill: "none", stroke: "rgba(0,0,0,0.3)", stroke_width: "0.8", stroke_linecap: "round" }
                    path { d: "M55 50 L58 54", fill: "none", stroke: "rgba(0,0,0,0.3)", stroke_width: "0.8", stroke_linecap: "round" }
                    path {
                        d: "M17 63 L25 60 L35 65 L43 59 L53 66 L63 58 L73 65 L83 59 L91 64 L99 60 L104 65",
                        fill: "none", stroke: "rgba(255,255,255,0.12)", stroke_width: "1.5", stroke_linecap: "round",
                    }
                }

                {render_special_mark(&mark)}

                if dirty {
                    circle { cx: "35", cy: "95", r: "2", fill: "rgba(139,92,42,0.5)", class: "egg-dust-particle" }
                    circle { cx: "78", cy: "100", r: "1.5", fill: "rgba(139,92,42,0.4)", class: "egg-dust-particle" }
                    circle { cx: "50", cy: "108", r: "1.8", fill: "rgba(139,92,42,0.45)", class: "egg-dust-particle" }
                    circle { cx: "68", cy: "105", r: "1.2", fill: "rgba(139,92,42,0.35)", class: "egg-dust-particle" }
                    path {
                        d: "M82 55 Q84 50 86 55 Q84 58 82 55",
                        fill: "rgba(100,180,255,0.6)",
                        class: "egg-sweat-drop",
                    }
                }

                if sick {
                    path {
                        d: "M30 70 Q26 66 30 62 Q34 58 38 62 Q42 66 38 70 Q34 74 30 70",
                        fill: "none", stroke: "rgba(120,80,200,0.5)", stroke_width: "1",
                        class: "egg-spiral-spin",
                    }
                    path {
                        d: "M82 68 Q78 64 82 60 Q86 56 90 60 Q94 64 90 68 Q86 72 82 68",
                        fill: "none", stroke: "rgba(120,80,200,0.5)", stroke_width: "1",
                        class: "egg-spiral-spin",
                    }
                }

                if happy && !dirty && !sick {
                    polygon {
                        points: "25,55 27,60 32,60 28,63 29,68 25,65 21,68 22,63 18,60 23,60",
                        fill: "rgba(250,204,21,0.7)", class: "egg-sparkle-twinkle",
                    }
                    polygon {
                        points: "95,50 96,53 99,53 97,55 98,58 95,56 92,58 93,55 91,53 94,53",
                        fill: "rgba(250,204,21,0.6)", class: "egg-sparkle-twinkle",
                    }
                    polygon {
                        points: "50,35 51,38 54,38 52,40 53,43 50,41 47,43 48,40 46,38 49,38",
                        fill: "rgba(250,204,21,0.5)", class: "egg-sparkle-twinkle",
                    }
                }

                circle { cx: "20", cy: "70", r: "2", fill: "{lighten_color(&base_color, 50)}", opacity: "0.3", class: "egg-ambient-particle" }
                circle { cx: "100", cy: "60", r: "1.5", fill: "{lighten_color(&base_color, 50)}", opacity: "0.25", class: "egg-ambient-particle", style: "animation-delay: 0.8s;" }
                circle { cx: "60", cy: "25", r: "1.8", fill: "{lighten_color(&base_color, 50)}", opacity: "0.2", class: "egg-ambient-particle", style: "animation-delay: 1.6s;" }
            }
        }
    }
}

fn render_special_mark(mark: &str) -> Element {
    match mark {
        "sigil_eye" => rsx! {
            g {
                path { d: "M52 80 Q60 74 68 80 Q60 86 52 80", fill: "none", stroke: "rgba(255,255,255,0.4)", stroke_width: "1" }
                circle { cx: "60", cy: "80", r: "3", fill: "rgba(255,255,255,0.3)" }
                circle { cx: "60", cy: "80", r: "1.5", fill: "rgba(255,255,255,0.5)" }
            }
        },
        "shimmer_band" => rsx! {
            g {
                rect { x: "30", y: "82", width: "60", height: "6", rx: "3", fill: "rgba(255,255,255,0.15)" }
                rect { x: "35", y: "83", width: "50", height: "4", rx: "2", fill: "rgba(255,255,255,0.1)" }
            }
        },
        "rune_top" => rsx! {
            g {
                path { d: "M55 40 L60 35 L65 40", fill: "none", stroke: "rgba(255,255,255,0.3)", stroke_width: "1", stroke_linecap: "round" }
                line { x1: "60", y1: "35", x2: "60", y2: "45", stroke: "rgba(255,255,255,0.25)", stroke_width: "0.8" }
                line { x1: "56", y1: "42", x2: "64", y2: "42", stroke: "rgba(255,255,255,0.2)", stroke_width: "0.6" }
            }
        },
        "ring_mark" => rsx! {
            circle {
                cx: "60", cy: "85", r: "20",
                fill: "none", stroke: "rgba(255,255,255,0.2)", stroke_width: "1.5", stroke_dasharray: "3 3",
            }
        },
        "oval_spots" => rsx! {
            g {
                ellipse { cx: "45", cy: "75", rx: "5", ry: "3", fill: "rgba(255,255,255,0.12)" }
                ellipse { cx: "72", cy: "90", rx: "4", ry: "2.5", fill: "rgba(255,255,255,0.1)" }
                ellipse { cx: "55", cy: "100", rx: "3.5", ry: "2", fill: "rgba(255,255,255,0.08)" }
            }
        },
        "glow_crack_pattern" => rsx! {
            g {
                path { d: "M50 60 L55 75 L48 90", fill: "none", stroke: "rgba(255,200,100,0.3)", stroke_width: "1", stroke_linecap: "round" }
                path { d: "M70 55 L68 72 L75 88", fill: "none", stroke: "rgba(255,200,100,0.25)", stroke_width: "0.8", stroke_linecap: "round" }
                path { d: "M58 65 L62 80", fill: "none", stroke: "rgba(255,200,100,0.2)", stroke_width: "0.6", stroke_linecap: "round" }
            }
        },
        "dot_center" => rsx! {
            g {
                circle { cx: "60", cy: "85", r: "4", fill: "rgba(255,255,255,0.25)" }
                circle { cx: "60", cy: "85", r: "2", fill: "rgba(255,255,255,0.4)" }
            }
        },
        _ => rsx! { g {} },
    }
}
