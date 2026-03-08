//! Code Status Badge Component
//!
//! Displays status badges for issues and pull requests.
use crate::utils::nip34::IssueStatus;
use dioxus::prelude::*;
/// Size variants for the badge
#[derive(Clone, Copy, PartialEq, Default)]
#[allow(dead_code)]
pub enum BadgeSize {
    Small,
    #[default]
    Default,
    Large,
}
/// Status badge component
#[component]
pub fn CodeStatusBadge(status: IssueStatus, #[props(default)] size: BadgeSize) -> Element {
    let (bg_class, text, icon) = match status {
        IssueStatus::Open => (
            "bg-green-500/10 text-green-500 border-green-500/20",
            "Open",
            rsx! {
                svg {
                    class: "w-3.5 h-3.5",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "12", cy: "12", r: "10" }
                }
            },
        ),
        IssueStatus::Applied => (
            "bg-purple-500/10 text-purple-500 border-purple-500/20",
            "Merged",
            rsx! {
                svg {
                    class: "w-3.5 h-3.5",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "18", cy: "18", r: "3" }
                    circle { cx: "6", cy: "6", r: "3" }
                    path { d: "M6 21V9a9 9 0 0 0 9 9" }
                }
            },
        ),
        IssueStatus::Closed => (
            "bg-red-500/10 text-red-500 border-red-500/20",
            "Closed",
            rsx! {
                svg {
                    class: "w-3.5 h-3.5",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "12", cy: "12", r: "10" }
                    line {
                        x1: "15",
                        y1: "9",
                        x2: "9",
                        y2: "15",
                    }
                    line {
                        x1: "9",
                        y1: "9",
                        x2: "15",
                        y2: "15",
                    }
                }
            },
        ),
        IssueStatus::Draft => (
            "bg-yellow-500/10 text-yellow-500 border-yellow-500/20",
            "Draft",
            rsx! {
                svg {
                    class: "w-3.5 h-3.5",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path {
                        d: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z",
                        stroke_dasharray: "4 4",
                    }
                }
            },
        ),
    };
    let size_class = match size {
        BadgeSize::Small => "px-1.5 py-0.5 text-xs gap-1",
        BadgeSize::Default => "px-2 py-1 text-sm gap-1.5",
        BadgeSize::Large => "px-3 py-1.5 text-base gap-2",
    };
    rsx! {
        span { class: "inline-flex items-center rounded-full border font-medium {bg_class} {size_class}",
            {icon}
            span { "{text}" }
        }
    }
}
/// Simple text-only status indicator
#[allow(dead_code)]
#[component]
pub fn CodeStatusText(status: IssueStatus) -> Element {
    rsx! {
        span { class: "font-medium {status_color_class(status)}", "{status_text(status)}" }
    }
}
/// Get the color class for a status (for use in custom styling)
pub fn status_color_class(status: IssueStatus) -> &'static str {
    match status {
        IssueStatus::Open => "text-green-500",
        IssueStatus::Applied => "text-purple-500",
        IssueStatus::Closed => "text-red-500",
        IssueStatus::Draft => "text-yellow-500",
    }
}
/// Get the display text for a status
pub fn status_text(status: IssueStatus) -> &'static str {
    match status {
        IssueStatus::Open => "Open",
        IssueStatus::Applied => "Merged",
        IssueStatus::Closed => "Closed",
        IssueStatus::Draft => "Draft",
    }
}
