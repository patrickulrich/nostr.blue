use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn CodePullDetail(note_id: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer {
        address: crate::utils::nip19_urls::note_route_id(&note_id, None),
    });
    rsx! {}
}
