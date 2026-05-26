use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn Note(note_id: String, from_voice: Option<String>) -> Element {
    let _ = from_voice;
    let nav = navigator();
    nav.replace(Route::AddressViewer { address: note_id });
    rsx! {}
}
