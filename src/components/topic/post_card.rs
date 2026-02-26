//! Topic Post Card Component
//! Reddit-style post card with vote column on left, content on right
use crate::components::topic::{TopicBadge, VoteColumn};
use crate::components::RichContent;
use crate::routes::Route;
use crate::stores::profiles::get_cached_profile;
use crate::stores::topic_store::{TopicPost, VoteCounts};
use crate::utils::format::format_relative_time_or;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen::JsCast;

/// Reddit-style topic post card
#[component]
pub fn TopicPostCard(
    post: TopicPost,
    #[props(default)]
    vote_counts: Option<VoteCounts>,
    #[props(default = false)]
    show_topic_badge: bool,
) -> Element {
    let profile = get_cached_profile(&post.pubkey);
    let author_name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| {
            let truncated: String = post.pubkey.chars().take(8).collect();
            format!("{}...", truncated)
        });
    let author_picture = profile.as_ref().and_then(|p| p.picture.clone());
    let time_ago = format_relative_time_or(post.created_at, "just now");
    let counts = vote_counts.unwrap_or_default();
    let topic_for_link = post.topic.clone();
    let post_id_for_link = post.id.clone();
    let topic_for_key = post.topic.clone();
    let post_id_for_key = post.id.clone();
    let post_for_vote = post.clone();

    rsx! {
        div {
            class: "flex gap-3 bg-card border border-border rounded-lg p-4 hover:bg-accent/50 transition",
            // Vote column
            VoteColumn {
                post: post_for_vote,
                vote_counts: counts,
            }
            // Content
            div {
                class: "flex-1 min-w-0",
                // Header: topic badge + author + time
                div {
                    class: "flex items-center gap-2 text-sm text-muted-foreground mb-1 flex-wrap",
                    if show_topic_badge {
                        TopicBadge { topic: post.topic.clone() }
                    }
                    Link {
                        to: Route::Profile { pubkey: post.pubkey.clone() },
                        class: "flex items-center gap-1.5 hover:text-foreground transition",
                        if let Some(pic) = &author_picture {
                            img {
                                src: "{pic}",
                                alt: "{author_name}",
                                class: "w-5 h-5 rounded-full object-cover",
                            }
                        }
                        span { class: "font-medium", "{author_name}" }
                    }
                    span { "\u{00B7}" }
                    span { "{time_ago}" }
                }
                // Post content — use div+onclick instead of Link to avoid nested <a> when content contains links
                div {
                    class: "block cursor-pointer",
                    role: "link",
                    tabindex: "0",
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if !activate { return; }
                        // Don't navigate if key event originated from/inside an anchor element
                        if let Some(target) = evt.data.as_web_event().target() {
                            if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                                if element.closest("a, button, input, textarea, select, summary, [role=\"button\"]").ok().flatten().is_some() {
                                    return;
                                }
                            }
                        }
                        evt.prevent_default();
                        navigator().push(Route::TopicPostDetail {
                            topic: topic_for_key.clone(),
                            post_id: post_id_for_key.clone(),
                        });
                    },
                    onclick: move |evt| {
                        // Don't navigate if click originated from/inside an interactive element
                        if let Some(target) = evt.data.as_web_event().target() {
                            if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                                if element.closest("a, button, input, textarea, select, summary, [role=\"button\"]").ok().flatten().is_some() {
                                    return;
                                }
                            }
                        }
                        navigator().push(Route::TopicPostDetail {
                            topic: topic_for_link.clone(),
                            post_id: post_id_for_link.clone(),
                        });
                    },
                    div {
                        class: "prose prose-sm max-w-none text-foreground",
                        RichContent {
                            content: post.content.clone(),
                            tags: post.event.tags.to_vec(),
                        }
                    }
                }
            }
        }
    }
}
