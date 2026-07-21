use crate::components::deflock::map_container::DeflockMapContainer;
use crate::stores::nostr_client;
use dioxus::prelude::*;

#[component]
pub fn DeflockMap() -> Element {
    rsx! {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            div { class: "fixed inset-0 bg-black flex items-center justify-center z-[100]",
                div { class: "text-center",
                    div { class: "mb-4 flex justify-center",
                        span { class: "inline-block h-10 w-10 rounded-full border-4 border-white/30 border-t-white animate-spin" }
                    }
                    p { class: "text-sm text-white/70", "Loading..." }
                }
            }
        } else {
            DeflockMapContainer {}
        }
    }
}
