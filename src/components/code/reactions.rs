//! Code Section Reactions
//!
//! Lightweight reaction display for issues, PRs, and comments.
//! Uses NIP-25 (Kind 7) reactions.
use crate::hooks::{use_reaction, ReactionEmoji};
use crate::stores::nostr_client::HAS_SIGNER;
use dioxus::prelude::*;

/// Inline reactions for code events (issues, PRs, comments)
#[component]
pub fn CodeReactions(event_id: String, event_author: String) -> Element {
    let reaction = use_reaction(event_id, event_author, None);
    let is_liked = *reaction.is_liked.read();
    let count = *reaction.like_count.read();
    let has_signer = *HAS_SIGNER.read();

    rsx! {
        div { class: "flex items-center gap-3",
            button {
                r#type: "button",
                class: if is_liked {
                    "flex items-center gap-1 px-2 py-1 text-xs rounded-lg bg-primary/10 text-primary border border-primary/20 transition"
                } else {
                    "flex items-center gap-1 px-2 py-1 text-xs rounded-lg text-muted-foreground hover:bg-accent/50 border border-transparent hover:border-border transition"
                },
                aria_label: if is_liked { "Unlike" } else { "Like" },
                disabled: !has_signer,
                onclick: move |_| {
                    if is_liked {
                        reaction.react_with.call(ReactionEmoji::Unlike);
                    } else {
                        reaction.toggle_like.call(());
                    }
                },
                // Thumbs up icon
                svg {
                    class: "w-3.5 h-3.5",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3zM7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3" }
                }
                if count > 0 {
                    span { "{count}" }
                }
            }
        }
    }
}
