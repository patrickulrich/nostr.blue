use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn LiveStreamDetail(note_id: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer { address: note_id });
    rsx! {}
}
