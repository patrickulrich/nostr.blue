use crate::components::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use crate::stores::edit_cache;
use crate::utils::format_relative_time_or;
use dioxus::prelude::*;

#[component]
pub fn EditStatus(edit_info: edit_cache::EditInfo, event_id: String) -> Element {
    let edited_ago = format_relative_time_or(edit_info.edited_at, "just now");
    let mut show_history = use_signal(|| false);
    let event_id_for_history = event_id.clone();

    rsx! {
        span { class: "text-muted-foreground text-sm", "·" }
        button {
            class: "text-muted-foreground text-sm italic hover:underline cursor-pointer",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                show_history.set(true);
            },
            "Edited {edited_ago}"
        }
        DialogRoot {
            open: *show_history.read(),
            on_open_change: move |v: bool| show_history.set(v),
            is_modal: true,
            DialogContent {
                class: "max-w-lg max-h-[80vh] overflow-y-auto",
                DialogTitle {
                    class: "text-lg font-bold mb-4",
                    "Edit History"
                }
                DialogDescription {
                    class: "text-muted-foreground text-sm mb-4",
                    "Changes made to this post over time"
                }
                {render_history(&event_id_for_history)}
                div { class: "mt-4 flex justify-end",
                    button {
                        class: "px-4 py-2 text-sm font-medium hover:bg-accent rounded-lg transition",
                        onclick: move |_| show_history.set(false),
                        "Close"
                    }
                }
            }
        }
    }
}

fn render_history(event_id: &str) -> Element {
    let history = edit_cache::get_edit_history(event_id);
    let _v = edit_cache::EDIT_VERSION.read();
    if history.is_empty() {
        return rsx! {
            p { class: "text-muted-foreground text-sm", "No edit history available" }
        };
    }
    rsx! {
        div { class: "space-y-3",
            for (i, entry) in history.iter().enumerate() {
                div {
                    key: "{entry.event_id}",
                    class: "border border-border rounded-lg p-3",
                    div { class: "flex items-center justify-between mb-2",
                        span { class: "text-sm font-medium text-foreground",
                            if i == 0 {
                                "Original"
                            } else {
                                "Edit {i}"
                            }
                        }
                        span { class: "text-xs text-muted-foreground",
                            {format_relative_time_or(entry.timestamp, "just now")}
                        }
                    }
                    p { class: "text-sm text-foreground whitespace-pre-wrap break-words",
                        "{entry.content}"
                    }
                }
            }
        }
    }
}
