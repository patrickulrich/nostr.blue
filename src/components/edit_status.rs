use crate::stores::edit_cache::EditInfo;
use crate::utils::format_relative_time_or;
use dioxus::prelude::*;

#[component]
pub fn EditStatus(edit_info: EditInfo) -> Element {
    let edited_ago = format_relative_time_or(edit_info.edited_at, "just now");
    rsx! {
        span { class: "text-muted-foreground text-sm", "·" }
        span { class: "text-muted-foreground text-sm italic", "Edited {edited_ago}" }
    }
}
