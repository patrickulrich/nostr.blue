//! PR Review Section Component
//!
//! Displays code reviews for a pull request and allows maintainers
//! to submit reviews with Approve/Request Changes/Comment states.
//! Reviews are cached in-memory per PR event ID.
use crate::services::git_hosting::reviews::fetch_pr_reviews;
use crate::stores::nostr_client::{get_client, CLIENT_INITIALIZED, HAS_SIGNER};
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format_relative_time_or;
use crate::utils::nip34::PersistedReview;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::borrow::Cow;
use std::collections::HashSet;

/// Publish a review as a Kind 9807 event on Nostr
pub async fn publish_review_event(
    pr_event_id: &str,
    pr_author_pubkey: &str,
    state: crate::utils::nip34::ReviewState,
    content: &str,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let event_id =
        EventId::from_hex(pr_event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let author_pk = PublicKey::from_hex(pr_author_pubkey)
        .map_err(|e| format!("Invalid author pubkey: {}", e))?;
    let builder = EventBuilder::new(Kind::Custom(PersistedReview::KIND), content)
        .tag(Tag::event(event_id))
        .tag(Tag::public_key(author_pk))
        .tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("state")),
            [state.as_str()],
        ));
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
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
        self.to_review_state().label()
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

    fn to_review_state(self) -> crate::utils::nip34::ReviewState {
        match self {
            Self::Approved => crate::utils::nip34::ReviewState::Approved,
            Self::ChangesRequested => crate::utils::nip34::ReviewState::ChangesRequested,
            Self::Commented => crate::utils::nip34::ReviewState::Commented,
        }
    }
}

