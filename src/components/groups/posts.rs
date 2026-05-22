use crate::components::rich_content::RichContent;
use crate::stores::profiles;
use crate::stores::social::group_store::GroupNote;
use crate::utils::time;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

const PUBKEY_COLORS: &[&str] = &[
    "text-red-500", "text-orange-500", "text-amber-500", "text-yellow-500",
    "text-lime-500", "text-green-500", "text-emerald-500", "text-teal-500",
    "text-cyan-500", "text-sky-500", "text-blue-500", "text-indigo-500",
    "text-violet-500", "text-purple-500", "text-fuchsia-500", "text-pink-500",
    "text-rose-500",
];

fn pubkey_color(pubkey: &str) -> &'static str {
    let hash = pubkey.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    PUBKEY_COLORS[(hash as usize) % PUBKEY_COLORS.len()]
}

#[component]
pub fn GroupPostsView(
    notes: Vec<GroupNote>,
    relay_url: String,
    group_id: String,
) -> Element {
    let top_level: Vec<&GroupNote> = notes
        .iter()
        .filter(|n| n.root_id.is_none() && n.reply_to.is_none())
        .collect();

    rsx! {
        div { class: "p-4 space-y-3",
            if top_level.is_empty() {
                div { class: "text-center py-8 text-muted-foreground text-sm",
                    "No posts yet in this group"
                }
            }
            for note in top_level {
                GroupNoteCard {
                    key: "{note.id}",
                    note: note.clone(),
                }
            }
        }
    }
}

#[component]
fn GroupNoteCard(note: GroupNote) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let author_pk = note.author.clone();

    {
        let pk = author_pk.clone();
        use_effect(move || {
            let pk = pk.clone();
            spawn(async move {
                if let Ok(p) = profiles::fetch_profile(pk).await {
                    profile.set(Some(p));
                }
            });
        });
    }

    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| truncate_pubkey(&author_pk));
    let color_class = pubkey_color(&author_pk);
    let initial = display_name.chars().next().unwrap_or('?');
    let ts = time::format_time_ago(note.created_at);

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-3",
            div { class: "flex items-center gap-3",
                div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-sm font-semibold text-muted-foreground overflow-hidden shrink-0",
                    if let Some(url) = profile.read().as_ref().and_then(|p| p.picture.clone()).filter(|u| !u.is_empty()) {
                        img { class: "w-full h-full object-cover", src: "{url}", loading: "lazy" }
                    } else {
                        "{initial}"
                    }
                }
                div {
                    div { class: "text-sm font-semibold {color_class}", "{display_name}" }
                    div { class: "text-xs text-muted-foreground", "{ts}" }
                }
            }
            div { class: "text-sm text-foreground",
                RichContent {
                    content: note.content.clone(),
                    tags: note.event.tags.iter().cloned().collect(),
                    collapsible: true,
                }
            }
            if !note.reactions.is_empty() {
                div { class: "flex flex-wrap gap-1 pt-2 border-t border-border",
                    for (emoji, pubkeys) in note.reactions.iter() {
                        span {
                            key: "{emoji}",
                            class: "text-xs px-1.5 py-0.5 rounded-full border border-border bg-accent/50 cursor-default",
                            "{emoji} {pubkeys.len()}"
                        }
                    }
                }
            }
        }
    }
}
