pub mod color_scales;

use dioxus::prelude::*;
use color_scales::{ColorScale, HorizontalLine, color_at};

fn nice_ticks(y_min: f64, y_max: f64, count: usize) -> Vec<f64> {
    let range = y_max - y_min;
    if range <= 0.0 {
        return vec![y_min];
    }
    let raw_step = range / count.max(1) as f64;
    let magnitude = 10_f64.powf(raw_step.log10().floor());
    let residual = raw_step / magnitude;
    let nice_step = magnitude
        * match residual {
            r if r <= 1.5 => 1.0,
            r if r <= 3.5 => 2.0,
            r if r <= 7.5 => 5.0,
            _ => 10.0,
        };
    let start = (y_min / nice_step).floor() * nice_step;
    let mut ticks: Vec<f64> = (0..=count + 2)
        .map(|i| start + i as f64 * nice_step)
        .filter(|&v| v >= y_min - nice_step * 0.5 && v <= y_max + nice_step * 0.5)
        .collect();
    ticks.dedup();
    ticks
}

fn format_tick(v: f64) -> String {
    if v == v.floor() || v.abs() >= 10.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

#[component]
pub fn SvgLineChart(
    data: Vec<(usize, f64)>,
    x_labels: Vec<String>,
    y_min: f64,
    y_max: f64,
    color_scale: ColorScale,
    show_area_fill: bool,
    width: f64,
    height: f64,
    padding_left: f64,
    padding_right: f64,
    padding_top: f64,
    padding_bottom: f64,
    horizontal_lines: Vec<HorizontalLine>,
) -> Element {
    if data.is_empty() {
        return rsx! {
            div { class: "h-full flex items-center justify-center text-muted-foreground text-sm",
                "No data"
            }
        };
    }

    let chart_w = width - padding_left - padding_right;
    let chart_h = height - padding_top - padding_bottom;
    let y_range = y_max - y_min;

    let points: Vec<(f64, f64)> = data
        .iter()
        .map(|(i, y)| {
            let x =
                padding_left + (*i as f64 / (data.len().max(1) - 1).max(1) as f64) * chart_w;
            let cy = padding_top + chart_h - ((y - y_min) / y_range.max(0.001)) * chart_h;
            (x, cy)
        })
        .collect();

    let line_path = points
        .iter()
        .enumerate()
        .map(|(i, (x, y))| {
            if i == 0 {
                format!("M{:.1},{:.1}", x, y)
            } else {
                format!(" L{:.1},{:.1}", x, y)
            }
        })
        .collect::<String>();

    let area_path = if show_area_fill {
        let bottom_y = padding_top + chart_h;
        let first_x = points.first().map(|(x, _)| *x).unwrap_or(0.0);
        let last_x = points.last().map(|(x, _)| *x).unwrap_or(0.0);
        format!(
            "{} L{:.1},{:.1} L{:.1},{:.1} Z",
            line_path, last_x, bottom_y, first_x, bottom_y
        )
    } else {
        String::new()
    };

    let label_step = (x_labels.len() / 8).clamp(1, 4);
    let y_ticks = nice_ticks(y_min, y_max, 5);

    let x_end_grid = width - padding_right;

    rsx! {
        svg {
            view_box: "0 0 {width} {height}",
            class: "w-full",
            style: "height: 250px; min-width: {width}px;",
            if show_area_fill {
                defs {
                    linearGradient { id: "area-grad", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%", stop_color: "{color_at(&color_scale, data.iter().map(|(_,v)| *v).fold(f64::NEG_INFINITY, f64::max))}", stop_opacity: "0.3" }
                        stop { offset: "100%", stop_color: "{color_at(&color_scale, data.iter().map(|(_,v)| *v).fold(f64::NEG_INFINITY, f64::max))}", stop_opacity: "0.05" }
                    }
                }
                path { d: "{area_path}", fill: "url(#area-grad)", stroke: "none" }
            }
            g { class: "y-grid",
                for tick in &y_ticks {
                    {
                        let ly = padding_top + chart_h - ((tick - y_min) / y_range.max(0.001)) * chart_h;
                        let tick_label = format_tick(*tick);
                        let label_x = padding_left - 5.0;
                        let label_y = ly + 3.5;
                        rsx! {
                            line { x1: "{padding_left}", y1: "{ly}", x2: "{x_end_grid}", y2: "{ly}", stroke: "currentColor", stroke_opacity: "0.08", stroke_width: "1" }
                            text { x: "{label_x}", y: "{label_y}", text_anchor: "end", font_size: "10", fill: "currentColor", fill_opacity: "0.5", "{tick_label}" }
                        }
                    }
                }
            }
            for line in &horizontal_lines {
                {
                    let ly = padding_top + chart_h - ((line.value - y_min) / y_range.max(0.001)) * chart_h;
                    let dash = if line.dashed { "5,3" } else { "" };
                    let x_text = x_end_grid + 2.0;
                    let y_text = ly + 3.0;
                    rsx! {
                        line { x1: "{padding_left}", y1: "{ly}", x2: "{x_end_grid}", y2: "{ly}", stroke: "{line.color}", stroke_width: "1", stroke_dasharray: "{dash}", opacity: "0.7" }
                        text { x: "{x_text}", y: "{y_text}", font_size: "9", fill: "{line.color}", text_anchor: "start", "{line.label}" }
                    }
                }
            }
            path { d: "{line_path}", fill: "none", stroke: "{color_at(&color_scale, data.iter().map(|(_,v)| *v).sum::<f64>() / data.len().max(1) as f64)}", stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round" }
            for (i, label) in x_labels.iter().enumerate() {
                if i % label_step == 0 || i == x_labels.len() - 1 {
                    {
                        let x = padding_left + (i as f64 / (x_labels.len().max(1) - 1).max(1) as f64) * chart_w;
                        let y_label = height - 2.0;
                        let short_label = if label.contains('T') {
                            label.split('T').nth(1).unwrap_or(label).trim_end_matches(":00").to_string()
                        } else {
                            label.split('-').next_back().unwrap_or(label).to_string()
                        };
                        rsx! {
                            text { x: "{x}", y: "{y_label}", font_size: "9", fill: "currentColor", fill_opacity: "0.5", text_anchor: "middle", "{short_label}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn SvgBarChart(
    data: Vec<f64>,
    x_labels: Vec<String>,
    y_max: f64,
    bar_color: String,
    threshold_lines: Vec<HorizontalLine>,
    width: f64,
    height: f64,
    padding_left: f64,
    padding_right: f64,
    padding_top: f64,
    padding_bottom: f64,
) -> Element {
    if data.is_empty() {
        return rsx! { div {} };
    }

    let chart_w = width - padding_left - padding_right;
    let chart_h = height - padding_top - padding_bottom;
    let bar_width = (chart_w / data.len() as f64) * 0.7;
    let gap = (chart_w / data.len() as f64) * 0.3;
    let y_range = y_max.max(0.1);

    let label_step = (x_labels.len() / 8).max(1);
    let y_ticks = nice_ticks(0.0, y_max, 4);
    let x_end_grid = width - padding_right;

    rsx! {
        svg {
            view_box: "0 0 {width} {height}",
            class: "w-full",
            style: "height: 250px; min-width: {width}px;",
            g { class: "y-grid",
                for tick in &y_ticks {
                    {
                        let ly = padding_top + chart_h - ((tick) / y_range) * chart_h;
                        let tick_label = format_tick(*tick);
                        let label_x = padding_left - 5.0;
                        let label_y = ly + 3.5;
                        rsx! {
                            line { x1: "{padding_left}", y1: "{ly}", x2: "{x_end_grid}", y2: "{ly}", stroke: "currentColor", stroke_opacity: "0.08", stroke_width: "1" }
                            text { x: "{label_x}", y: "{label_y}", text_anchor: "end", font_size: "10", fill: "currentColor", fill_opacity: "0.5", "{tick_label}" }
                        }
                    }
                }
            }
            for (i, val) in data.iter().enumerate() {
                {
                    let x = padding_left + (i as f64 / data.len() as f64) * chart_w + gap / 2.0;
                    let bar_h = (*val / y_range) * chart_h;
                    let y = padding_top + chart_h - bar_h;
                    rsx! {
                        rect { x: "{x}", y: "{y}", width: "{bar_width}", height: "{bar_h.max(0.5)}", fill: "{bar_color}", rx: "2", opacity: "0.85" }
                    }
                }
            }
            for line in &threshold_lines {
                {
                    let ly = padding_top + chart_h - ((line.value) / y_range) * chart_h;
                    let x_text = padding_left + 2.0;
                    let y_text = ly - 2.0;
                    rsx! {
                        line { x1: "{padding_left}", y1: "{ly}", x2: "{x_end_grid}", y2: "{ly}", stroke: "{line.color}", stroke_width: "1", stroke_dasharray: "4,2", opacity: "0.6" }
                        text { x: "{x_text}", y: "{y_text}", font_size: "8", fill: "{line.color}", "{line.label}" }
                    }
                }
            }
            for (i, label) in x_labels.iter().enumerate() {
                if i % label_step == 0 || i == x_labels.len() - 1 {
                    {
                        let x = padding_left + (i as f64 / data.len() as f64) * chart_w + (chart_w / data.len() as f64) / 2.0;
                        let y_label = height - 2.0;
                        rsx! {
                            text { x: "{x}", y: "{y_label}", font_size: "8", fill: "currentColor", fill_opacity: "0.5", text_anchor: "middle", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ArcProgress(
    value: f64,
    max: f64,
    arc_angle: f64,
    color: String,
    size: f64,
    center_text: String,
    sublabel: String,
) -> Element {
    let center = size / 2.0;
    let radius = center * 0.75;
    let stroke_w = size * 0.08;

    let start_angle = 270.0 - arc_angle / 2.0;
    let circumference = 2.0 * std::f64::consts::PI * radius;
    let arc_length = (arc_angle / 360.0) * circumference;
    let progress = (value / max.max(0.001)).min(1.0);
    let progress_length = progress * arc_length;
    let dashoffset = arc_length - progress_length;

    let bg_dasharray = format!("{}", arc_length);
    let fg_dasharray = format!("{}", arc_length);

    let text_y = center - 4.0;
    let sub_y = center + size * 0.12;
    let font_big = size * 0.22;
    let font_small = size * 0.09;

    rsx! {
        svg {
            view_box: "0 0 {size} {size}",
            class: "w-full h-full",
            circle {
                cx: "{center}",
                cy: "{center}",
                r: "{radius}",
                fill: "none",
                stroke: "var(--color-border)",
                stroke_width: "{stroke_w}",
                stroke_dasharray: "{bg_dasharray}",
                stroke_dashoffset: "0",
                transform: "rotate({start_angle} {center} {center})",
                stroke_linecap: "round",
            }
            circle {
                cx: "{center}",
                cy: "{center}",
                r: "{radius}",
                fill: "none",
                stroke: "{color}",
                stroke_width: "{stroke_w}",
                stroke_dasharray: "{fg_dasharray}",
                stroke_dashoffset: "{dashoffset}",
                transform: "rotate({start_angle} {center} {center})",
                stroke_linecap: "round",
                style: "transition: stroke-dashoffset 0.8s ease",
            }
            text { x: "{center}", y: "{text_y}", text_anchor: "middle", dominant_baseline: "middle", font_size: "{font_big}", font_weight: "bold", fill: "var(--color-foreground)", "{center_text}" }
            text { x: "{center}", y: "{sub_y}", text_anchor: "middle", dominant_baseline: "middle", font_size: "{font_small}", fill: "var(--color-muted-foreground)", "{sublabel}" }
        }
    }
}

#[component]
pub fn WindCompass(
    direction: f64,
    speed: f64,
    speed_unit_label: String,
    size: f64,
) -> Element {
    let center = size / 2.0;
    let radius = center * 0.7;
    let arrow_len = radius * 0.65;

    let rad = (direction - 90.0).to_radians();
    let tip_x = center + arrow_len * rad.cos();
    let tip_y = center + arrow_len * rad.sin();
    let base_x = center - arrow_len * 0.3 * rad.cos();
    let base_y = center - arrow_len * 0.3 * rad.sin();

    let perp_x = 6.0 * rad.sin();
    let perp_y = -6.0 * rad.cos();

    let arrow_path = format!(
        "M{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Z",
        tip_x, tip_y, base_x + perp_x, base_y + perp_y, base_x - perp_x, base_y - perp_y,
        tip_x, tip_y
    );

    let south_y = size - 3.0;
    let west_x_text = 8.0;
    let east_x_text = size - 8.0;
    let center_offset = center + 3.0;
    let center_up = center - 8.0;
    let center_down = center + 8.0;

    rsx! {
        svg {
            view_box: "0 0 {size} {size}",
            class: "w-full h-full",
            circle { cx: "{center}", cy: "{center}", r: "{radius}", fill: "none", stroke: "var(--color-border)", stroke_width: "1.5" }
            text { x: "{center}", y: "10", text_anchor: "middle", font_size: "10", font_weight: "bold", fill: "var(--color-muted-foreground)", "N" }
            text { x: "{center}", y: "{south_y}", text_anchor: "middle", font_size: "10", fill: "var(--color-muted-foreground)", "S" }
            text { x: "{west_x_text}", y: "{center_offset}", text_anchor: "middle", font_size: "10", fill: "var(--color-muted-foreground)", "W" }
            text { x: "{east_x_text}", y: "{center_offset}", text_anchor: "middle", font_size: "10", fill: "var(--color-muted-foreground)", "E" }
            path { d: "{arrow_path}", fill: "var(--color-foreground)", opacity: "0.8" }
            text { x: "{center}", y: "{center_up}", text_anchor: "middle", dominant_baseline: "middle", font_size: "11", font_weight: "bold", fill: "var(--color-foreground)", "{speed:.0}" }
            text { x: "{center}", y: "{center_down}", text_anchor: "middle", dominant_baseline: "middle", font_size: "7", fill: "var(--color-muted-foreground)", "{speed_unit_label}" }
        }
    }
}
