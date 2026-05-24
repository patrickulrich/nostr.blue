use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn WikiAuthor(pubkey: String) -> Element {
    let nav = navigator();
    let address = if pubkey.starts_with("npub") || pubkey.starts_with("nprofile") {
        pubkey
    } else {
        crate::utils::nip19_urls::profile_route_id(&pubkey)
    };
    nav.replace(Route::AddressViewer { address });
    rsx! {}
}
