//! PR Review Section Component
//!
//! Displays code reviews for a pull request and allows maintainers
//! to submit reviews with Approve/Request Changes/Comment states.
//! Reviews are cached in-memory per PR event ID.
use crate::services::git_hosting::reviews::fetch_pr_reviews;
use crate::stores::profiles::PROFILE_CACHE;
use crate::stores::nostr_client::{get_client, HAS_SIGNER};
use crate::utils::format_relative_time_or;
use crate::utils::nip34::PersistedReview;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;

/// Publish a review as a Kind 1985 event on Nostr
pub async fn publish_review_event(pr_event_id: &str, state: &str, content: &str) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let event_id = EventId::from_hex(pr_event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let builder = EventBuilder::new(Kind::Custom(1985), content)
        .tag(Tag::event(event_id))
        .tag(Tag::custom(
            TagKind::Custom(std::borrow::Cow::Borrowed("state")),
            [state],
        ));
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish review: {}", e))?;
    Ok(output.id().to_hex())
}

/// Local review state enum for form selection
#[derive(Debug, Clone, Copy, PartialEq)]
enum LocalReviewState {
    Approved,
    ChangesRequested,
    Commented,
}

impl LocalReviewState {
    fn label(&self) -> &'static str {
        match self {
            Self::Approved => "Approved",
            Self::ChangesRequested => "Changes Requested",
            Self::Commented => "Commented",
        }
    }

    fn bg_class(&self) -> &'static str {
        match self {
            Self::Approved => "bg-green-500/10 text-green-500 border-green-500/20",
            Self::ChangesRequested => "bg-orange-500/10 text-orange-500 border-orange-500/20",
            Self::Commented => "bg-blue-500/10 text-blue-500 border-blue-500/20",
        }
    }

    fn from_persisted(state: &crate::utils::nip34::ReviewState) -> Self {
        match state {
            crate::utils::nip34::ReviewState::Approved => Self::Approved,
            crate::utils::nip34::ReviewState::ChangesRequested => Self::ChangesRequested,
            crate::utils::nip34::ReviewState::Commented => Self::Commented,
        }
    }

    fn to_event_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::ChangesRequested => "changes_requested",
            Self::Commented => "commented",
        }
    }
}

