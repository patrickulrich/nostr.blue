use crate::stores::nostr_client;
use crate::stores::relay::signals::RELAY_CONNECTED;
use dioxus::prelude::*;

use crate::components::places::map_container::PlacesMapContainer;

#[component]
pub fn PlacesMap() -> Element {
    rsx! {
        if !*nostr_client::CLIENT_INITIALIZED.read() || !*RELAY_CONNECTED.read() {
            div { class: "fixed inset-0 bg-black flex items-center justify-center z-[100]",
                div { class: "text-center",
                    div { class: "mb-4 flex justify-center",
                        span { class: "inline-block h-10 w-10 rounded-full border-4 border-white/30 border-t-white animate-spin" }
                    }
                    p { class: "text-sm text-white/70", "Connecting..." }
                }
            }
        } else {
            PlacesMapContainer {}
        }
    }
}
