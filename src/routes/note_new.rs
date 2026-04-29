use crate::components::note_composer::{NoteComposer, NoteMode};
use dioxus::prelude::*;

#[component]
pub fn NoteNew(quote: Option<String>) -> Element {
    rsx! { NoteComposer { mode: NoteMode::FullPage { quote } } }
}
