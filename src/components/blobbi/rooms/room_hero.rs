use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::blobbi_visual::BlobbiVisual;
use crate::components::blobbi::visual::recipe::stat_color;
use crate::components::blobbi::visual::status_reaction::resolve_recipe_with_override;
use crate::components::blobbi::core::decay::{apply_decay, get_visible_stats};
use crate::components::blobbi::actions::music::floating_notes::FloatingMusicNotes;
use crate::components::blobbi::social::sing_effect::SingEffect;

#[component]
pub fn RoomHero(blobbi: BlobbiCompanion, reaction: Option<String>) -> Element {
    let _recipe = resolve_recipe_with_override(&blobbi);
    let base_color = blobbi.visual_traits.base_color.clone();

    let happiness = blobbi.stats.happiness;
    let animation_style = match reaction.as_deref() {
        Some("listening") => {
            let sway = 5.0;
            format!("animation: blobbi-sway {sway:.1}s ease-in-out infinite")
        }
        Some("happy") => {
            let bob = 1.5;
            format!("animation: blobbi-bob {bob:.1}s ease-in-out infinite")
        }
        Some("eating") => {
            let dur = 0.6;
            format!("animation: blobbi-wobble {dur:.1}s ease-in-out infinite")
        }
        _ => {
            let bob = 4.0 - (happiness / 100.0) * 1.5;
            let sway = 6.0 - (happiness / 100.0) * 2.0;
            format!("animation: blobbi-bob {bob:.1}s ease-in-out infinite, blobbi-sway {sway:.1}s ease-in-out infinite")
        }
    };

    rsx! {
        div { class: "relative flex flex-col items-center justify-center pt-10 px-4 sm:px-6 flex-1 min-h-0",
            StatsCrown { blobbi: blobbi.clone() }

            div {
                class: "relative transition-all duration-500",
                style: if !blobbi.is_sleeping() { animation_style } else { String::new() },
                div { class: "absolute inset-0 -m-16 sm:-m-20 bg-primary/5 rounded-full blur-3xl pointer-events-none" }

                if reaction.as_deref() == Some("listening") {
                    FloatingMusicNotes { active: true }
                }

                if reaction.as_deref() == Some("singing") {
                    SingEffect {}
                }

                BlobbiVisual {
                    blobbi: blobbi.clone(),
                    size: Some(if blobbi.is_egg() { "144".to_string() } else { "192".to_string() })
                }
            }

            if !blobbi.is_egg() {
                div { class: "flex flex-col items-center mt-1",
                    h2 {
                        class: "text-xl sm:text-2xl md:text-3xl font-bold text-center",
                        style: "color: {base_color}",
                        "{blobbi.display_name()}"
                    }
                    if let Some(title) = &blobbi.personality.title {
                        if !title.is_empty() {
                            span { class: "text-xs text-muted-foreground mt-0.5",
                                "{title}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatsCrown(blobbi: BlobbiCompanion) -> Element {
    let projected = apply_decay(&blobbi, crate::platform::timestamp::now_secs());
    let all_visible = get_visible_stats(projected.stage);
    let stats = &projected.stats;
    let visible: Vec<&&str> = all_visible.iter().filter(|s| {
        let v = match **s {
            "hunger" => stats.hunger,
            "happiness" => stats.happiness,
            "health" => stats.health,
            "hygiene" => stats.hygiene,
            "energy" => stats.energy,
            _ => 100.0,
        };
        v < 70.0
    }).collect();
    let count = visible.len();
    let arc_spread: f64 = if count <= 2 { 80.0 } else if count <= 3 { 110.0 } else { 140.0 };
    let arc_half = arc_spread / 2.0;
    let radius = 120.0_f64;

    rsx! {
        div { class: "relative flex items-end justify-center w-full mb-4 sm:mb-8",
            style: "height: 40px",
            for (i, stat_name) in visible.iter().enumerate() {
                {
                    let value = match **stat_name {
                        "hunger" => stats.hunger,
                        "happiness" => stats.happiness,
                        "health" => stats.health,
                        "hygiene" => stats.hygiene,
                        "energy" => stats.energy,
                        _ => 100.0,
                    };
                    let color = stat_color(value);
                    let icon = match **stat_name {
                        "hunger" => "\u{1F37D}",
                        "happiness" => "\u{1F3AE}",
                        "health" => "\u{2764}",
                        "hygiene" => "\u{1F4A7}",
                        "energy" => "\u{26A1}",
                        _ => "\u{2753}",
                    };

                    let angle_deg = if count == 1 {
                        0.0
                    } else {
                        -arc_half + (arc_spread / (count - 1) as f64) * i as f64
                    };
                    let angle_rad = angle_deg * std::f64::consts::PI / 180.0;
                    let x = angle_rad.sin() * radius;
                    let y = angle_rad.cos() * radius - radius;
                    let position_style = format!("transform: translate(-50%, 0); left: calc(50% + {x:.1}px); bottom: {y:.1}px;");

                    rsx! {
                        StatIndicator {
                            key: "{stat_name}",
                            icon: icon.to_string(),
                            value,
                            color: color.to_string(),
                            position_style,
                            stat_name: stat_name.to_string(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatIndicator(icon: String, value: f64, color: String, position_style: String, stat_name: String) -> Element {
    let (warn_threshold, crit_threshold) = match stat_name.as_str() {
        "health" => (60.0, 30.0),
        "energy" => (40.0, 20.0),
        "hunger" => (50.0, 25.0),
        "hygiene" => (50.0, 25.0),
        "happiness" => (40.0, 20.0),
        _ => (40.0, 20.0),
    };
    let is_low = value < warn_threshold;
    let is_critical = value < crit_threshold;
    let display_value = (value * 0.94).round() as u64;
    let dash = format!("{} 100", display_value);

    rsx! {
        div {
            class: "absolute transition-all duration-500",
            style: position_style,
            div {
                class: if is_critical { "relative size-12 sm:size-[4.5rem] rounded-full flex items-center justify-center bg-muted/10 animate-pulse" }
                    else { "relative size-12 sm:size-[4.5rem] rounded-full flex items-center justify-center bg-muted/10" },
                svg { class: "absolute inset-0 -rotate-90", view_box: "0 0 36 36",
                    circle { cx: "18", cy: "18", r: "15", fill: "none", stroke: "currentColor", stroke_width: "2.5", class: "text-muted/15" }
                    circle {
                        cx: "18", cy: "18", r: "15", fill: "none", stroke_width: "2.5", stroke_linecap: "round",
                        stroke: "{color}",
                        stroke_dasharray: "{dash}",
                        class: "transition-all duration-500",
                    }
                }
                span { class: "relative text-lg sm:text-2xl", "{icon}" }
            }
            if is_low {
                span { class: "absolute -top-1.5 -right-2 size-3",
                    if is_critical {
                        "⚠"
                    } else {
                        "🔸"
                    }
                }
            }
        }
    }
}
