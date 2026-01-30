//! P2P Depth Chart Component
//!
//! Native SVG depth chart showing order liquidity distribution by premium percentage.
//! Similar to RoboSats depth chart visualization.

use crate::utils::format::format_sats_with_unit;
use crate::utils::nip69::{OrderStatus, OrderType, P2POrder};
use dioxus::prelude::*;

/// Depth data for charting
#[derive(Clone, Debug)]
struct DepthData {
    /// Sell orders sorted by premium (descending = best deals first)
    sells: Vec<(f64, u64)>, // (premium %, cumulative sats)
    /// Buy orders sorted by premium (ascending = best deals first)
    buys: Vec<(f64, u64)>, // (premium %, cumulative sats)
    /// Maximum cumulative sats (for Y-axis scaling)
    max_cumulative: u64,
}

impl DepthData {
    fn new() -> Self {
        Self {
            sells: Vec::new(),
            buys: Vec::new(),
            max_cumulative: 0,
        }
    }
}

/// Compute depth data from orders
fn compute_depth_data(orders: &[P2POrder]) -> DepthData {
    let mut data = DepthData::new();

    // Filter to pending orders only with valid premium
    let pending_orders: Vec<&P2POrder> = orders
        .iter()
        .filter(|o| o.status == OrderStatus::Pending && o.premium.is_some())
        .collect();

    if pending_orders.is_empty() {
        return data;
    }

    // Separate buy and sell orders
    let mut sell_orders: Vec<&P2POrder> = pending_orders
        .iter()
        .filter(|o| o.order_type == OrderType::Sell)
        .copied()
        .collect();

    let mut buy_orders: Vec<&P2POrder> = pending_orders
        .iter()
        .filter(|o| o.order_type == OrderType::Buy)
        .copied()
        .collect();

    // Sort sell orders by premium ascending (lowest premium first = best deals for buyers)
    // Note: None premiums are treated as 0.0 via unwrap_or
    sell_orders.sort_by(|a, b| {
        a.premium
            .unwrap_or(0.0)
            .partial_cmp(&b.premium.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Sort buy orders by premium descending (best deals = highest premium first for sellers)
    buy_orders.sort_by(|a, b| {
        b.premium
            .unwrap_or(0.0)
            .partial_cmp(&a.premium.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute cumulative sats for sells (skip orders without valid sats to avoid skewing the chart)
    let mut cumulative: u64 = 0;
    for order in &sell_orders {
        if order.amount_sats == 0 {
            continue; // Skip orders without valid sats amount
        }
        cumulative = cumulative.saturating_add(order.amount_sats);
        data.sells.push((order.premium.unwrap_or(0.0), cumulative));
    }
    data.max_cumulative = data.max_cumulative.max(cumulative);

    // Compute cumulative sats for buys (skip orders without valid sats)
    cumulative = 0;
    for order in &buy_orders {
        if order.amount_sats == 0 {
            continue; // Skip orders without valid sats amount
        }
        cumulative = cumulative.saturating_add(order.amount_sats);
        data.buys.push((order.premium.unwrap_or(0.0), cumulative));
    }
    data.max_cumulative = data.max_cumulative.max(cumulative);

    // Ensure minimum for scaling
    if data.max_cumulative == 0 {
        data.max_cumulative = 1;
    }

    data
}

/// Convert premium percentage to X coordinate
/// Clamps premiums outside the visible range to chart edges
fn premium_to_x(premium: f64, width: f64, padding: f64) -> f64 {
    let min_premium = -10.0;
    let max_premium = 20.0;
    let clamped = premium.clamp(min_premium, max_premium);
    padding + (clamped - min_premium) / (max_premium - min_premium) * (width - 2.0 * padding)
}

/// Convert sats to Y coordinate
fn sats_to_y(sats: u64, max_sats: u64, height: f64, padding: f64) -> f64 {
    let baseline = height - padding;
    baseline - (sats as f64 / max_sats as f64) * (height - 2.0 * padding)
}

/// Build SVG path for area fill
fn build_area_path(
    data: &[(f64, u64)],
    width: f64,
    height: f64,
    padding: f64,
    max_sats: u64,
) -> String {
    if data.is_empty() {
        return String::new();
    }

    let baseline = height - padding;

    // Start at baseline
    let first_x = premium_to_x(data[0].0, width, padding);
    let mut path = format!("M {} {}", first_x, baseline);

    // Draw line to first point
    path.push_str(&format!(
        " L {} {}",
        first_x,
        sats_to_y(data[0].1, max_sats, height, padding)
    ));

    // Draw through all points
    for (premium, cumulative) in data.iter().skip(1) {
        let x = premium_to_x(*premium, width, padding);
        let y = sats_to_y(*cumulative, max_sats, height, padding);
        path.push_str(&format!(" L {} {}", x, y));
    }

    // Close path back to baseline
    let last_x = premium_to_x(data.last().unwrap().0, width, padding);
    path.push_str(&format!(" L {} {} Z", last_x, baseline));

    path
}

/// P2P Depth Chart component
#[component]
pub fn P2PDepthChart(orders: Vec<P2POrder>) -> Element {
    // Chart dimensions (viewBox coordinates, not pixels)
    let width = 800.0_f64;
    let height = 400.0_f64;
    let padding = 50.0_f64;

    // Compute depth data
    let depth_data = compute_depth_data(&orders);

    // Check if we have any data
    let has_data = !depth_data.sells.is_empty() || !depth_data.buys.is_empty();

    // Build SVG paths
    let sell_path = build_area_path(
        &depth_data.sells,
        width,
        height,
        padding,
        depth_data.max_cumulative,
    );
    let buy_path = build_area_path(
        &depth_data.buys,
        width,
        height,
        padding,
        depth_data.max_cumulative,
    );

    // Y-axis tick values
    let y_ticks: Vec<u64> = if depth_data.max_cumulative > 0 {
        (0..=4)
            .map(|i| (depth_data.max_cumulative as f64 * i as f64 / 4.0) as u64)
            .collect()
    } else {
        vec![0]
    };

    // Order counts
    let sell_count = depth_data.sells.len();
    let buy_count = depth_data.buys.len();

    rsx! {
        div {
            class: "w-full",

            // Chart container with aspect ratio
            div {
                class: "w-full bg-card border border-border rounded-lg p-4",
                style: "aspect-ratio: 2/1; min-height: 200px;",

                if !has_data {
                    // Empty state
                    div {
                        class: "w-full h-full flex items-center justify-center text-muted-foreground",
                        "No orders with premium data to display"
                    }
                } else {
                    svg {
                        view_box: "0 0 {width} {height}",
                        preserve_aspect_ratio: "xMidYMid meet",
                        class: "w-full h-full",

                        // Background
                        rect {
                            x: "0",
                            y: "0",
                            width: "{width}",
                            height: "{height}",
                            fill: "transparent"
                        }

                        // Grid lines (horizontal)
                        g {
                            class: "grid-lines",
                            for i in 0..5 {
                                {
                                    let y = padding + (i as f64) * (height - 2.0 * padding) / 4.0;
                                    rsx! {
                                        line {
                                            x1: "{padding}",
                                            y1: "{y}",
                                            x2: "{width - padding}",
                                            y2: "{y}",
                                            stroke: "currentColor",
                                            stroke_opacity: "0.1",
                                            stroke_width: "1"
                                        }
                                    }
                                }
                            }
                        }

                        // Grid lines (vertical) at premium intervals
                        g {
                            class: "grid-lines-vertical",
                            for premium in [-10, -5, 0, 5, 10, 15, 20] {
                                {
                                    let x = premium_to_x(premium as f64, width, padding);
                                    rsx! {
                                        line {
                                            x1: "{x}",
                                            y1: "{padding}",
                                            x2: "{x}",
                                            y2: "{height - padding}",
                                            stroke: "currentColor",
                                            stroke_opacity: "0.05",
                                            stroke_width: "1"
                                        }
                                    }
                                }
                            }
                        }

                        // Zero line (more prominent)
                        {
                            let zero_x = premium_to_x(0.0, width, padding);
                            rsx! {
                                line {
                                    x1: "{zero_x}",
                                    y1: "{padding}",
                                    x2: "{zero_x}",
                                    y2: "{height - padding}",
                                    stroke: "currentColor",
                                    stroke_opacity: "0.3",
                                    stroke_width: "1",
                                    stroke_dasharray: "4,4"
                                }
                            }
                        }

                        // Sell orders area (green) - "wants to sell BTC"
                        if !sell_path.is_empty() {
                            path {
                                d: "{sell_path}",
                                fill: "rgba(34, 197, 94, 0.3)",
                                stroke: "rgb(34, 197, 94)",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round"
                            }
                        }

                        // Buy orders area (blue) - "wants to buy BTC"
                        if !buy_path.is_empty() {
                            path {
                                d: "{buy_path}",
                                fill: "rgba(59, 130, 246, 0.3)",
                                stroke: "rgb(59, 130, 246)",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round"
                            }
                        }

                        // X-axis labels (premium %)
                        g {
                            class: "x-axis-labels",
                            for premium in [-10i32, -5, 0, 5, 10, 15, 20] {
                                {
                                    let x = premium_to_x(premium as f64, width, padding);
                                    let label = if premium >= 0 {
                                        format!("+{}%", premium)
                                    } else {
                                        format!("{}%", premium)
                                    };
                                    rsx! {
                                        text {
                                            x: "{x}",
                                            y: "{height - 15.0}",
                                            text_anchor: "middle",
                                            font_size: "11",
                                            fill: "currentColor",
                                            fill_opacity: "0.6",
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }

                        // X-axis title
                        text {
                            x: "{width / 2.0}",
                            y: "{height - 2.0}",
                            text_anchor: "middle",
                            font_size: "12",
                            fill: "currentColor",
                            fill_opacity: "0.6",
                            "Premium"
                        }

                        // Y-axis labels (cumulative sats)
                        g {
                            class: "y-axis-labels",
                            for (i, sats) in y_ticks.iter().enumerate() {
                                {
                                    let y = height - padding - (i as f64) * (height - 2.0 * padding) / 4.0;
                                    let label = format_sats_with_unit(*sats);
                                    rsx! {
                                        text {
                                            x: "{padding - 5.0}",
                                            y: "{y + 4.0}",
                                            text_anchor: "end",
                                            font_size: "10",
                                            fill: "currentColor",
                                            fill_opacity: "0.6",
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }

                        // Legend
                        g {
                            class: "legend",
                            transform: "translate({width - 140.0}, 20)",

                            // Sell legend
                            rect {
                                x: "0",
                                y: "0",
                                width: "12",
                                height: "12",
                                fill: "rgb(34, 197, 94)"
                            }
                            text {
                                x: "18",
                                y: "10",
                                font_size: "11",
                                fill: "currentColor",
                                "Sell ({sell_count})"
                            }

                            // Buy legend
                            rect {
                                x: "0",
                                y: "20",
                                width: "12",
                                height: "12",
                                fill: "rgb(59, 130, 246)"
                            }
                            text {
                                x: "18",
                                y: "30",
                                font_size: "11",
                                fill: "currentColor",
                                "Buy ({buy_count})"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Skeleton loader for depth chart
#[component]
pub fn P2PDepthChartSkeleton() -> Element {
    rsx! {
        div {
            class: "w-full bg-card border border-border rounded-lg p-4 animate-pulse",
            style: "aspect-ratio: 2/1; min-height: 200px;",
            div {
                class: "w-full h-full flex items-center justify-center",
                div {
                    class: "h-32 w-3/4 bg-muted rounded"
                }
            }
        }
    }
}