/// PR Review Section component
#[component]
pub fn PRReviewSection(
    pr_id: String,
    pr_pubkey: String,
    maintainers: Vec<String>,
    user_pubkey: String,
    is_authenticated: bool,
    #[props(default = None)]
    required_approvals: Option<u32>,
) -> Element {
    let can_review = is_authenticated
        && user_pubkey != pr_pubkey
        && maintainers.contains(&user_pubkey);
    let mut show_form = use_signal(|| false);
    let mut selected_state = use_signal(|| LocalReviewState::Approved);
    let mut review_body = use_signal(String::new);
    let mut reviews = use_signal(Vec::<PersistedReview>::new);

    // Fetch persisted reviews from relays on mount
    let pr_id_for_fetch = pr_id.clone();
    use_effect(move || {
        let id = pr_id_for_fetch.clone();
        spawn(async move {
            if let Ok(persisted) = fetch_pr_reviews(&id).await {
                reviews.set(persisted);
            }
        });
    });

    let review_list = reviews.read();
    let approve_count = review_list.iter().filter(|r| r.state == crate::utils::nip34::ReviewState::Approved).count();
    let changes_count = review_list
        .iter()
        .filter(|r| r.state == crate::utils::nip34::ReviewState::ChangesRequested)
        .count();

    let handle_submit = {
        let pr_id = pr_id.clone();
        let user_pubkey = user_pubkey.clone();
        move |_| {
            let state = *selected_state.read();
            let body = review_body.read().clone();
            let content = if body.trim().is_empty() {
                String::new()
            } else {
                body
            };
            let state_str = state.to_event_str();
            let now = web_sys::js_sys::Date::now() as u64 / 1000;
            // Add to local display immediately
            let local_review = PersistedReview {
                pr_event_id: pr_id.clone(),
                content: content.clone(),
                state: match state {
                    LocalReviewState::Approved => crate::utils::nip34::ReviewState::Approved,
                    LocalReviewState::ChangesRequested => crate::utils::nip34::ReviewState::ChangesRequested,
                    LocalReviewState::Commented => crate::utils::nip34::ReviewState::Commented,
                },
                event_id: String::new(),
                pubkey: user_pubkey.clone(),
                created_at: now,
            };
            let mut current = reviews.write();
            current.retain(|r| r.pubkey != user_pubkey);
            current.push(local_review);
            drop(current);
            show_form.set(false);
            review_body.set(String::new());
            // Publish to relays
            let id = pr_id.clone();
            let saved_content = content.clone();
            let saved_pubkey = user_pubkey.clone();
            spawn(async move {
                if let Err(e) = publish_review_event(&id, state_str, &saved_content).await {
                    web_sys::console::error_1(&format!("Failed to publish review: {}", e).into());
                    // Rollback: remove the optimistic entry
                    let mut current = reviews.write();
                    current.retain(|r| !(r.pubkey == saved_pubkey && r.content == saved_content && r.event_id.is_empty()));
                    drop(current);
                    // Restore form so user can retry
                    show_form.set(true);
                    review_body.set(saved_content);
                }
            });
        }
    };

    rsx! {
        div { class: "border border-border rounded-lg",
            div { class: "px-4 py-3 border-b border-border bg-muted/30",
                h3 { class: "font-semibold text-sm flex items-center gap-2",
                    svg {
                        class: "w-4 h-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" }
                        path { d: "M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" }
                    }
                    "Reviews"
                }
            }
            div { class: "p-4 space-y-3",
                // Summary badges
                if approve_count > 0 || changes_count > 0 {
                    {
                        let approve_label = if approve_count == 1 {
                            format!("{} approval", approve_count)
                        } else {
                            format!("{} approvals", approve_count)
                        };
                        let changes_label = if changes_count == 1 {
                            format!("{} change request", changes_count)
                        } else {
                            format!("{} change requests", changes_count)
                        };
                        rsx! {
                            div { class: "flex flex-wrap gap-2",
                                if approve_count > 0 {
                                    span { class: "px-2 py-0.5 text-xs rounded-full border bg-green-500/10 text-green-500 border-green-500/20",
                                        "{approve_label}"
                                    }
                                }
                                if changes_count > 0 {
                                    span { class: "px-2 py-0.5 text-xs rounded-full border bg-orange-500/10 text-orange-500 border-orange-500/20",
                                        "{changes_label}"
                                    }
                                }
                            }
                        }
                    }
                }
                // Required approvals indicator
                if let Some(required) = required_approvals {
                    if required > 0 {
                        {
                            let met = approve_count >= required as usize;
                            rsx! {
                                div { class: if met { "text-xs text-green-500 flex items-center gap-1" } else { "text-xs text-muted-foreground flex items-center gap-1" },
                                    if met {
                                        "Required approvals met ({approve_count}/{required})"
                                    } else {
                                        "Requires {required} approval(s), has {approve_count}"
                                    }
                                }
                            }
                        }
                    }
                }
                // Review list
                if review_list.is_empty() {
                    p { class: "text-sm text-muted-foreground text-center py-2",
                        "No reviews yet"
                    }
                } else {
                    div { class: "space-y-2",
                        for review in review_list.iter() {
                            {
                                let local_state = LocalReviewState::from_persisted(&review.state);
                                let profile = PROFILE_CACHE.read().peek(&review.pubkey).cloned();
                                let name = profile
                                    .as_ref()
                                    .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
                                    .unwrap_or_else(|| truncate_pubkey(&review.pubkey));
                                let picture = profile.as_ref().and_then(|p| p.picture.clone());
                                rsx! {
                                    div {
                                        key: "{review.pubkey}_{review.created_at}",
                                        class: "flex items-start gap-2 p-2 rounded-lg bg-muted/30",
                                        div { class: "w-6 h-6 rounded-full bg-muted flex items-center justify-center overflow-hidden shrink-0",
                                            if let Some(pic) = &picture {
                                                img {
                                                    class: "w-full h-full object-cover",
                                                    src: "{pic}",
                                                    alt: "Reviewer",
                                                }
                                            } else {
                                                span { class: "text-[10px]", "{name.chars().next().unwrap_or('?')}" }
                                            }
                                        }
                                        div { class: "flex-1 min-w-0",
                                            div { class: "flex items-center gap-2 flex-wrap",
                                                span { class: "text-sm font-medium", "{name}" }
                                                span { class: format!("px-1.5 py-0.5 text-xs rounded-full border {}", local_state.bg_class()),
                                                    "{local_state.label()}"
                                                }
                                                span { class: "text-xs text-muted-foreground",
                                                    {format_relative_time_or(review.created_at, "")}
                                                }
                                            }
                                            if !review.content.is_empty() {
                                                p { class: "text-sm text-muted-foreground mt-1", "{review.content}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Add Review button / form
                if can_review {
                    if *show_form.read() {
                        div { class: "border border-border rounded-lg p-3 space-y-3",
                            div { class: "flex gap-2",
                                button {
                                    class: if *selected_state.read() == LocalReviewState::Approved {
                                        "flex-1 px-2 py-1.5 text-xs rounded-lg border-2 border-green-500 bg-green-500/10 text-green-500 font-medium"
                                    } else {
                                        "flex-1 px-2 py-1.5 text-xs rounded-lg border border-border text-muted-foreground hover:bg-accent/50 transition"
                                    },
                                    onclick: move |_| selected_state.set(LocalReviewState::Approved),
                                    "Approve"
                                }
                                button {
                                    class: if *selected_state.read() == LocalReviewState::ChangesRequested {
                                        "flex-1 px-2 py-1.5 text-xs rounded-lg border-2 border-orange-500 bg-orange-500/10 text-orange-500 font-medium"
                                    } else {
                                        "flex-1 px-2 py-1.5 text-xs rounded-lg border border-border text-muted-foreground hover:bg-accent/50 transition"
                                    },
                                    onclick: move |_| selected_state.set(LocalReviewState::ChangesRequested),
                                    "Request Changes"
                                }
                                button {
                                    class: if *selected_state.read() == LocalReviewState::Commented {
                                        "flex-1 px-2 py-1.5 text-xs rounded-lg border-2 border-blue-500 bg-blue-500/10 text-blue-500 font-medium"
                                    } else {
                                        "flex-1 px-2 py-1.5 text-xs rounded-lg border border-border text-muted-foreground hover:bg-accent/50 transition"
                                    },
                                    onclick: move |_| selected_state.set(LocalReviewState::Commented),
                                    "Comment"
                                }
                            }
                            textarea {
                                class: "w-full p-2 text-sm bg-background border border-border rounded-lg resize-none focus:outline-none focus:ring-1 focus:ring-primary",
                                placeholder: "Leave a comment (optional)",
                                rows: 3,
                                value: "{review_body}",
                                oninput: move |e| review_body.set(e.value()),
                            }
                            div { class: "flex justify-end gap-2",
                                button {
                                    class: "px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground transition",
                                    onclick: move |_| show_form.set(false),
                                    "Cancel"
                                }
                                button {
                                    class: "px-3 py-1.5 text-xs bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition",
                                    onclick: handle_submit,
                                    "Submit Review"
                                }
                            }
                        }
                    } else {
                        button {
                            class: "w-full px-3 py-2 text-sm border border-border rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent/50 transition",
                            onclick: move |_| show_form.set(true),
                            "Add Review"
                        }
                    }
                }
            }
        }
    }
}
