use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn PackDetail(naddr: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer { address: naddr });
    rsx! {}
}
