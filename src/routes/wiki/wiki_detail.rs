use crate::components::viewers::WikiViewer;
use dioxus::prelude::*;

#[component]
pub fn WikiDetail(npub: String, identifier: String) -> Element {
    rsx! { WikiViewer { npub, identifier } }
}
