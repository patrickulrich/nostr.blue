use crate::components::viewers::citation_viewer::CitationsHome as CitationsHomeViewer;
use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn CitationDetail(naddr: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer { address: naddr });
    rsx! {}
}

#[component]
pub fn CitationsHome() -> Element {
    rsx! { CitationsHomeViewer {} }
}
