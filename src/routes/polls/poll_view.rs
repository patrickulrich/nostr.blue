use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn PollView(noteid: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer { address: noteid });
    rsx! {}
}
