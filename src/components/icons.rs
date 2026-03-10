use dioxus::prelude::*;
/// Icon size prop
#[derive(Props, Clone, PartialEq)]
pub struct IconProps {
    #[props(default = "w-7 h-7".to_string())]
    pub class: String,
    #[props(default = false)]
    pub filled: bool,
}
#[component]
pub fn HomeIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" }
            polyline { points: "9 22 9 12 15 12 15 22" }
        }
    }
}
#[component]
pub fn CompassIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
            polygon { points: "16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76" }
        }
    }
}
#[component]
pub fn BookOpenIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
#[component]
pub fn BellIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9" }
            path { d: "M10.3 21a1.94 1.94 0 0 0 3.4 0" }
        }
    }
}
#[component]
pub fn MailIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                width: "20",
                height: "16",
                x: "2",
                y: "4",
                rx: "2",
            }
            path { d: "m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7" }
        }
    }
}
#[component]
pub fn ZapIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polygon { points: "13 2 3 14 12 14 11 22 21 10 12 10 13 2" }
        }
    }
}
#[component]
pub fn ListIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                x2: "21",
                y1: "6",
                y2: "6",
            }
            line {
                x1: "8",
                x2: "21",
                y1: "12",
                y2: "12",
            }
            line {
                x1: "8",
                x2: "21",
                y1: "18",
                y2: "18",
            }
            line {
                x1: "3",
                x2: "3.01",
                y1: "6",
                y2: "6",
            }
            line {
                x1: "3",
                x2: "3.01",
                y1: "12",
                y2: "12",
            }
            line {
                x1: "3",
                x2: "3.01",
                y1: "18",
                y2: "18",
            }
        }
    }
}
/// Grid layout icon (lucide-react: layout-grid)
#[component]
pub fn GridIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                width: "7",
                height: "7",
                rx: "1",
            }
            rect {
                x: "14",
                y: "3",
                width: "7",
                height: "7",
                rx: "1",
            }
            rect {
                x: "3",
                y: "14",
                width: "7",
                height: "7",
                rx: "1",
            }
            rect {
                x: "14",
                y: "14",
                width: "7",
                height: "7",
                rx: "1",
            }
        }
    }
}
#[component]
pub fn BookmarkIcon(props: IconProps) -> Element {
    let fill_value = if props.filled { "currentColor" } else { "none" };
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "{fill_value}",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z" }
        }
    }
}
#[component]
pub fn UsersIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
#[component]
pub fn UserIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" }
            circle { cx: "12", cy: "7", r: "4" }
        }
    }
}
#[component]
pub fn SettingsIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" }
            circle { cx: "12", cy: "12", r: "3" }
        }
    }
}
#[component]
pub fn MoreHorizontalIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            circle { cx: "12", cy: "12", r: "1" }
            circle { cx: "19", cy: "12", r: "1" }
            circle { cx: "5", cy: "12", r: "1" }
        }
    }
}
#[component]
pub fn PenSquareIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" }
            path { d: "M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4Z" }
        }
    }
}
#[component]
pub fn VideoIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5" }
            rect {
                x: "2",
                y: "6",
                width: "14",
                height: "12",
                rx: "2",
            }
        }
    }
}
#[component]
pub fn CalendarIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                width: "18",
                height: "18",
                x: "3",
                y: "4",
                rx: "2",
                ry: "2",
            }
            line {
                x1: "16",
                x2: "16",
                y1: "2",
                y2: "6",
            }
            line {
                x1: "8",
                x2: "8",
                y1: "2",
                y2: "6",
            }
            line {
                x1: "3",
                x2: "21",
                y1: "10",
                y2: "10",
            }
        }
    }
}
#[component]
pub fn MusicIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
#[component]
pub fn CameraIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" }
            circle { cx: "12", cy: "13", r: "3" }
        }
    }
}
#[component]
pub fn BarChartIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                x1: "12",
                y1: "20",
                x2: "12",
                y2: "10",
            }
            line {
                x1: "18",
                y1: "20",
                x2: "18",
                y2: "4",
            }
            line {
                x1: "6",
                y1: "20",
                x2: "6",
                y2: "16",
            }
        }
    }
}
#[component]
pub fn HeartIcon(props: IconProps) -> Element {
    let fill_value = if props.filled { "currentColor" } else { "none" };
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "{fill_value}",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" }
        }
    }
}
#[component]
pub fn MessageCircleIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M7.9 20A9 9 0 1 0 4 16.1L2 22Z" }
        }
    }
}
#[component]
pub fn Repeat2Icon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m2 9 3-3 3 3" }
            path { d: "M13 18H7a2 2 0 0 1-2-2V6" }
            path { d: "m22 15-3 3-3-3" }
            path { d: "M11 6h6a2 2 0 0 1 2 2v10" }
        }
    }
}
#[component]
pub fn ShareIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8" }
            polyline { points: "16 6 12 2 8 6" }
            line {
                x1: "12",
                x2: "12",
                y1: "2",
                y2: "15",
            }
        }
    }
}
#[component]
pub fn RefreshIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M21 2v6h-6" }
            path { d: "M3 12a9 9 0 0 1 15-6.7L21 8" }
            path { d: "M3 22v-6h6" }
            path { d: "M21 12a9 9 0 0 1-15 6.7L3 16" }
        }
    }
}
#[component]
pub fn ChevronDownIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polyline { points: "6 9 12 15 18 9" }
        }
    }
}
#[component]
pub fn ChevronUpIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polyline { points: "18 15 12 9 6 15" }
        }
    }
}
#[component]
pub fn VolumeIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polygon { points: "11 5 6 9 2 9 2 15 6 15 11 19 11 5" }
            path { d: "M15.54 8.46a5 5 0 0 1 0 7.07" }
            path { d: "M19.07 4.93a10 10 0 0 1 0 14.14" }
        }
    }
}
#[component]
pub fn VolumeXIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polygon { points: "11 5 6 9 2 9 2 15 6 15 11 19 11 5" }
            line {
                x1: "23",
                y1: "9",
                x2: "17",
                y2: "15",
            }
            line {
                x1: "17",
                y1: "9",
                x2: "23",
                y2: "15",
            }
        }
    }
}
#[component]
pub fn CheckIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polyline { points: "20 6 9 17 4 12" }
        }
    }
}
#[component]
pub fn AlertTriangleIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m21.73 18-8-14a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" }
            line {
                x1: "12",
                x2: "12",
                y1: "9",
                y2: "13",
            }
            line {
                x1: "12",
                x2: "12.01",
                y1: "17",
                y2: "17",
            }
        }
    }
}
#[component]
pub fn ClockIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
            polyline { points: "12 6 12 12 16 14" }
        }
    }
}
#[component]
pub fn PlayIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polygon { points: "5 3 19 12 5 21 5 3" }
        }
    }
}
#[component]
pub fn DiscIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
            circle { cx: "12", cy: "12", r: "3" }
        }
    }
}
#[component]
pub fn ArrowLeftIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                x1: "19",
                y1: "12",
                x2: "5",
                y2: "12",
            }
            polyline { points: "12 19 5 12 12 5" }
        }
    }
}
#[component]
pub fn CopyIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                width: "14",
                height: "14",
                x: "8",
                y: "8",
                rx: "2",
                ry: "2",
            }
            path { d: "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" }
        }
    }
}
#[component]
pub fn SendIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                x1: "22",
                y1: "2",
                x2: "11",
                y2: "13",
            }
            polygon { points: "22 2 15 22 11 13 2 9 22 2" }
        }
    }
}
#[component]
pub fn SearchIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            circle { cx: "11", cy: "11", r: "8" }
            path { d: "m21 21-4.3-4.3" }
        }
    }
}
#[component]
pub fn Link2Icon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M9 17H7A5 5 0 0 1 7 7h2" }
            path { d: "M15 7h2a5 5 0 1 1 0 10h-2" }
            line {
                x1: "8",
                y1: "12",
                x2: "16",
                y2: "12",
            }
        }
    }
}
#[component]
pub fn FileVideoIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
            polyline { points: "14 2 14 8 20 8" }
            path { d: "m10 15.5 4 2.5v-6l-4 2.5" }
        }
    }
}
#[component]
pub fn HashIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
                x1: "4",
                y1: "9",
                x2: "20",
                y2: "9",
            }
            line {
                x1: "4",
                y1: "15",
                x2: "20",
                y2: "15",
            }
            line {
                x1: "10",
                y1: "3",
                x2: "8",
                y2: "21",
            }
            line {
                x1: "16",
                y1: "3",
                x2: "14",
                y2: "21",
            }
        }
    }
}
#[component]
pub fn InfoIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
            path { d: "M12 16v-4" }
            path { d: "M12 8h.01" }
        }
    }
}
pub const PLAY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>"#;
pub const PLAY_SMALL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="currentColor" stroke="none"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>"#;
pub const PAUSE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="4" width="4" height="16"></rect><rect x="14" y="4" width="4" height="16"></rect></svg>"#;
pub const SKIP_FORWARD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 4 15 12 5 20 5 4"></polygon><line x1="19" y1="5" x2="19" y2="19"></line></svg>"#;
pub const SKIP_BACK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="19 20 9 12 19 4 19 20"></polygon><line x1="5" y1="19" x2="5" y2="5"></line></svg>"#;
pub const VOLUME_2: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M15.54 8.46a5 5 0 0 1 0 7.07"></path><path d="M19.07 4.93a10 10 0 0 1 0 14.14"></path></svg>"#;
pub const VOLUME_X: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line></svg>"#;
pub const X: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>"#;
pub const HEART: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"></path></svg>"#;
pub const ZAP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon></svg>"#;
pub const SEARCH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"></circle><line x1="21" y1="21" x2="16.65" y2="16.65"></line></svg>"#;
pub const ARROW_LEFT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"></line><polyline points="12 19 5 12 12 5"></polyline></svg>"#;
pub const SHARE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="5" r="3"></circle><circle cx="6" cy="12" r="3"></circle><circle cx="18" cy="19" r="3"></circle><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"></line><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"></line></svg>"#;
pub const EXTERNAL_LINK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path><polyline points="15 3 21 3 21 9"></polyline><line x1="10" y1="14" x2="21" y2="3"></line></svg>"#;
pub const USER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path><circle cx="12" cy="7" r="4"></circle></svg>"#;
pub const MUSIC_NOTE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"></path><circle cx="6" cy="18" r="3"></circle><circle cx="18" cy="16" r="3"></circle></svg>"#;
pub const BOOK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"></path><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"></path></svg>"#;
pub const FILE_TEXT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line><polyline points="10 9 9 9 8 9"></polyline></svg>"#;
pub const BITCOIN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11.767 19.089c4.924.868 6.14-6.025 1.216-6.894m-1.216 6.894L5.86 18.047m5.908 1.042-.347 1.97m1.563-8.864c4.924.869 6.14-6.025 1.215-6.893m-1.215 6.893-3.94-.694m5.155-6.2L8.29 4.26m5.908 1.042.348-1.97M7.48 20.364l3.126-17.727"></path></svg>"#;
pub const CHEVRON_UP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="18 15 12 9 6 15"></polyline></svg>"#;
pub const CHEVRON_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"></polyline></svg>"#;
pub const PODCAST: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="11" r="1"></circle><path d="M11 17a1 1 0 0 1 2 0c0 .5-.34 3-.5 4.5a.5.5 0 0 1-1 0c-.16-1.5-.5-4-.5-4.5Z"></path><path d="M8 14a5 5 0 1 1 8 0"></path><path d="M17 18.5a9 9 0 1 0-10 0"></path></svg>"#;
pub const MAP_PIN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0Z"></path><circle cx="12" cy="10" r="3"></circle></svg>"#;
pub const FILM: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"></rect><line x1="7" y1="2" x2="7" y2="22"></line><line x1="17" y1="2" x2="17" y2="22"></line><line x1="2" y1="12" x2="22" y2="12"></line><line x1="2" y1="7" x2="7" y2="7"></line><line x1="2" y1="17" x2="7" y2="17"></line><line x1="17" y1="17" x2="22" y2="17"></line><line x1="17" y1="7" x2="22" y2="7"></line></svg>"#;
pub const REWIND_15: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 17a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v1"></path><path d="m14 12 3-3v6"></path><path d="M9 6.8V2m-2 0v4.8"></path><path d="M4 7a8 8 0 0 1 8-1"></path><path d="M20 12a8 8 0 1 1-3-6.3"></path></svg>"#;
pub const FORWARD_15: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 17a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v1"></path><path d="m14 12 3-3v6"></path><path d="M15 6.8V2m2 0v4.8"></path><path d="M4 12a8 8 0 0 1 3-6.3"></path><path d="M20 7a8 8 0 0 0-8-1"></path><path d="M4 12a8 8 0 1 0 3-6.3"></path></svg>"#;
pub const GAUGE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 14 4-4"></path><path d="M3.34 19a10 10 0 1 1 17.32 0"></path></svg>"#;
/// Check mark icon (for confirmation states)
#[allow(dead_code)]
pub const CHECK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"></path></svg>"#;
pub const MIC: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"></path><path d="M19 10v2a7 7 0 0 1-14 0v-2"></path><line x1="12" x2="12" y1="19" y2="22"></line></svg>"#;
pub const AT_SIGN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"></circle><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-3.92 7.94"></path></svg>"#;
pub const LOADER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="2" x2="12" y2="6"></line><line x1="12" y1="18" x2="12" y2="22"></line><line x1="4.93" y1="4.93" x2="7.76" y2="7.76"></line><line x1="16.24" y1="16.24" x2="19.07" y2="19.07"></line><line x1="2" y1="12" x2="6" y2="12"></line><line x1="18" y1="12" x2="22" y2="12"></line><line x1="4.93" y1="19.07" x2="7.76" y2="16.24"></line><line x1="16.24" y1="7.76" x2="19.07" y2="4.93"></line></svg>"#;
pub const SETTINGS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path><circle cx="12" cy="12" r="3"></circle></svg>"#;
pub const EYE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z"></path><circle cx="12" cy="12" r="3"></circle></svg>"#;
pub const STAR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>"#;
pub const STAR_FILLED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>"#;
pub const RADIO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4.9 19.1C1 15.2 1 8.8 4.9 4.9"></path><path d="M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5"></path><circle cx="12" cy="12" r="2"></circle><path d="M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5"></path><path d="M19.1 4.9C23 8.8 23 15.1 19.1 19"></path></svg>"#;
pub const LOCK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="11" x="3" y="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>"#;
pub const TRASH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"></path><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"></path><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"></path></svg>"#;
pub const PLUS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" x2="12" y1="5" y2="19"></line><line x1="5" x2="19" y1="12" y2="12"></line></svg>"#;
pub const GIT_FORK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="18" r="3"></circle><circle cx="6" cy="6" r="3"></circle><circle cx="18" cy="6" r="3"></circle><path d="M18 9v2c0 .6-.4 1-1 1H7c-.6 0-1-.4-1-1V9"></path><path d="M12 12v3"></path></svg>"#;
/// Pin icon for pinned notes (NIP-51)
#[component]
pub fn PinIcon(props: IconProps) -> Element {
    let fill_value = if props.filled { "currentColor" } else { "none" };
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "{fill_value}",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M12 17v5" }
            path { d: "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z" }
        }
    }
}
/// Mastodon/Fediverse icon (ActivityPub protocol)
#[component]
pub fn MastodonIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "currentColor",
            path { d: "M21.327 8.566c0-4.339-2.843-5.61-2.843-5.61-1.433-.658-3.894-.935-6.451-.956h-.063c-2.557.021-5.016.298-6.45.956 0 0-2.843 1.272-2.843 5.61 0 .993-.019 2.181.012 3.441.103 4.243.778 8.425 4.701 9.463 1.809.479 3.362.579 4.612.51 2.268-.126 3.541-.809 3.541-.809l-.075-1.646s-1.621.511-3.441.449c-1.804-.062-3.707-.194-3.999-2.409a4.523 4.523 0 0 1-.04-.621s1.77.433 4.014.536c1.372.063 2.658-.08 3.965-.236 2.506-.299 4.688-1.843 4.962-3.254.434-2.223.398-5.424.398-5.424zm-3.353 5.59h-2.081V9.057c0-1.075-.452-1.62-1.357-1.62-1 0-1.501.647-1.501 1.927v2.791h-2.069V9.364c0-1.28-.501-1.927-1.502-1.927-.905 0-1.357.546-1.357 1.62v5.099H6.026V8.903c0-1.074.273-1.927.823-2.558.566-.631 1.307-.955 2.228-.955 1.065 0 1.872.41 2.405 1.228l.518.869.519-.869c.533-.818 1.34-1.228 2.405-1.228.92 0 1.662.324 2.228.955.549.631.822 1.484.822 2.558v5.253z" }
        }
    }
}
/// Bluesky icon (AT Protocol)
#[component]
pub fn BlueskyIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "currentColor",
            path { d: "M12 10.8c-1.087-2.114-4.046-6.053-6.798-7.995C2.566.944 1.561 1.266.902 1.565.139 1.908 0 3.08 0 3.768c0 .69.378 5.65.624 6.479.815 2.736 3.713 3.66 6.383 3.364.136-.02.275-.039.415-.056-.138.022-.276.04-.415.056-3.912.58-7.387 2.005-2.83 7.078 5.013 5.19 6.87-1.113 7.823-4.308.953 3.195 2.05 9.271 7.733 4.308 4.267-4.308 1.172-6.498-2.74-7.078a8.741 8.741 0 0 1-.415-.056c.14.017.279.036.415.056 2.67.297 5.568-.628 6.383-3.364.246-.828.624-5.79.624-6.478 0-.69-.139-1.861-.902-2.206-.659-.298-1.664-.62-4.3 1.24C16.046 4.748 13.087 8.687 12 10.8Z" }
        }
    }
}
/// RSS feed icon
#[component]
pub fn RssIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M4 11a9 9 0 0 1 9 9" }
            path { d: "M4 4a16 16 0 0 1 16 16" }
            circle { cx: "5", cy: "19", r: "1" }
        }
    }
}
/// Globe/Web icon
#[component]
pub fn GlobeIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
            path { d: "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" }
            path { d: "M2 12h20" }
        }
    }
}
/// External link icon
#[component]
pub fn ExternalLinkIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M15 3h6v6" }
            path { d: "M10 14 21 3" }
            path { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" }
        }
    }
}
/// Git merge icon (for merge requests)
#[component]
pub fn GitMergeIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
    }
}
/// Maximize icon (two diagonal arrows)
#[component]
pub fn MaximizeIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polyline { points: "15 3 21 3 21 9" }
            polyline { points: "9 21 3 21 3 15" }
            line { x1: "21", y1: "3", x2: "14", y2: "10" }
            line { x1: "3", y1: "21", x2: "10", y2: "14" }
        }
    }
}
/// X/Close icon
#[component]
pub fn XIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M18 6 6 18" }
            path { d: "m6 6 12 12" }
        }
    }
}
/// Comment bubble icon (for issue/PR comments)
#[component]
pub fn CommentIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
        }
    }
}
/// Shopping cart icon (for marketplace)
#[component]
pub fn ShoppingCartIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            circle { cx: "9", cy: "21", r: "1" }
            circle { cx: "20", cy: "21", r: "1" }
            path { d: "M1 1h4l2.68 13.39a2 2 0 0 0 2 1.61h9.72a2 2 0 0 0 2-1.61L23 6H6" }
        }
    }
}
/// Shopping bag icon (for marketplace sidebar)
#[component]
pub fn ShoppingBagIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4Z" }
            path { d: "M3 6h18" }
            path { d: "M16 10a4 4 0 0 1-8 0" }
        }
    }
}
#[component]
pub fn FilterIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polygon { points: "22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3" }
        }
    }
}
#[component]
pub fn EditIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" }
            path { d: "M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" }
        }
    }
}
#[component]
pub fn TrashIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polyline { points: "3 6 5 6 21 6" }
            path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
            line {
                x1: "10",
                y1: "11",
                x2: "10",
                y2: "17",
            }
            line {
                x1: "14",
                y1: "11",
                x2: "14",
                y2: "17",
            }
        }
    }
}
/// Download icon - arrow pointing down into tray
#[component]
pub fn DownloadIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
            polyline { points: "7 10 12 15 17 10" }
            line {
                x1: "12",
                x2: "12",
                y1: "15",
                y2: "3",
            }
        }
    }
}
/// File icon - generic document
#[component]
pub fn FileIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
            polyline { points: "14 2 14 8 20 8" }
        }
    }
}
/// FileText icon - document with lines
#[component]
pub fn FileTextIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
            polyline { points: "14 2 14 8 20 8" }
            line {
                x1: "16",
                x2: "8",
                y1: "13",
                y2: "13",
            }
            line {
                x1: "16",
                x2: "8",
                y1: "17",
                y2: "17",
            }
            line {
                x1: "10",
                x2: "8",
                y1: "9",
                y2: "9",
            }
        }
    }
}
/// Printer icon - for print to PDF
#[component]
pub fn PrinterIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polyline { points: "6 9 6 2 18 2 18 9" }
            path { d: "M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2" }
            rect {
                width: "12",
                height: "8",
                x: "6",
                y: "14",
            }
        }
    }
}
/// Nostr.blue mini logo - small "N" in blue circle for minicard previews
#[component]
pub fn NostrBlueMiniLogo(#[props(default = "w-3 h-3".to_string())] class: String) -> Element {
    rsx! {
        svg {
            class: "{class} opacity-50 hover:opacity-100 cursor-pointer transition-opacity",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 100 100",
            circle {
                cx: "50",
                cy: "50",
                r: "50",
                fill: "#3b82f6",
            }
            text {
                x: "50",
                y: "70",
                font_family: "Arial, sans-serif",
                font_size: "60",
                font_weight: "bold",
                fill: "#fff",
                text_anchor: "middle",
                "N"
            }
        }
    }
}
/// Live video icon (video camera with recording indicator)
#[component]
pub fn LiveVideoIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" }
        }
    }
}
/// Microphone icon (for voice messages and podcasts)
#[component]
pub fn MicrophoneIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Poll icon (grid/table layout)
#[component]
pub fn PollIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Radio/broadcast icon (signal waves)
#[component]
pub fn RadioIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Wallet icon
#[component]
pub fn WalletIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Arrows exchange icon (P2P trading)
#[component]
pub fn ArrowsExchangeIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Map pin icon (for events/locations)
#[component]
pub fn MapPinIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Chef hat icon (for recipes)
#[component]
pub fn ChefHatIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Trending up icon (chart with upward arrow)
#[component]
pub fn TrendingUpIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Award/badge icon (medal with ribbon)
#[component]
pub fn AwardIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Quote icon (quotation marks)
#[component]
pub fn QuoteIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Code bracket icon (angle brackets for code)
#[component]
pub fn CodeBracketIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// CPU/chip icon (for DVM/data vending machines)
#[component]
pub fn CpuIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Book icon (closed book with bookmark)
#[component]
pub fn BookIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Cloud icon (for Blossom storage)
#[component]
pub fn CloudIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Bible icon (book with cross)
#[component]
pub fn BibleIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
/// Chat bubble icon (message square) — delegates to CommentIcon (identical SVG, different semantic name)
#[component]
pub fn ChatBubbleIcon(props: IconProps) -> Element {
    CommentIcon(props)
}
/// Highlighter icon (marker pen)
#[component]
pub fn HighlighterIcon(props: IconProps) -> Element {
    rsx! {
        svg {
            class: "{props.class}",
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
