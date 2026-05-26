use crate::components::viewers::CalendarEventViewer;
use dioxus::prelude::*;

#[component]
pub fn CalendarEventDetail(naddr: String, from: Option<String>) -> Element {
    rsx! { CalendarEventViewer { naddr, from } }
}