fn should_replace_review(existing: &PersistedReview, candidate: &PersistedReview) -> bool {
    if candidate.created_at > existing.created_at {
        return true;
    }
    if !existing.event_id.is_empty()
        && !candidate.event_id.is_empty()
        && existing.event_id == candidate.event_id
        && candidate.created_at != existing.created_at
    {
        return true;
    }
    if candidate.created_at == existing.created_at {
        // If existing is optimistic (no event_id), only replace if candidate is semantically the same
        if existing.event_id.is_empty() && !candidate.event_id.is_empty() {
            // Compare semantic fields to ensure it's the same review
            return existing.pubkey == candidate.pubkey
                && existing.state == candidate.state
                && existing.content == candidate.content;
        }
        // For persisted reviews, use event_id as tie-breaker (NIP-01 style: lower ID wins)
        if !existing.event_id.is_empty() && !candidate.event_id.is_empty() {
            return candidate.event_id < existing.event_id;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::nip34::ReviewState;

    fn review(event_id: &str, created_at: u64) -> PersistedReview {
        PersistedReview {
            pr_event_id: "pr".to_string(),
            content: "content".to_string(),
            state: ReviewState::Approved,
            event_id: event_id.to_string(),
            pubkey: "pubkey".to_string(),
            created_at,
        }
    }

    fn review_with_content(
        event_id: &str,
        created_at: u64,
        content: &str,
        state: ReviewState,
    ) -> PersistedReview {
        PersistedReview {
            pr_event_id: "pr".to_string(),
            content: content.to_string(),
            state,
            event_id: event_id.to_string(),
            pubkey: "pubkey".to_string(),
            created_at,
        }
    }

    #[test]
    fn lower_event_id_wins_for_equal_persisted_reviews() {
        let existing = review("ff", 100);
        let candidate = review("0a", 100);
        assert!(should_replace_review(&existing, &candidate));
        assert!(!should_replace_review(&candidate, &existing));
    }

    #[test]
    fn optimistic_same_fields_replaces() {
        // Existing optimistic review (no event_id)
        let existing = review_with_content("", 100, "Great PR!", ReviewState::Approved);
        // Candidate persisted review with identical semantic fields
        let candidate = review_with_content("abc123", 100, "Great PR!", ReviewState::Approved);
        // Should replace because semantic fields match
        assert!(should_replace_review(&existing, &candidate));
    }

    #[test]
    fn optimistic_different_content_no_replace() {
        // Existing optimistic review with one content
        let existing = review_with_content("", 100, "Great PR!", ReviewState::Approved);
        // Candidate persisted review with different content
        let candidate = review_with_content("abc123", 100, "Needs work", ReviewState::Approved);
        // Should NOT replace because content differs
        assert!(!should_replace_review(&existing, &candidate));
    }

    #[test]
    fn newer_timestamp_wins() {
        // Existing review with older timestamp
        let existing = review("abc", 100);
        // Candidate review with newer timestamp
        let candidate = review("def", 200);
        // Should replace because candidate is newer
        assert!(should_replace_review(&existing, &candidate));
        // Should NOT replace in reverse direction
        assert!(!should_replace_review(&candidate, &existing));
    }

    #[test]
    fn same_event_id_with_different_timestamp_replaces() {
        let existing = review("same-event", 100);
        let candidate = review("same-event", 200);
        assert!(should_replace_review(&existing, &candidate));
    }

    #[test]
    fn same_event_id_older_candidate_still_replaces() {
        let existing = review("same-event", 200);
        let candidate = review("same-event", 100);
        assert!(should_replace_review(&existing, &candidate));
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
    #[props(default = None)] required_approvals: Option<u32>,
    #[props(default = None)] on_review_submitted: Option<EventHandler<()>>,
) -> Element {
    let can_review =
        is_authenticated && user_pubkey != pr_pubkey && maintainers.contains(&user_pubkey);
    let mut show_form = use_signal(|| false);
    let mut selected_state = use_signal(|| LocalReviewState::Approved);
    let mut review_body = use_signal(String::new);
    let mut reviews = use_signal(Vec::<PersistedReview>::new);
    let mut publish_error = use_signal(|| None::<String>);
    let mut fetch_error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);

    // Increment the generation and reset local UI state only when the PR changes.
    let mut gen = use_signal(|| 0u32);
    use_effect(use_reactive(&pr_id, move |id| {
        let _ = id;
        let current_gen = gen.peek().wrapping_add(1);
        gen.set(current_gen);
        reviews.set(Vec::new());
        publish_error.set(None);
        fetch_error.set(None);
        show_form.set(false);
        submitting.set(false);
        review_body.set(String::new());
        selected_state.set(LocalReviewState::Approved);
    }));

    // Fetch persisted reviews when the PR changes or the client becomes ready.
    use_effect(use_reactive(
        (&pr_id, &*CLIENT_INITIALIZED.read()),
        move |(id, is_ready)| {
            if !is_ready {
                return;
            }
            let current_gen = *gen.peek();
            spawn(async move {
                match fetch_pr_reviews(&id).await {
                    Ok(persisted) => {
                        if *gen.peek() != current_gen {
                            return;
                        }
                        // Clear any previous fetch error on success
                        fetch_error.set(None);
                        let mut by_pubkey =
                            std::collections::HashMap::<String, PersistedReview>::new();
                        for r in reviews.read().iter().cloned() {
                            by_pubkey
                                .entry(r.pubkey.clone())
                                .and_modify(|existing| {
                                    if should_replace_review(existing, &r) {
                                        *existing = r.clone();
                                    }
                                })
                                .or_insert(r);
                        }
                        for r in persisted {
                            by_pubkey
                                .entry(r.pubkey.clone())
                                .and_modify(|existing| {
                                    if should_replace_review(existing, &r) {
                                        *existing = r.clone();
                                    }
                                })
                                .or_insert(r);
                        }
                        let mut sorted: Vec<_> = by_pubkey.into_values().collect();
                        sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                        reviews.set(sorted);
                    }
                    Err(e) => {
                        if *gen.peek() != current_gen {
                            return;
                        }
                        log::warn!("Failed to fetch PR reviews for {}: {}", id, e);
                        fetch_error.set(Some(format!("Failed to load reviews: {}", e)));
                    }
                }
            });
        },
    ));

    let review_list = reviews.read();
    let maintainer_set: HashSet<&String> = HashSet::from_iter(maintainers.iter());
    let approve_count = review_list
        .iter()
        .filter(|r| {
            r.state == crate::utils::nip34::ReviewState::Approved
                && maintainer_set.contains(&r.pubkey)
                && r.pubkey != pr_pubkey
        })
        .count();
    let changes_count = review_list
        .iter()
        .filter(|r| {
            r.state == crate::utils::nip34::ReviewState::ChangesRequested
                && maintainer_set.contains(&r.pubkey)
                && r.pubkey != pr_pubkey
        })
        .count();

    let handle_submit = {
        let pr_id = pr_id.clone();
        let user_pubkey = user_pubkey.clone();
        let saved_pr_pubkey = pr_pubkey.clone();
        move |_| {
            if *submitting.peek() {
                return;
            }
            if !*HAS_SIGNER.read() {
                publish_error.set(Some("No signer attached".to_string()));
                return;
            }
            submitting.set(true);
            let state = *selected_state.read();
            let body = review_body.read().clone();
            let content = if body.trim().is_empty() {
                String::new()
            } else {
                body
            };
            let review_state = state.to_review_state();
            let now = crate::platform::timestamp::now_secs();
            // Capture prior review for rollback
            let prior_review = {
                let current = reviews.read();
                current.iter().find(|r| r.pubkey == user_pubkey).cloned()
            };
            // Add to local display immediately
            let local_review = PersistedReview {
                pr_event_id: pr_id.clone(),
                content: content.clone(),
                state: review_state,
                event_id: String::new(),
                pubkey: user_pubkey.clone(),
                created_at: now,
            };
            let mut current = reviews.write();
            current.retain(|r| r.pubkey != user_pubkey);
            current.push(local_review);
            current.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            drop(current);
            show_form.set(false);
            review_body.set(String::new());
            publish_error.set(None);
            // Publish to relays
            let id = pr_id.clone();
            let saved_content = content.clone();
            let saved_pubkey = user_pubkey.clone();
            let author_pk = saved_pr_pubkey.clone();
            let on_review_submitted = on_review_submitted;
            let submit_gen = *gen.peek();
            spawn(async move {
                match publish_review_event(&id, &author_pk, review_state, &saved_content).await {
                    Ok(event_id) => {
                        if *gen.peek() != submit_gen {
                            return;
                        }
                        let mut current = reviews.write();
                        if let Some(r) = current.iter_mut().find(|r| {
                            r.pubkey == saved_pubkey
                                && r.content == saved_content
                                && r.event_id.is_empty()
                        }) {
                            r.event_id = event_id;
                        }
                        drop(current);
                        submitting.set(false);
                        if let Some(handler) = on_review_submitted.as_ref() {
                            handler.call(());
                        }
                    }
                    Err(e) => {
                        if *gen.peek() != submit_gen {
                            return;
                        }
                        submitting.set(false);
                        publish_error.set(Some(e.to_string()));
                        // Rollback: remove the optimistic entry and restore prior review
                        let mut current = reviews.write();
                        current.retain(|r| {
                            !(r.pubkey == saved_pubkey
                                && r.content == saved_content
                                && r.event_id.is_empty())
                        });
                        if let Some(prior) = prior_review {
                            // Only restore if no newer persisted review exists for this pubkey
                            let has_newer = current.iter().any(|r| {
                                r.pubkey == prior.pubkey && r.created_at >= prior.created_at
                            });
                            if !has_newer {
                                current.push(prior);
                                current.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                            }
                        }
                        drop(current);
                        // Restore form so user can retry
                        show_form.set(true);
                        if review_body.peek().is_empty() {
                            review_body.set(saved_content);
                        }
                    }
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
                            let met = approve_count >= required as usize && changes_count == 0;
                            rsx! {
                                div { class: if changes_count > 0 { "text-xs text-orange-500 flex items-center gap-1" } else if met { "text-xs text-green-500 flex items-center gap-1" } else { "text-xs text-muted-foreground flex items-center gap-1" },
                                    if changes_count > 0 {
                                        "Changes requested ({changes_count} outstanding)"
                                    } else if met {
                                        "Required approvals met ({approve_count}/{required})"
                                    } else {
                                        "Requires {required} approval(s), has {approve_count}"
                                    }
                                }
                            }
                        }
                    }
                }
                // Fetch error display
                if let Some(err) = fetch_error.read().as_ref() {
                    div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                        "{err}"
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
                                let review_key = if review.event_id.is_empty() { format!("{}_{}", review.pubkey, review.created_at) } else { review.event_id.clone() };
                                rsx! {
                                    div {
                                        key: "{review_key}",
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
                // Publish error display
                if let Some(err) = publish_error.read().as_ref() {
                    div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                        "{err}"
                    }
                }
                // Add Review button / form
                if can_review {
                    if *show_form.read() {
                        div { class: "bg-card border border-border rounded-lg p-4 space-y-3",
                            div { class: "bg-muted rounded-lg overflow-hidden p-1 flex",
                                button {
                                    class: if *selected_state.read() == LocalReviewState::Approved {
                                        "flex-1 px-2 py-1.5 text-xs border-2 border-green-500 bg-green-500/10 text-green-500 font-medium"
                                    } else {
                                        "flex-1 px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent/50 transition"
                                    },
                                    onclick: move |_| selected_state.set(LocalReviewState::Approved),
                                    "Approve"
                                }
                                button {
                                    class: if *selected_state.read() == LocalReviewState::ChangesRequested {
                                        "flex-1 px-2 py-1.5 text-xs border-2 border-orange-500 bg-orange-500/10 text-orange-500 font-medium"
                                    } else {
                                        "flex-1 px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent/50 transition"
                                    },
                                    onclick: move |_| selected_state.set(LocalReviewState::ChangesRequested),
                                    "Request Changes"
                                }
                                button {
                                    class: if *selected_state.read() == LocalReviewState::Commented {
                                        "flex-1 px-2 py-1.5 text-xs border-2 border-blue-500 bg-blue-500/10 text-blue-500 font-medium"
                                    } else {
                                        "flex-1 px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent/50 transition"
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
                                    class: "px-3 py-1.5 text-xs bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50",
                                    disabled: *submitting.read(),
                                    onclick: handle_submit,
                                    if *submitting.read() { "Submitting..." } else { "Submit Review" }
                                }
                            }
                        }
                    } else {
                        button {
                            class: "w-full px-3 py-2 text-sm border border-border rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent/50 transition",
                            onclick: move |_| {
                                if *submitting.peek() {
                                    return;
                                }
                                show_form.set(true);
                            },
                            "Add Review"
                        }
                    }
                }
            }
        }
    }
}
