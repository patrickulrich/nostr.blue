use dioxus::prelude::*;

#[component]
pub fn NoteDisplay() -> Element {
    rsx! {
        div {
            class: "note-placeholder",
            "Note component placeholder"
        }
    }
}
