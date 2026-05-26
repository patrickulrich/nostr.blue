use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn VoiceMessageDetail(voice_id: String) -> Element {
    let nav = navigator();
    let address = if voice_id.starts_with("nevent") || voice_id.starts_with("note") {
        voice_id
    } else {
        crate::utils::nip19_urls::note_route_id(&voice_id, None)
    };
    nav.replace(Route::AddressViewer { address });
    rsx! {}
}
