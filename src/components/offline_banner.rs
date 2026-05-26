use crate::stores::ui::online_status::ONLINE_STATUS;
use dioxus::prelude::*;

#[component]
pub fn OfflineBanner() -> Element {
    let online = ONLINE_STATUS();

    if online {
        return rsx! {};
    }

    rsx! {
        div {
            class: "fixed top-0 left-0 right-0 z-[60] bg-amber-600 text-white text-center text-sm py-1.5 px-4 font-medium",
            "You are offline — some features may be unavailable"
        }
    }
}
