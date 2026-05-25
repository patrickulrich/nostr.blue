use crate::routes::Route;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[component]
pub fn CommunityPage(naddr: String) -> Element {
    let nav = navigator();
    let address = if naddr.starts_with("naddr1") {
        naddr
    } else if naddr.contains(':') {
        Coordinate::parse(&naddr)
            .ok()
            .and_then(|c| c.to_bech32().ok())
            .unwrap_or(naddr)
    } else {
        naddr
    };
    nav.replace(Route::AddressViewer { address });
    rsx! {}
}
