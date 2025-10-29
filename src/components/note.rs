use dioxus::prelude::*;

#[component]
pub fn Note() -> Element {
    rsx! {
        div {
            class: "note-placeholder",
            "Note component placeholder"
        }
    }
}
