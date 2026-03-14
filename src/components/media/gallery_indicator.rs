use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct GalleryIndicatorProps {
    pub current_index: usize,
    pub count: usize,
    pub on_select: EventHandler<usize>,
}

#[component]
pub fn GalleryIndicator(props: GalleryIndicatorProps) -> Element {
    if props.count <= 1 {
        return rsx! {};
    }

    rsx! {
        div { class: "flex items-center justify-center gap-2",
            for index in 0..props.count {
                button {
                    key: "{index}",
                    class: if index == props.current_index {
                        "h-2.5 w-2.5 rounded-full bg-white"
                    } else {
                        "h-2.5 w-2.5 rounded-full bg-white/40 hover:bg-white/70 transition"
                    },
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        props.on_select.call(index);
                    },
                    "aria-label": format!("Go to image {}", index + 1),
                }
            }
        }
    }
}
