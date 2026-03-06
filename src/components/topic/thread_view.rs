//! Thread View Component
//! Recursive nested replies with indentation for topic post threads
use crate::components::topic::VoteColumn;
use crate::components::RichContent;
use crate::routes::Route;
use crate::stores::profiles::get_cached_profile;
use crate::stores::topic_store::{TopicThread, VoteCounts};
use crate::utils::format::format_relative_time_or;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;

/// Maximum visual depth for reply indentation
const MAX_VISUAL_DEPTH: usize = 6;

/// Maximum recursion depth to prevent stack overflow on deeply nested threads
const MAX_RECURSION_DEPTH: usize = 20;

/// Display a thread tree with recursive nested replies
#[component]
pub fn ThreadView(
    thread: Vec<Rc<TopicThread>>,
    #[props(default)] vote_counts: Rc<HashMap<String, VoteCounts>>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2",
            for item in &thread {
                ThreadNode {
                    key: "{item.post.id}",
                    thread: item.clone(),
                    vote_counts: vote_counts.clone(),
                    depth: 0,
                }
            }
        }
    }
}

/// Count all descendants (replies + their replies) for a thread node using iterative traversal
fn count_descendants(thread: &TopicThread) -> usize {
    const MAX_COUNT: usize = 1000;
    let mut count = 0usize;
    let mut stack: Vec<&TopicThread> = thread.replies.iter().map(|r| r.as_ref()).collect();
    while let Some(node) = stack.pop() {
        count += 1;
        if count >= MAX_COUNT {
            return count;
        }
        stack.extend(node.replies.iter().map(|r| r.as_ref()));
    }
    count
}

/// Single thread node with its replies
#[component]
fn ThreadNode(
    thread: Rc<TopicThread>,
    vote_counts: Rc<HashMap<String, VoteCounts>>,
    #[props(default = 0)] depth: usize,
) -> Element {
    // Prevent stack overflow on deeply nested threads
    if depth >= MAX_RECURSION_DEPTH {
        let count = count_descendants(&thread);
        return rsx! {
            div { class: "py-2 px-3 text-xs text-muted-foreground italic",
                if count > 0 {
                    "Thread continues ({count} more replies hidden)"
                } else {
                    "Further replies hidden"
                }
            }
        };
    }

    let profile = get_cached_profile(&thread.post.pubkey);
    let author_name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| {
            let truncated: String = thread.post.pubkey.chars().take(8).collect();
            format!("{}...", truncated)
        });
    let author_picture = profile.as_ref().and_then(|p| p.picture.clone());
    let time_ago = format_relative_time_or(thread.post.created_at, "just now");
    let counts = vote_counts
        .get(&thread.post.id)
        .cloned()
        .unwrap_or_default();
    let post_for_vote = thread.post.clone();

    let indent_class = match depth.min(MAX_VISUAL_DEPTH) {
        0 => "",
        1 => "ml-4 border-l-2 border-border/80 pl-3",
        2 => "ml-4 border-l-2 border-border/60 pl-3",
        3 => "ml-4 border-l-2 border-accent pl-3",
        4 => "ml-4 border-l-2 border-accent/80 pl-3",
        5 => "ml-4 border-l-2 border-accent/60 pl-3",
        _ => "ml-4 border-l-2 border-border/40 pl-3",
    };

    rsx! {
        div {
            class: "{indent_class}",
            // Reply content
            div {
                class: "flex gap-2 py-2",
                // Mini vote column
                VoteColumn {
                    post: post_for_vote,
                    vote_counts: counts,
                }
                div {
                    class: "flex-1 min-w-0",
                    // Reply header
                    div {
                        class: "flex items-center gap-2 text-xs text-muted-foreground mb-1",
                        Link {
                            to: Route::Profile { pubkey: thread.post.pubkey.clone() },
                            class: "flex items-center gap-1 hover:text-foreground transition",
                            if let Some(pic) = &author_picture {
                                img {
                                    src: "{pic}",
                                    alt: "{author_name}",
                                    class: "w-4 h-4 rounded-full object-cover",
                                    loading: "lazy",
                                }
                            }
                            span { class: "font-medium", "{author_name}" }
                        }
                        span { "\u{00B7}" }
                        span { "{time_ago}" }
                    }
                    // Reply content
                    div {
                        class: "text-sm text-foreground",
                        RichContent {
                            content: thread.post.content.clone(),
                            tags: thread.post.event.tags.clone().to_vec(),
                        }
                    }
                }
            }
            // Nested replies
            if !thread.replies.is_empty() {
                for reply in &thread.replies {
                    ThreadNode {
                        key: "{reply.post.id}",
                        thread: reply.clone(),
                        vote_counts: vote_counts.clone(),
                        depth: depth + 1,
                    }
                }
            }
        }
    }
}
