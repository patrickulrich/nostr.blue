use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn PhotoDetail(photo_id: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer {
        address: crate::utils::nip19_urls::note_route_id(&photo_id, None),
    });
    rsx! {}
}
