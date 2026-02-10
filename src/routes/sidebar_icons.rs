use dioxus::prelude::*;

use crate::stores::sidebar_store::SidebarItem;

/// Helper function to render sidebar icons for dynamic sidebar
pub(super) fn render_sidebar_icon(
    item: &SidebarItem,
    class: &str,
) -> Element {
    match item {
        SidebarItem::Home => {
            rsx! {
                crate::components::icons::HomeIcon { class: class.to_string() }
            }
        }
        SidebarItem::Explore => {
            rsx! {
                crate::components::icons::CompassIcon { class: class.to_string() }
            }
        }
        SidebarItem::Articles => {
            rsx! {
                crate::components::icons::BookOpenIcon { class: class.to_string() }
            }
        }
        SidebarItem::Music => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M9 18V5l12-2v13" }
                    circle { cx: "6", cy: "18", r: "3" }
                    circle { cx: "18", cy: "16", r: "3" }
                }
            }
        }
        SidebarItem::Photos => {
            rsx! {
                crate::components::icons::CameraIcon { class: class.to_string() }
            }
        }
        SidebarItem::Videos => {
            rsx! {
                crate::components::icons::VideoIcon { class: class.to_string() }
            }
        }
        SidebarItem::Live => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" }
                }
            }
        }
        SidebarItem::Notifications => {
            rsx! {
                crate::components::icons::BellIcon { class: class.to_string() }
            }
        }
        SidebarItem::Messages => {
            rsx! {
                crate::components::icons::MailIcon { class: class.to_string() }
            }
        }
        SidebarItem::Bookmarks => {
            rsx! {
                crate::components::icons::BookmarkIcon { class: class.to_string() }
            }
        }
        SidebarItem::Profile => {
            rsx! {
                crate::components::icons::UserIcon { class: class.to_string() }
            }
        }
        SidebarItem::Settings => {
            rsx! {
                crate::components::icons::SettingsIcon { class: class.to_string() }
            }
        }
        SidebarItem::VoiceMessages => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" }
                    path { d: "M19 10v2a7 7 0 0 1-14 0v-2" }
                    line {
                        x1: "12",
                        x2: "12",
                        y1: "19",
                        y2: "22",
                    }
                }
            }
        }
        SidebarItem::Polls => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    rect {
                        x: "3",
                        y: "3",
                        width: "18",
                        height: "18",
                        rx: "2",
                    }
                    line {
                        x1: "3",
                        y1: "9",
                        x2: "21",
                        y2: "9",
                    }
                    line {
                        x1: "9",
                        y1: "21",
                        x2: "9",
                        y2: "9",
                    }
                }
            }
        }
        SidebarItem::WebBookmarks => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z" }
                }
            }
        }
        SidebarItem::Podcasts => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" }
                    path { d: "M19 10v2a7 7 0 0 1-14 0v-2" }
                    line {
                        x1: "12",
                        x2: "12",
                        y1: "19",
                        y2: "22",
                    }
                }
            }
        }
        SidebarItem::Radio => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M4.9 19.1C1 15.2 1 8.8 4.9 4.9" }
                    path { d: "M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5" }
                    circle { cx: "12", cy: "12", r: "2" }
                    path { d: "M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5" }
                    path { d: "M19.1 4.9C23 8.8 23 15.1 19.1 19" }
                }
            }
        }
        SidebarItem::Wallet => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M21 12V7H5a2 2 0 0 1 0-4h14v4" }
                    path { d: "M3 5v14a2 2 0 0 0 2 2h16v-5" }
                    path { d: "M18 12a2 2 0 0 0 0 4h4v-4Z" }
                }
            }
        }
        SidebarItem::P2PTrading => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M16 3l4 4-4 4" }
                    path { d: "M20 7H4" }
                    path { d: "M8 21l-4-4 4-4" }
                    path { d: "M4 17h16" }
                }
            }
        }
        SidebarItem::Communities => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                    circle { cx: "9", cy: "7", r: "4" }
                    path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                    path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
                }
            }
        }
        SidebarItem::Events => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" }
                    circle { cx: "12", cy: "10", r: "3" }
                }
            }
        }
        SidebarItem::Calendar => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    rect {
                        x: "3",
                        y: "4",
                        width: "18",
                        height: "18",
                        rx: "2",
                        ry: "2",
                    }
                    line {
                        x1: "16",
                        y1: "2",
                        x2: "16",
                        y2: "6",
                    }
                    line {
                        x1: "8",
                        y1: "2",
                        x2: "8",
                        y2: "6",
                    }
                    line {
                        x1: "3",
                        y1: "10",
                        x2: "21",
                        y2: "10",
                    }
                }
            }
        }
        SidebarItem::Recipes => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M6 13.87A4 4 0 0 1 7.41 6a5.11 5.11 0 0 1 1.05-1.54 5 5 0 0 1 7.08 0A5.11 5.11 0 0 1 16.59 6 4 4 0 0 1 18 13.87V21H6Z" }
                    line {
                        x1: "6",
                        y1: "17",
                        x2: "18",
                        y2: "17",
                    }
                }
            }
        }
        SidebarItem::PinBoards => {
            rsx! {
                crate::components::icons::PinIcon { class: class.to_string(), filled: false }
            }
        }
        SidebarItem::Trending => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    polyline { points: "23 6 13.5 15.5 8.5 10.5 1 18" }
                    polyline { points: "17 6 23 6 23 12" }
                }
            }
        }
        SidebarItem::Nips => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                    polyline { points: "14 2 14 8 20 8" }
                    line {
                        x1: "16",
                        y1: "13",
                        x2: "8",
                        y2: "13",
                    }
                    line {
                        x1: "16",
                        y1: "17",
                        x2: "8",
                        y2: "17",
                    }
                    polyline { points: "10 9 9 9 8 9" }
                }
            }
        }
        SidebarItem::Badges => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "12", cy: "8", r: "6" }
                    path { d: "M15.477 12.89 17 22l-5-3-5 3 1.523-9.11" }
                }
            }
        }
        SidebarItem::Citations => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V21" }
                    path { d: "M15 21c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3" }
                }
            }
        }
        SidebarItem::Code => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    polyline { points: "16 18 22 12 16 6" }
                    polyline { points: "8 6 2 12 8 18" }
                }
            }
        }
        SidebarItem::Lists => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    line {
                        x1: "8",
                        y1: "6",
                        x2: "21",
                        y2: "6",
                    }
                    line {
                        x1: "8",
                        y1: "12",
                        x2: "21",
                        y2: "12",
                    }
                    line {
                        x1: "8",
                        y1: "18",
                        x2: "21",
                        y2: "18",
                    }
                    line {
                        x1: "3",
                        y1: "6",
                        x2: "3.01",
                        y2: "6",
                    }
                    line {
                        x1: "3",
                        y1: "12",
                        x2: "3.01",
                        y2: "12",
                    }
                    line {
                        x1: "3",
                        y1: "18",
                        x2: "3.01",
                        y2: "18",
                    }
                }
            }
        }
        SidebarItem::Dvm => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    rect {
                        x: "4",
                        y: "4",
                        width: "16",
                        height: "16",
                        rx: "2",
                    }
                    rect {
                        x: "9",
                        y: "9",
                        width: "6",
                        height: "6",
                    }
                    line {
                        x1: "9",
                        y1: "1",
                        x2: "9",
                        y2: "4",
                    }
                    line {
                        x1: "15",
                        y1: "1",
                        x2: "15",
                        y2: "4",
                    }
                    line {
                        x1: "9",
                        y1: "20",
                        x2: "9",
                        y2: "23",
                    }
                    line {
                        x1: "15",
                        y1: "20",
                        x2: "15",
                        y2: "23",
                    }
                    line {
                        x1: "20",
                        y1: "9",
                        x2: "23",
                        y2: "9",
                    }
                    line {
                        x1: "20",
                        y1: "14",
                        x2: "23",
                        y2: "14",
                    }
                    line {
                        x1: "1",
                        y1: "9",
                        x2: "4",
                        y2: "9",
                    }
                    line {
                        x1: "1",
                        y1: "14",
                        x2: "4",
                        y2: "14",
                    }
                }
            }
        }
        SidebarItem::Wiki => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" }
                    path { d: "M8 7h6" }
                    path { d: "M8 11h8" }
                }
            }
        }
        SidebarItem::Publications => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" }
                    path { d: "M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" }
                }
            }
        }
        SidebarItem::Shop => {
            rsx! {
                crate::components::icons::ShoppingBagIcon { class: class.to_string() }
            }
        }
        SidebarItem::Blossom => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" }
                }
            }
        }
        SidebarItem::Bible => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" }
                    path { d: "M12 7v6" }
                    path { d: "M9 10h6" }
                }
            }
        }
        SidebarItem::Highlights => {
            rsx! {
                svg {
                    class: "{class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "m9 11-6 6v3h9l3-3" }
                    path { d: "m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4" }
                }
            }
        }
    }
}
