use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn VideoDetail(video_id: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer {
        address: crate::utils::nip19_urls::note_route_id(&video_id, None),
    });
    rsx! {}
}
