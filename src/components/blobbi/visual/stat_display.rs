use dioxus::prelude::*;

use crate::utils::nip_bb::BlobbiStats;
use crate::utils::nip_bb::*;

use super::recipe::stat_color;

#[component]
pub fn StatDisplay(stats: BlobbiStats, compact: bool) -> Element {
    let radius = if compact { 18.0 } else { 28.0 };
    let stroke_width = if compact { 3.0 } else { 4.0 };
    let size = (radius + stroke_width + 2.0) * 2.0;
    let center = size / 2.0;
    let circumference = 2.0 * std::f64::consts::PI * radius;

    let gap = if compact { 2 } else { 4 };
    rsx! {
        div { class: "flex items-center justify-center",
            style: "gap: {gap}px",
            {render_stat_arc("hunger", stats.hunger, circumference, radius, center, stroke_width, compact)}
            {render_stat_arc("happiness", stats.happiness, circumference, radius, center, stroke_width, compact)}
            {render_stat_arc("health", stats.health, circumference, radius, center, stroke_width, compact)}
            {render_stat_arc("hygiene", stats.hygiene, circumference, radius, center, stroke_width, compact)}
            {render_stat_arc("energy", stats.energy, circumference, radius, center, stroke_width, compact)}
        }
    }
}

#[allow(dead_code)]
fn render_stat_arc(
    label: &str,
    value: f64,
    circumference: f64,
    radius: f64,
    center: f64,
    stroke_width: f64,
    compact: bool,
) -> Element {
    let pct = (value / STAT_MAX).clamp(0.0, 1.0);
    let dash_offset = circumference * (1.0 - pct);
    let color = stat_color(value);
    let size = (radius + stroke_width + 2.0) * 2.0;
    let icon = match label {
        "hunger" => "🍔",
        "happiness" => "😊",
        "health" => "❤️",
        "hygiene" => "✨",
        "energy" => "⚡",
        _ => "?",
    };

    rsx! {
        div { class: "flex flex-col items-center",
            svg {
                width: "{size}",
                height: "{size}",
                view_box: "0 0 {size} {size}",
                circle {
                    cx: "{center}",
                    cy: "{center}",
                    r: "{radius}",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "{stroke_width}",
                    class: "text-muted/30",
                }
                circle {
                    cx: "{center}",
                    cy: "{center}",
                    r: "{radius}",
                    fill: "none",
                    stroke: "{color}",
                    stroke_width: "{stroke_width}",
                    stroke_dasharray: "{circumference}",
                    stroke_dashoffset: "{dash_offset}",
                    stroke_linecap: "round",
                    transform: "rotate(-90 {center} {center})",
                    style: "transition: stroke-dashoffset 0.5s ease",
                }
                text {
                    x: "{center}",
                    y: "{center}",
                    text_anchor: "middle",
                    dominant_baseline: "central",
                    font_size: if compact { "10" } else { "14" },
                    fill: "currentColor",
                    class: "text-foreground",
                    {if compact { icon.to_string() } else { format!("{:.0}", value) }}
                }
            }
            if !compact {
                span { class: "text-xs text-muted-foreground mt-0.5",
                    {label.chars().next().unwrap_or('?').to_uppercase().to_string()}
                }
            }
        }
    }
}

#[component]
pub fn StatBars(stats: BlobbiStats) -> Element {
    rsx! {
        div { class: "space-y-1.5",
            {render_stat_bar("🍔 Hunger", stats.hunger)}
            {render_stat_bar("😊 Happy", stats.happiness)}
            {render_stat_bar("❤️ Health", stats.health)}
            {render_stat_bar("✨ Clean", stats.hygiene)}
            {render_stat_bar("⚡ Energy", stats.energy)}
        }
    }
}

#[allow(dead_code)]
fn render_stat_bar(label: &str, value: f64) -> Element {
    let pct = (value / STAT_MAX * 100.0).clamp(0.0, 100.0);
    let color = stat_color(value);

    rsx! {
        div { class: "flex items-center gap-2",
            span { class: "text-xs w-16 truncate", "{label}" }
            div { class: "flex-1 h-2 bg-muted rounded-full overflow-hidden",
                div {
                    class: "h-full rounded-full transition-all duration-500",
                    style: "width: {pct}%; background-color: {color}",
                }
            }
            span { class: "text-xs text-muted-foreground w-8 text-right",
                "{value:.0}"
            }
        }
    }
}
