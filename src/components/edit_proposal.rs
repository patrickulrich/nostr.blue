use crate::components::edit_post::EditPostView;
use crate::components::RichContent;
use crate::stores::profiles;
use crate::utils::format_relative_time_or;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::Event as NostrEvent;

#[component]
pub fn EditProposalCard(event: NostrEvent, original_event: NostrEvent) -> Element {
    let mut show_accept_modal = use_signal(|| false);
    let my_pubkey = crate::stores::auth_store::get_pubkey()
        .and_then(|pk| nostr_sdk::PublicKey::from_hex(&pk).ok());
    let is_original_author = my_pubkey
        .as_ref()
        .map(|pk| *pk == original_event.pubkey)
        .unwrap_or(false);

    let proposer_pk = event.pubkey.to_hex();
    let profile = profiles::get_profile(&proposer_pk);
    let display_name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().filter(|n| !n.is_empty()))
        .or_else(|| profile.as_ref().and_then(|p| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&proposer_pk));
    let short_pk = truncate_pubkey(&proposer_pk);
    let timestamp = format_relative_time_or(event.created_at.as_secs(), "just now");

    let summary = event.tags.iter().find_map(|tag| {
        let vec = tag.clone().to_vec();
        if vec.len() >= 2 && vec[0] == "summary" {
            Some(vec[1].clone())
        } else {
            None
        }
    });

    rsx! {
        div { class: "my-2 border border-border rounded-lg overflow-hidden",
            div { class: "px-4 py-3 bg-muted/50 border-b border-border",
                div { class: "flex items-center justify-between",
                    div { class: "flex items-center gap-2",
                        span { class: "text-sm font-medium text-foreground",
                            "Edit Proposal"
                        }
                        span { class: "text-muted-foreground text-sm",
                            "by {display_name} @{short_pk}"
                        }
                    }
                    span { class: "text-muted-foreground text-sm", "{timestamp}" }
                }
                if let Some(sum) = summary {
                    div { class: "mt-1 text-sm text-muted-foreground italic",
                        "{sum}"
                    }
                }
            }
            div { class: "px-4 py-3",
                div { class: "text-sm mb-2 text-muted-foreground font-medium",
                    "Proposed content:"
                }
                div { class: "text-sm",
                    RichContent {
                        content: event.content.clone(),
                        tags: event.tags.iter().cloned().collect(),
                    }
                }
            }
            if is_original_author {
                div { class: "px-4 py-3 border-t border-border",
                    button {
                        class: "w-full px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition",
                        onclick: move |_| {
                            show_accept_modal.set(true);
                        },
                        "Accept Suggestion"
                    }
                }
            }
        }
        if *show_accept_modal.read() {
            EditPostView {
                original_event: original_event.clone(),
                prefill_content: Some(event.content.clone()),
                on_close: move |_| show_accept_modal.set(false),
                on_success: move |_| show_accept_modal.set(false),
            }
        }
    }
}
