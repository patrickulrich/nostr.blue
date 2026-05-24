//! Topic Post Card Component
//! Reddit-style post card with vote column on left, content on right
use crate::components::topic::{TopicBadge, VoteColumn};
use crate::components::RichContent;
use crate::components::SensitiveContent;
use crate::routes::Route;
use crate::stores::nostr_client;
use crate::stores::profiles::get_cached_profile;
use crate::stores::topic_store::{TopicPost, VoteCounts};
use crate::utils::format::format_relative_time_or;
use crate::utils::nip36;
use dioxus::prelude::*;
use std::collections::HashSet;
use std::rc::Rc;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

#[cfg(feature = "web")]
const INTERACTIVE_ELEMENT_SELECTOR: &str =
    "a, button, input, textarea, select, summary, video, audio, iframe, [role=\"button\"], [role=\"link\"]:not([data-post-card-root]), [contenteditable=\"true\"], [data-interactive]";

/// Reddit-style topic post card
#[component]
pub fn TopicPostCard(
    post: TopicPost,
    #[props(default)] vote_counts: Option<VoteCounts>,
    #[props(default = false)] show_topic_badge: bool,
    #[props(default = None)] cached_muted_posts: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_blocked_users: Option<Rc<HashSet<String>>>,
) -> Element {
    let post_id_for_check = post.id.clone();
    let author_pubkey_for_check = post.pubkey.clone();
    let cached_muted_posts_reactive = cached_muted_posts.clone();
    let cached_blocked_users_reactive = cached_blocked_users.clone();
    let mut is_muted = use_signal(|| None::<bool>);
    let mut is_author_blocked = use_signal(|| None::<bool>);
    let mut show_hidden_anyway = use_signal(|| false);

    use_effect(use_reactive!(|(
        cached_muted_posts_reactive,
        cached_blocked_users_reactive,
        post_id_for_check,
        author_pubkey_for_check,
    )| {
        let post_id = post_id_for_check.clone();
        let author_pubkey = author_pubkey_for_check.clone();
        if let Some(ref muted_set) = cached_muted_posts_reactive {
            if let Ok(muted) = nostr_client::is_post_muted_cached(&post_id, muted_set) {
                is_muted.set(Some(muted));
            }
        }
        if let Some(ref blocked_set) = cached_blocked_users_reactive {
            if let Ok(blocked) = nostr_client::is_user_blocked_cached(&author_pubkey, blocked_set) {
                is_author_blocked.set(Some(blocked));
            }
        }
        if cached_muted_posts_reactive.is_none() || cached_blocked_users_reactive.is_none() {
            let need_muted = cached_muted_posts_reactive.is_none();
            let need_blocked = cached_blocked_users_reactive.is_none();
            spawn(async move {
                if need_muted {
                    match nostr_client::is_post_muted(post_id.clone()).await {
                        Ok(muted) => is_muted.set(Some(muted)),
                        Err(_) => is_muted.set(Some(false)),
                    }
                }
                if need_blocked {
                    match nostr_client::is_user_blocked(author_pubkey).await {
                        Ok(blocked) => is_author_blocked.set(Some(blocked)),
                        Err(_) => is_author_blocked.set(Some(false)),
                    }
                }
            });
        }
    }));

    let is_hidden = (is_muted.read().unwrap_or(false) || is_author_blocked.read().unwrap_or(false))
        && !*show_hidden_anyway.read();

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
            if is_hidden {
                div { class: "flex items-center gap-3 w-full py-2",
                    div { class: "flex-1 text-muted-foreground text-sm",
                        if is_author_blocked.read().unwrap_or(false) {
                            "Post from blocked user"
                        } else if is_muted.read().unwrap_or(false) {
                            "Muted post"
                        }
                    }
                    button {
                        class: "px-3 py-1 text-sm text-primary hover:underline",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            show_hidden_anyway.set(true);
                        },
                        "Show anyway"
                    }
                }
            } else {
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
                        to: Route::AddressViewer { address: crate::utils::nip19_urls::profile_route_id(&post.pubkey) },
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
                    "data-post-card-root": "true",
                    role: "link",
                    tabindex: "0",
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter);
                        if !activate { return; }
                        // Don't navigate if key event originated from/inside an anchor element
                        #[cfg(feature = "web")]
                        {
                            if let Some(target) = evt.data.as_web_event().target() {
                                if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                                    if element.closest(INTERACTIVE_ELEMENT_SELECTOR).ok().flatten().is_some() {
                                        return;
                                    }
                                }
                            }
                        }
                        evt.prevent_default();
                        navigator().push(Route::TopicPostDetail {
                            topic: topic_for_key.clone(),
                            post_id: post_id_for_key.clone(),
                        });
                    },
                    onclick: move |_evt| {
                        // Don't navigate if click originated from/inside an interactive element
                        #[cfg(feature = "web")]
                        {
                            if let Some(target) = _evt.data.as_web_event().target() {
                                if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                                    if element.closest(INTERACTIVE_ELEMENT_SELECTOR).ok().flatten().is_some() {
                                        return;
                                    }
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
                        {
                            let content_warning = nip36::get_content_warning(&post.event.tags);
                            if let Some(reason) = content_warning {
                                rsx! {
                                    SensitiveContent { reason,
                                        RichContent {
                                            content: post.content.clone(),
                                            tags: post.event.tags.to_vec(),
                                            interactive_media: true,
                                        }
                                    }
                                }
                            } else {
                                rsx! {
                                    RichContent {
                                        content: post.content.clone(),
                                        tags: post.event.tags.to_vec(),
                                        interactive_media: true,
                                    }
                                }
                            }
                        }
                    }
                }
            }
            }
        }
    }
}
