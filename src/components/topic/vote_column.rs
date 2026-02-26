//! Vote Column Component
//! Up arrow, score, down arrow for Reddit-style voting on topic posts
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::topic_store::{vote_on_post, TopicPost, VoteCounts, VoteDirection};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};

/// Vote column with up/down arrows and score display
#[component]
pub fn VoteColumn(post: TopicPost, vote_counts: VoteCounts) -> Element {
    let toast = consume_toast();
    let has_signer = *HAS_SIGNER.read();
    let mut voting = use_signal(|| false);

    let score = vote_counts.score();
    let user_vote = vote_counts.user_vote;

    let up_active = user_vote == Some(VoteDirection::Up);
    let down_active = user_vote == Some(VoteDirection::Down);

    let up_class = if up_active {
        "text-primary"
    } else {
        "text-muted-foreground hover:text-primary"
    };
    let down_class = if down_active {
        "text-destructive"
    } else {
        "text-muted-foreground hover:text-destructive"
    };
    let score_class = if score > 0 {
        "text-sm font-semibold tabular-nums text-primary"
    } else if score < 0 {
        "text-sm font-semibold tabular-nums text-destructive"
    } else {
        "text-sm font-semibold tabular-nums text-muted-foreground"
    };

    let post_for_up = post.clone();
    let post_for_down = post.clone();

    rsx! {
        div {
            class: "flex flex-col items-center gap-0.5 min-w-[2rem]",
            // Upvote button
            button {
                class: "p-1 rounded transition {up_class}",
                aria_label: "Upvote",
                title: "Upvote",
                disabled: !has_signer || *voting.read(),
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if *voting.peek() { return; }
                    if !has_signer { return; }
                    let post = post_for_up.clone();
                    voting.set(true);
                    spawn(async move {
                        if let Err(e) = vote_on_post(&post, VoteDirection::Up).await {
                            log::error!("Vote failed: {}", e);
                            toast.error(format!("Vote failed: {e}"), ToastOptions::new());
                        }
                        voting.set(false);
                    });
                },
                // Up arrow SVG
                svg {
                    class: "w-5 h-5",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: if up_active { "currentColor" } else { "none" },
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 19V5" }
                    path { d: "M5 12l7-7 7 7" }
                }
            }
            // Score
            span {
                class: "{score_class}",
                "{score}"
            }
            // Downvote button
            button {
                class: "p-1 rounded transition {down_class}",
                aria_label: "Downvote",
                title: "Downvote",
                disabled: !has_signer || *voting.read(),
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if *voting.peek() { return; }
                    if !has_signer { return; }
                    let post = post_for_down.clone();
                    voting.set(true);
                    spawn(async move {
                        if let Err(e) = vote_on_post(&post, VoteDirection::Down).await {
                            log::error!("Vote failed: {}", e);
                            toast.error(format!("Vote failed: {e}"), ToastOptions::new());
                        }
                        voting.set(false);
                    });
                },
                // Down arrow SVG
                svg {
                    class: "w-5 h-5",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: if down_active { "currentColor" } else { "none" },
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 5v14" }
                    path { d: "M19 12l-7 7-7-7" }
                }
            }
        }
    }
}
