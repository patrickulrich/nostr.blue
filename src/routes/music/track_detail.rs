use crate::components::viewers::MusicTrackViewer;
use dioxus::prelude::*;

#[component]
pub fn MusicTrackDetail(track_id: String) -> Element {
    rsx! { MusicTrackViewer { track_id } }
}
