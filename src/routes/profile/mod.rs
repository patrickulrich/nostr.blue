use crate::routes::Route;
use dioxus::prelude::*;

pub(crate) mod loader;
pub(crate) mod types;

pub use types::{MediaSubTab, ProfileTab, ZapSubTab};

#[component]
pub fn Profile(pubkey: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer { address: pubkey });
    rsx! {}
}
