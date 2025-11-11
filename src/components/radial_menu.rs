use dioxus::prelude::*;
use crate::components::icons::*;

#[derive(Props, Clone, PartialEq)]
pub struct RadialMenuProps {
    pub is_open: bool,
    pub on_close: EventHandler<()>,
    pub on_note_click: EventHandler<()>,
    pub on_article_click: EventHandler<()>,
    pub on_photo_click: EventHandler<()>,
    pub on_video_landscape_click: EventHandler<()>,
    pub on_video_portrait_click: EventHandler<()>,
    pub on_voice_click: EventHandler<()>,
    pub on_poll_click: EventHandler<()>,
}

#[component]
pub fn RadialMenu(props: RadialMenuProps) -> Element {
    // Calculate positions for 7 buttons in a circle
    // Starting from left (180 degrees) and going counter-clockwise
    let radius = 100; // pixels from center

    // Button positions (angle in degrees, then converted to radians)
    let positions = [
        (180.0, "Note"),           // Left
        (225.0, "Article"),        // Bottom-left
        (270.0, "Photo"),          // Bottom
        (315.0, "Video"),          // Bottom-right
        (0.0, "Shorts"),           // Right
        (135.0, "Voice"),          // Top-left
        (90.0, "Poll"),            // Top
    ];

    let calculate_position = |angle: f64| -> (i32, i32) {
        let radians = angle.to_radians();
        let x = (radians.cos() * radius as f64) as i32;
        let y = (radians.sin() * radius as f64) as i32;
        (x, y)
    };

    let render_radial_button = |position: f64, icon: Element, color_class: &str, title: &str, on_click: EventHandler<()>| {
        let (x, y) = calculate_position(position);
        rsx! {
            button {
                class: "absolute w-14 h-14 rounded-full {color_class} text-white shadow-lg flex items-center justify-center transition-all duration-300 z-50 pointer-events-auto opacity-100 scale-100",
                style: format!("left: 50%; top: 50%; transform: translate(calc(-50% + {}px), calc(-50% + {}px));", x, y),
                onclick: move |e| {
                    e.stop_propagation();
                    on_click.call(());
                },
                title: "{title}",
                {icon}
            }
        }
    };

    rsx! {
        // Backdrop overlay
        if props.is_open {
            div {
                class: "fixed inset-0 z-40",
                onclick: move |_| props.on_close.call(()),
            }
        }

        // Radial button container - only render when open
        if props.is_open {
            div {
                class: "absolute inset-0 pointer-events-none",

                // Note button (left)
                {render_radial_button(
                    positions[0].0,
                    rsx! { MessageCircleIcon { class: "w-6 h-6".to_string() } },
                    "bg-gradient-to-br from-purple-500 to-purple-600 hover:from-purple-600 hover:to-purple-700",
                    "Create Note",
                    props.on_note_click
                )}

                // Article button (bottom-left)
                {render_radial_button(
                    positions[1].0,
                    rsx! { BookOpenIcon { class: "w-6 h-6".to_string() } },
                    "bg-gradient-to-br from-blue-500 to-blue-600 hover:from-blue-600 hover:to-blue-700",
                    "Write Article",
                    props.on_article_click
                )}

                // Photo button (bottom)
                {render_radial_button(
                    positions[2].0,
                    rsx! { CameraIcon { class: "w-6 h-6".to_string() } },
                    "bg-gradient-to-br from-pink-500 to-pink-600 hover:from-pink-600 hover:to-pink-700",
                    "Share Photo",
                    props.on_photo_click
                )}

                // Video Landscape button (bottom-right)
                {render_radial_button(
                    positions[3].0,
                    rsx! { VideoIcon { class: "w-6 h-6".to_string() } },
                    "bg-gradient-to-br from-red-500 to-red-600 hover:from-red-600 hover:to-red-700",
                    "Upload Video",
                    props.on_video_landscape_click
                )}

                // Video Portrait button (right)
                {render_radial_button(
                    positions[4].0,
                    rsx! { FileVideoIcon { class: "w-6 h-6".to_string() } },
                    "bg-gradient-to-br from-orange-500 to-orange-600 hover:from-orange-600 hover:to-orange-700",
                    "Create Short",
                    props.on_video_portrait_click
                )}

                // Voice Message button (top-left)
                {render_radial_button(
                    positions[5].0,
                    rsx! {
                        svg {
                            class: "w-6 h-6",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            xmlns: "http://www.w3.org/2000/svg",
                            path { d: "M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z" }
                            path { d: "M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z" }
                        }
                    },
                    "bg-gradient-to-br from-amber-500 to-amber-600 hover:from-amber-600 hover:to-amber-700",
                    "Record Voice Message",
                    props.on_voice_click
                )}

                // Poll button (top)
                {render_radial_button(
                    positions[6].0,
                    rsx! {
                        svg {
                            class: "w-6 h-6",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            xmlns: "http://www.w3.org/2000/svg",
                            path { d: "M3 13h8V3H3v10zm0 8h8v-6H3v6zm10 0h8V11h-8v10zm0-18v6h8V3h-8z" }
                        }
                    },
                    "bg-gradient-to-br from-green-500 to-green-600 hover:from-green-600 hover:to-green-700",
                    "Create Poll",
                    props.on_poll_click
                )}
            }
        }
    }
}
