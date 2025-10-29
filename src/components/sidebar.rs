use dioxus::prelude::*;

#[component]
pub fn Sidebar() -> Element {
    rsx! {
        div {
            class: "sidebar-placeholder",
            "Sidebar placeholder"
        }
    }
}
