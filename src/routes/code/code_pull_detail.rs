//! Pull Request Detail Page
//!
//! View a single NIP-34 Git patch/PR (Kind 1617) with comments,
//! diff viewer, code reviews, and merge controls.
use crate::components::code::{DiffViewer, PRReviewSection};
use crate::components::{icons, CodeStatusBadge};
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_pr_comments_by_id, fetch_pull_request, fetch_repository, publish_pr_comment_by_id,
    publish_pr_update_by_id, update_pr_status_by_id,
};
use crate::services::git_hosting::conflict_detection::{detect_conflicts, ConflictInfo, ConflictType};
use crate::services::git_hosting::pull_requests::{
    fetch_line_comments_by_id, publish_line_comment_by_id, LineComment,
};
use crate::stores::profiles::PROFILE_CACHE;
use crate::stores::{auth_store, nostr_client};
use crate::utils::format::{truncate_commit, truncate_pubkey};
use crate::utils::format_relative_time_or;
use crate::services::git_hosting::reviews::fetch_pr_reviews;
use crate::utils::nip34::{GitComment, IssueStatus, PersistedReview, PullRequest, Repository, ReviewState};
use crate::utils::permissions;
use dioxus::prelude::*;
use std::collections::HashMap;

/// Richer approval status returned by the approval resource
#[derive(Clone, PartialEq, Default)]
struct ApprovalStatus {
    approved: u32,
    has_changes_requested: bool,
    fetch_error: bool,
}

/// PR detail tab
#[derive(Clone, Copy, PartialEq)]
enum PrTab {
    Conversation,
    FilesChanged,
}

/// Pull request detail page component
#[component]
pub fn CodePullDetail(note_id: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut pr_result = use_signal(|| None::<Result<PullRequest, String>>);
    let mut refresh_trigger = use_signal(|| 0u32);
    let mut pr_gen = use_signal(|| 0u32);
    use_effect(use_reactive((&note_id, &refresh_trigger), move |(note_id, _trigger)| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let gen = pr_gen.peek().wrapping_add(1);
        pr_gen.set(gen);
        pr_result.set(None);
        spawn(async move {
            let result = fetch_pull_request(&note_id).await;
            if *pr_gen.peek() == gen {
                pr_result.set(Some(result));
            }
        });
    }));
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeHome {},
                        class: "text-muted-foreground hover:text-foreground",
                        dangerous_inner_html: icons::ARROW_LEFT,
                    }
                    h1 { class: "text-xl font-bold flex items-center gap-2",
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "18", cy: "18", r: "3" }
                            circle { cx: "6", cy: "6", r: "3" }
                            path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                            line {
                                x1: "6",
                                y1: "9",
                                x2: "6",
                                y2: "21",
                            }
                        }
                        "Pull Request"
                    }
                }
            }
            div { class: "p-4",
                if !*nostr_client::CLIENT_INITIALIZED.read()
                    || pr_result.read().is_none()
                {
                    LoadingSkeleton {}
                } else {
                    match pr_result.read().as_ref() {
                        Some(Ok(pr)) => rsx! {
                            PRContent {
                                pr: pr.clone(),
                                is_authenticated: auth.is_authenticated,
                                user_pubkey: auth.pubkey.clone().unwrap_or_default(),
                                on_pr_updated: move |_| {
                                    refresh_trigger.with_mut(|v| *v = v.wrapping_add(1));
                                },
                            }
                        },
                        Some(Err(e)) => rsx! {
                            ErrorState { message: e.clone() }
                        },
                        None => rsx! {
                            LoadingSkeleton {}
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn PRContent(pr: PullRequest, is_authenticated: bool, user_pubkey: String, on_pr_updated: EventHandler) -> Element {
    let pr_id = pr.event_id.clone();
    let pr_pubkey = pr.pubkey.clone();
    let mut display_status = use_signal(|| pr.status);
    use_effect(use_reactive(&pr.status, move |status| {
        display_status.set(status);
    }));
    let author_profile = PROFILE_CACHE.read().peek(&pr.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| pr.pubkey_display());

    // Fetch repository for permission checks
    let mut repo = use_signal(|| None::<Repository>);
    let mut repo_gen = use_signal(|| 0u32);
    let repo_naddr = pr.repository_naddr.clone();
    use_effect(use_reactive(&repo_naddr, move |naddr| {
        let gen = repo_gen.peek().wrapping_add(1);
        repo_gen.set(gen);
        repo.set(None);
        if naddr.is_empty() {
            return;
        }
        spawn(async move {
            if let Ok(r) = fetch_repository(&naddr).await {
                if *repo_gen.peek() == gen {
                    repo.set(Some(r));
                }
            }
        });
    }));

    // Permission checks: can_change_status includes author check; fall back to author-only when repo not loaded
    let can_update_status = is_authenticated
        && repo
            .read()
            .as_ref()
            .map(|r| permissions::can_change_status(&user_pubkey, r, &pr.pubkey))
            .unwrap_or(user_pubkey == pr.pubkey);
    let required_approvals = repo
        .read()
        .as_ref()
        .map(|r| r.required_approvals)
        .unwrap_or(0);

    // Fetch reviews to check approval count
    // Generation counter bumped when a new review is submitted, causing use_resource to refetch
    let mut reviews_gen = use_signal(|| 0u32);
    let pr_id_for_reviews = pr_id.clone();
    let approval_status = use_resource(move || {
        let _ = reviews_gen.read();
        let id = pr_id_for_reviews.clone();
        let maintainer_set: Vec<String> = repo.read().as_ref()
            .map(|r| {
                let mut m = r.maintainers.clone();
                if !m.contains(&r.pubkey) { m.push(r.pubkey.clone()); }
                m
            })
            .unwrap_or_default();
        async move {
            match fetch_pr_reviews(&id).await {
                Ok(reviews) => {
                    let mut latest: HashMap<&str, &PersistedReview> =
                        HashMap::new();
                    for r in &reviews {
                        match latest.get(r.pubkey.as_str()) {
                            Some(existing) if existing.created_at >= r.created_at => {}
                            _ => { latest.insert(r.pubkey.as_str(), r); }
                        }
                    }
                    let approved = latest.values()
                        .filter(|r| r.state == ReviewState::Approved && (maintainer_set.is_empty() || maintainer_set.contains(&r.pubkey)))
                        .count() as u32;
                    let has_changes_requested = latest.values()
                        .any(|r| r.state == ReviewState::ChangesRequested && (maintainer_set.is_empty() || maintainer_set.contains(&r.pubkey)));
                    ApprovalStatus {
                        approved,
                        has_changes_requested,
                        fetch_error: false,
                    }
                }
                Err(_) => ApprovalStatus {
                    approved: 0,
                    has_changes_requested: false,
                    fetch_error: true,
                },
            }
        }
    });

    let has_enough_approvals = required_approvals == 0
        || approval_status
            .read()
            .as_ref()
            .map(|status| status.approved >= required_approvals && !status.has_changes_requested)
            .unwrap_or(false);

    let can_merge = is_authenticated
        && has_enough_approvals
        && repo
            .read()
            .as_ref()
            .map(|r| permissions::can_merge(&user_pubkey, r))
            .unwrap_or(false);

    let maintainers: Vec<String> = repo
        .read()
        .as_ref()
        .map(|r| {
            let mut m = r.maintainers.clone();
            if !m.contains(&r.pubkey) {
                m.push(r.pubkey.clone());
            }
            m
        })
        .unwrap_or_default();

    let is_pr_author = is_authenticated && user_pubkey == pr.pubkey;

    let mut active_tab = use_signal(|| PrTab::Conversation);
    let mut new_comment = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut comment_error = use_signal(|| None::<String>);
    let mut is_updating_status = use_signal(|| false);
    let mut status_error = use_signal(|| None::<String>);
    let mut show_merge_confirm = use_signal(|| false);
    let mut show_update_form = use_signal(|| false);
    let mut update_content = use_signal(String::new);
    let mut update_commit = use_signal(String::new);
    let mut update_parent = use_signal(String::new);
    let mut is_submitting_update = use_signal(|| false);
    let mut update_error = use_signal(|| None::<String>);

    let pr_id_for_comments = pr_id.clone();
    let mut comments_gen = use_signal(|| 0u32);
    let comments = use_resource(move || {
        let id = pr_id_for_comments.clone();
        let _gen = *comments_gen.read(); // auto-dependency: reading this triggers refetch on increment
        async move { fetch_pr_comments_by_id(&id).await }
    });

    // Fetch line-level comments for the Files Changed tab
    let mut line_comments: Signal<Vec<LineComment>> = use_signal(Vec::new);
    let mut line_comment_error = use_signal(|| None::<String>);
    let mut line_load_gen = use_signal(|| 0u32);
    let mut line_publish_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&pr_id, move |id| {
        let gen = line_load_gen.peek().wrapping_add(1);
        line_load_gen.set(gen);
        line_comments.set(Vec::new());
        line_comment_error.set(None);
        spawn(async move {
            match fetch_line_comments_by_id(&id).await {
                Ok(lcs) => {
                    if *line_load_gen.peek() == gen {
                        line_comments.set(lcs);
                    }
                }
                Err(e) => {
                    if *line_load_gen.peek() == gen {
                        line_comment_error.set(Some(e));
                    }
                }
            }
        });
    }));

    let handle_status_change = {
        let pr_id = pr_id.clone();
        move |new_status: IssueStatus| {
            if *is_updating_status.peek() {
                return;
            }
            is_updating_status.set(true);
            let id = pr_id.clone();
            spawn(async move {
                match update_pr_status_by_id(&id, new_status).await {
                    Ok(_) => {
                        display_status.set(new_status);
                        status_error.set(None);
                    }
                    Err(e) => {
                        status_error.set(Some(format!("Failed to update status: {}", e)));
                    }
                }
                is_updating_status.set(false);
            });
        }
    };

    let handle_merge = {
        let mut handler = handle_status_change.clone();
        move |_| {
            show_merge_confirm.set(false);
            handler(IssueStatus::Applied);
        }
    };

    let handle_submit_comment = {
        let pr_id = pr_id.clone();
        let pr_author = pr_pubkey.clone();
        move |_| {
            let content = new_comment.read().clone();
            if content.trim().is_empty() {
                return;
            }
            if *is_submitting.peek() {
                return;
            }
            is_submitting.set(true);
            comment_error.set(None);
            let id = pr_id.clone();
            let author = pr_author.clone();
            spawn(async move {
                match publish_pr_comment_by_id(&id, &author, &content).await {
                    Ok(_) => {
                        new_comment.set(String::new());
                        comments_gen.with_mut(|v| *v = v.wrapping_add(1));
                    }
                    Err(e) => {
                        comment_error.set(Some(e));
                    }
                }
                is_submitting.set(false);
            });
        }
    };

    let handle_submit_update = {
        let pr_id = pr_id.clone();
        let repo_naddr = pr.repository_naddr.clone();
        move |_| {
            if *is_submitting_update.peek() { return; }
            let content = update_content.read().clone();
            let commit = update_commit.read().trim().to_string();
            let parent = update_parent.read().trim().to_string();
            let id = pr_id.clone();
            let naddr = repo_naddr.clone();
            if content.trim().is_empty() {
                update_error.set(Some("Update content is required".to_string()));
                return;
            }
            if !commit.is_empty()
                && (!matches!(commit.len(), 40 | 64) || !commit.chars().all(|c| c.is_ascii_hexdigit()))
            {
                update_error.set(Some("Commit hash must be a 40 or 64-character hex string".to_string()));
                return;
            }
            if !parent.is_empty()
                && (!matches!(parent.len(), 40 | 64) || !parent.chars().all(|c| c.is_ascii_hexdigit()))
            {
                update_error.set(Some("Parent commit must be a 40 or 64-character hex string".to_string()));
                return;
            }
            is_submitting_update.set(true);
            update_error.set(None);
            spawn(async move {
                let commit_opt = if commit.is_empty() { None } else { Some(commit.as_str()) };
                let parent_opt = if parent.is_empty() { None } else { Some(parent.as_str()) };
                match publish_pr_update_by_id(&id, &naddr, &content, commit_opt, parent_opt).await {
                    Ok(_) => {
                        show_update_form.set(false);
                        update_content.set(String::new());
                        update_commit.set(String::new());
                        update_parent.set(String::new());
                        on_pr_updated.call(());
                    }
                    Err(e) => {
                        update_error.set(Some(e));
                    }
                }
                is_submitting_update.set(false);
            });
        }
    };

    let title = if pr.is_cover_letter {
        pr.content.lines().next().map(|s| s.to_string())
    } else {
        None
    };

    let current_tab = *active_tab.read();

    rsx! {
        div { class: "space-y-6",
            // Header section
            div { class: "space-y-4",
                div { class: "flex items-start justify-between gap-4",
                    h1 { class: "text-xl font-semibold",
                        if let Some(t) = &title {
                            "{t}"
                        } else if pr.is_cover_letter {
                            "Patch Set"
                        } else {
                            "PR #{pr.event_id.chars().take(8).collect::<String>()}"
                        }
                    }
                    CodeStatusBadge { status: *display_status.read() }
                }
                div { class: "flex flex-wrap gap-2",
                    if pr.is_cover_letter {
                        span { class: "px-2 py-0.5 text-xs rounded-full bg-purple-500/10 text-purple-500 border border-purple-500/20",
                            "Cover Letter"
                        }
                    } else {
                        span { class: "px-2 py-0.5 text-xs rounded-full bg-green-500/10 text-green-500 border border-green-500/20",
                            "Patch"
                        }
                    }
                    if *display_status.read() == IssueStatus::Applied {
                        span { class: "px-2 py-0.5 text-xs rounded-full bg-purple-500/10 text-purple-500 border border-purple-500/20",
                            "Merged"
                        }
                    }
                }
                div { class: "flex items-center gap-3",
                    Link {
                        to: Route::Profile {
                            pubkey: pr.pubkey.clone(),
                        },
                        class: "flex items-center gap-2 hover:underline",
                        div { class: "w-6 h-6 rounded-full bg-muted flex items-center justify-center overflow-hidden",
                            if let Some(picture) = author_profile.as_ref().and_then(|p| p.picture.as_ref()) {
                                img {
                                    class: "w-full h-full object-cover",
                                    src: "{picture}",
                                    alt: "Author",
                                }
                            } else {
                                span { class: "text-xs", "{author_name.chars().next().unwrap_or('?')}" }
                            }
                        }
                        span { class: "text-sm font-medium", "{author_name}" }
                    }
                    span { class: "text-sm text-muted-foreground",
                        "opened "
                        {format_relative_time_or(pr.created_at, "Unknown")}
                    }
                }
                if let Some(branch) = &pr.branch_name {
                    div { class: "flex items-center gap-2 text-sm",
                        span { class: "text-muted-foreground", "Branch:" }
                        code { class: "px-2 py-0.5 bg-muted rounded font-mono text-xs",
                            "{branch}"
                        }
                    }
                }
                if let Some(commit) = &pr.commit {
                    div { class: "flex items-center gap-2 text-sm",
                        span { class: "text-muted-foreground", "Commit:" }
                        code { class: "px-2 py-0.5 bg-muted rounded font-mono text-xs",
                            "{truncate_commit(commit)}"
                        }
                    }
                }
                if let Some(parent) = &pr.parent_commit {
                    div { class: "flex items-center gap-2 text-sm",
                        span { class: "text-muted-foreground", "Parent:" }
                        code { class: "px-2 py-0.5 bg-muted rounded font-mono text-xs",
                            "{truncate_commit(parent)}"
                        }
                    }
                }
                if !pr.labels.is_empty() {
                    div { class: "flex flex-wrap gap-2",
                        for label in pr.labels.iter() {
                            span {
                                key: "{label}",
                                class: "px-2 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-500 border border-blue-500/20",
                                "{label}"
                            }
                        }
                    }
                }
            }

            // Status error display
            if let Some(err) = status_error.read().as_ref() {
                div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                    "{err}"
                }
            }

            // Status actions
            if can_update_status || can_merge {
                div { class: "flex flex-wrap gap-2",
                    if can_merge && matches!(*display_status.read(), IssueStatus::Open | IssueStatus::Draft) {
                        button {
                            class: "px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 flex items-center gap-2",
                            disabled: *is_updating_status.read(),
                            onclick: move |_| show_merge_confirm.set(true),
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
                                polyline { points: "16 18 22 12 16 6" }
                                polyline { points: "8 6 2 12 8 18" }
                            }
                            "Merge Pull Request"
                        }
                    }
                    if can_update_status && *display_status.read() != IssueStatus::Closed && *display_status.read() != IssueStatus::Applied {
                        button {
                            class: "px-3 py-1.5 text-sm bg-destructive/10 text-destructive rounded-lg hover:bg-destructive/20 transition disabled:opacity-50",
                            disabled: *is_updating_status.read(),
                            onclick: {
                                let mut handler = handle_status_change.clone();
                                move |_| handler(IssueStatus::Closed)
                            },
                            "Close PR"
                        }
                    }
                    if can_update_status && *display_status.read() == IssueStatus::Closed {
                        button {
                            class: "px-3 py-1.5 text-sm bg-primary/10 text-primary rounded-lg hover:bg-primary/20 transition disabled:opacity-50",
                            disabled: *is_updating_status.read(),
                            onclick: {
                                let mut handler = handle_status_change.clone();
                                move |_| handler(IssueStatus::Open)
                            },
                            "Reopen PR"
                        }
                    }
                }
            }

            // Approval requirement notice
            if required_approvals > 0 && *display_status.read() != IssueStatus::Applied {
                {
                    let status = approval_status.read();
                    let status_ref = status.as_ref();
                    let current_approvals = status_ref.map(|s| s.approved).unwrap_or(0);
                    let changes_requested = status_ref.map(|s| s.has_changes_requested).unwrap_or(false);
                    let has_fetch_error = status_ref.map(|s| s.fetch_error).unwrap_or(false);
                    let met = current_approvals >= required_approvals && !changes_requested;
                    let color_class = if changes_requested {
                        "bg-destructive/10 border-destructive/20 text-destructive"
                    } else if met {
                        "bg-green-500/10 border-green-500/20 text-green-600"
                    } else {
                        "bg-orange-500/10 border-orange-500/20 text-orange-600"
                    };
                    rsx! {
                        div { class: "p-3 rounded-lg border text-sm {color_class}",
                            if has_fetch_error {
                                "Unable to fetch review status. Approval data may be incomplete."
                            } else if changes_requested {
                                "Changes requested — resolve feedback before merging ({current_approvals}/{required_approvals} approvals)"
                            } else if met {
                                "Approval requirement met ({current_approvals}/{required_approvals})"
                            } else {
                                "Requires {required_approvals} approval(s) to merge ({current_approvals}/{required_approvals})"
                            }
                        }
                    }
                }
            }

            // Conflict detection banner
            if *display_status.read() == IssueStatus::Open || *display_status.read() == IssueStatus::Draft {
                if let Some(parent) = &pr.parent_commit {
                    ConflictDetectionBanner {
                        parent_commit: parent.clone(),
                        diff_content: pr.content.clone(),
                        repo: repo().clone(),
                    }
                }
            }

            // Update PR button (author only)
            if is_pr_author && *display_status.read() != IssueStatus::Applied && *display_status.read() != IssueStatus::Closed {
                if !*show_update_form.read() {
                    button {
                        class: "px-3 py-1.5 text-sm bg-accent text-foreground rounded-lg hover:bg-accent/80 transition flex items-center gap-1",
                        onclick: move |_| show_update_form.set(true),
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
                            path { d: "M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" }
                        }
                        "Update PR"
                    }
                } else {
                    div { class: "border border-border rounded-lg p-4 space-y-3",
                        h4 { class: "text-sm font-medium", "Push Update (Kind 1619)" }
                        if let Some(err) = update_error.read().as_ref() {
                            div { class: "text-xs text-destructive", "{err}" }
                        }
                        textarea {
                            class: "w-full h-32 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary resize-y",
                            placeholder: "Updated patch content or description...",
                            value: "{update_content}",
                            oninput: move |e| update_content.set(e.value()),
                        }
                        div { class: "grid grid-cols-2 gap-3",
                            input {
                                class: "px-3 py-2 bg-muted rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary font-mono",
                                r#type: "text",
                                placeholder: "New commit hash (optional)",
                                value: "{update_commit}",
                                oninput: move |e| update_commit.set(e.value()),
                            }
                            input {
                                class: "px-3 py-2 bg-muted rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary font-mono",
                                r#type: "text",
                                placeholder: "Parent commit (optional)",
                                value: "{update_parent}",
                                oninput: move |e| update_parent.set(e.value()),
                            }
                        }
                        div { class: "flex justify-end gap-2",
                            button {
                                class: "px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground transition",
                                onclick: move |_| show_update_form.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50",
                                disabled: *is_submitting_update.read(),
                                onclick: handle_submit_update,
                                if *is_submitting_update.read() {
                                    "Submitting..."
                                } else {
                                    "Submit Update"
                                }
                            }
                        }
                    }
                }
            }

            // Merge confirmation modal
            if *show_merge_confirm.read() {
                div { class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center",
                    onclick: move |_| show_merge_confirm.set(false),
                    div {
                        class: "bg-background border border-border rounded-xl p-6 max-w-md mx-4 shadow-lg",
                        onclick: move |e| e.stop_propagation(),
                        h3 { class: "text-lg font-semibold mb-2", "Merge Pull Request" }
                        p { class: "text-sm text-muted-foreground mb-4",
                            "This will mark the pull request as merged (Applied). This action publishes a status event to Nostr relays."
                        }
                        div { class: "flex justify-end gap-3",
                            button {
                                class: "px-4 py-2 text-sm text-muted-foreground hover:text-foreground transition",
                                onclick: move |_| show_merge_confirm.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                onclick: handle_merge,
                                "Confirm Merge"
                            }
                        }
                    }
                }
            }

            // Tab navigation
            div { class: "flex items-center gap-1 border-b border-border",
                button {
                    class: if current_tab == PrTab::Conversation {
                        "flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium border-b-2 border-primary text-foreground -mb-px"
                    } else {
                        "flex items-center gap-1.5 px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground hover:bg-accent/50 transition -mb-px border-b-2 border-transparent"
                    },
                    onclick: move |_| active_tab.set(PrTab::Conversation),
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
                        path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                    }
                    "Conversation"
                    span { class: "ml-1 px-1.5 py-0.5 text-xs rounded-full bg-muted text-muted-foreground",
                        "{pr.comment_count}"
                    }
                }
                button {
                    class: if current_tab == PrTab::FilesChanged {
                        "flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium border-b-2 border-primary text-foreground -mb-px"
                    } else {
                        "flex items-center gap-1.5 px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground hover:bg-accent/50 transition -mb-px border-b-2 border-transparent"
                    },
                    onclick: move |_| active_tab.set(PrTab::FilesChanged),
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
                        path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                        polyline { points: "14 2 14 8 20 8" }
                    }
                    "Files Changed"
                }
            }

            // Tab content
            match current_tab {
                PrTab::Conversation => rsx! {
                    div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                        // Main content: comments
                        div { class: "lg:col-span-2 space-y-4",
                            match &*comments.read() {
                                Some(Ok(comment_list)) => rsx! {
                                    div { class: "space-y-4",
                                        for comment in comment_list.iter() {
                                            CommentCard { key: "{comment.event_id}", comment: comment.clone() }
                                        }
                                        if comment_list.is_empty() {
                                            p { class: "text-sm text-muted-foreground text-center py-4",
                                                "No comments yet. Be the first to comment!"
                                            }
                                        }
                                    }
                                },
                                Some(Err(e)) => rsx! {
                                    p { class: "text-sm text-destructive", "Failed to load comments: {e}" }
                                },
                                None => rsx! {
                                    div { class: "space-y-3",
                                        for i in 0..2 {
                                            div {
                                                key: "{i}",
                                                class: "p-3 border border-border rounded-lg animate-pulse",
                                                div { class: "h-4 bg-muted rounded w-1/4 mb-2" }
                                                div { class: "h-3 bg-muted rounded w-3/4" }
                                            }
                                        }
                                    }
                                },
                            }
                            // Comment form
                            if is_authenticated {
                                div { class: "border border-border rounded-lg overflow-hidden",
                                    textarea {
                                        class: "w-full p-3 text-sm bg-background resize-none focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                                        placeholder: "Write a comment...",
                                        rows: 3,
                                        value: "{new_comment}",
                                        oninput: move |e| new_comment.set(e.value()),
                                    }
                                    div { class: "px-3 py-2 bg-muted/50 border-t border-border flex items-center justify-between",
                                        if let Some(error) = comment_error.read().as_ref() {
                                            span { class: "text-xs text-destructive", "{error}" }
                                        } else {
                                            span { class: "text-xs text-muted-foreground", "Plain text" }
                                        }
                                        button {
                                            class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50",
                                            disabled: *is_submitting.read() || new_comment.read().trim().is_empty(),
                                            onclick: handle_submit_comment,
                                            if *is_submitting.read() {
                                                "Submitting..."
                                            } else {
                                                "Comment"
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "p-4 bg-muted rounded-lg text-center",
                                    p { class: "text-sm text-muted-foreground", "Sign in to leave a comment" }
                                }
                            }
                        }

                        // Sidebar: reviews
                        div { class: "space-y-4",
                            PRReviewSection {
                                pr_id: pr_id.clone(),
                                pr_pubkey: pr_pubkey.clone(),
                                maintainers: maintainers.clone(),
                                user_pubkey: user_pubkey.clone(),
                                is_authenticated: is_authenticated,
                                on_review_submitted: move |_| {
                                    reviews_gen.with_mut(|v| *v = v.wrapping_add(1));
                                },
                            }

                            // Linked issues
                            if !pr.linked_issues.is_empty() {
                                div { class: "border border-border rounded-lg p-3",
                                    h4 { class: "text-xs font-medium text-muted-foreground mb-2", "Linked Issues" }
                                    div { class: "space-y-1",
                                        for issue_id in pr.linked_issues.iter() {
                                            Link {
                                                key: "{issue_id}",
                                                to: Route::CodeIssueDetail {
                                                    note_id: issue_id.clone(),
                                                },
                                                class: "flex items-center gap-2 text-sm text-primary hover:underline",
                                                svg {
                                                    class: "w-3 h-3 shrink-0",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    circle { cx: "12", cy: "12", r: "10" }
                                                    line { x1: "12", y1: "8", x2: "12", y2: "12" }
                                                    line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                                                }
                                                span { class: "font-mono text-xs truncate",
                                                    "Closes #{issue_id.chars().take(8).collect::<String>()}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Repository link
                            if !pr.repository_naddr.is_empty() {
                                div { class: "border border-border rounded-lg p-3",
                                    h4 { class: "text-xs font-medium text-muted-foreground mb-2", "Repository" }
                                    Link {
                                        to: Route::CodeRepo {
                                            naddr: pr.repository_naddr.clone(),
                                        },
                                        class: "text-sm text-primary hover:underline",
                                        if pr.repository_naddr.chars().count() > 30 {
                                            "{pr.repository_naddr.chars().take(30).collect::<String>()}..."
                                        } else {
                                            "{pr.repository_naddr}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                PrTab::FilesChanged => {
                    let pr_id_for_handler = pr_id.clone();
                    let pr_author_for_handler = pr_pubkey.clone();
                    rsx! {
                        if let Some(err) = line_comment_error.read().as_ref() {
                            div { class: "mb-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                                "{err}"
                            }
                        }
                        DiffViewer {
                            content: pr.content.clone(),
                            is_cover_letter: pr.is_cover_letter,
                            pr_event_id: Some(pr_id.clone()),
                            line_comments: line_comments.read().clone(),
                            on_line_comment: move |(file, line_num, text): (String, usize, String)| {
                                let id = pr_id_for_handler.clone();
                                let author = pr_author_for_handler.clone();
                                line_comment_error.set(None);
                                let gen = line_publish_gen.peek().wrapping_add(1);
                                line_publish_gen.set(gen);
                                spawn(async move {
                                    match publish_line_comment_by_id(&id, &author, &text, &file, line_num).await {
                                        Ok(_) => {
                                            // Refresh line comments after publishing
                                            match fetch_line_comments_by_id(&id).await {
                                                Ok(lcs) => {
                                                    if *line_publish_gen.peek() == gen {
                                                        line_comments.set(lcs);
                                                    }
                                                }
                                                Err(e) => {
                                                    if *line_publish_gen.peek() == gen {
                                                        log::warn!("Failed to refresh line comments: {}", e);
                                                        line_comment_error.set(Some(format!("Failed to refresh line comments: {}", e)));
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            if *line_publish_gen.peek() == gen {
                                                line_comment_error.set(Some(format!("Failed to publish line comment: {}", e)));
                                            }
                                        }
                                    }
                                });
                            },
                        }
                    }
                },
            }

            // Footer
            div { class: "pt-4 border-t border-border text-xs text-muted-foreground space-y-1",
                p { "Event ID: {pr.event_id}" }
            }
        }
    }
}

#[component]
fn CommentCard(comment: GitComment) -> Element {
    let author_profile = PROFILE_CACHE.read().peek(&comment.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&comment.pubkey));
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4",
            div { class: "flex items-center gap-2 mb-2",
                Link {
                    to: Route::Profile {
                        pubkey: comment.pubkey.clone(),
                    },
                    class: "flex items-center gap-2 hover:underline",
                    div { class: "w-5 h-5 rounded-full bg-muted flex items-center justify-center overflow-hidden",
                        if let Some(picture) = author_profile.as_ref().and_then(|p| p.picture.as_ref()) {
                            img {
                                class: "w-full h-full object-cover",
                                src: "{picture}",
                                alt: "Author",
                            }
                        } else {
                            span { class: "text-[10px]", "{author_name.chars().next().unwrap_or('?')}" }
                        }
                    }
                    span { class: "text-sm font-medium", "{author_name}" }
                }
                span { class: "text-xs text-muted-foreground",
                    {format_relative_time_or(comment.created_at, "Unknown")}
                }
            }
            div { class: "text-sm", "{comment.content}" }
        }
    }
}

#[component]
fn ErrorState(message: String) -> Element {
    rsx! {
        div { class: "text-center py-12",
            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-destructive",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "12", cy: "12", r: "10" }
                    line {
                        x1: "12",
                        y1: "8",
                        x2: "12",
                        y2: "12",
                    }
                    line {
                        x1: "12",
                        y1: "16",
                        x2: "12.01",
                        y2: "16",
                    }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "Pull Request Not Found" }
            p { class: "text-muted-foreground text-sm mb-4", "{message}" }
            Link { to: Route::CodeHome {}, class: "text-primary hover:underline", "Back to Code" }
        }
    }
}

#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-6 animate-pulse",
            div { class: "space-y-4",
                div { class: "h-6 bg-muted rounded w-2/3" }
                div { class: "flex items-center gap-3",
                    div { class: "w-6 h-6 rounded-full bg-muted" }
                    div { class: "h-4 bg-muted rounded w-24" }
                    div { class: "h-4 bg-muted rounded w-20" }
                }
            }
            div { class: "h-48 bg-muted rounded-lg" }
            div { class: "space-y-4",
                div { class: "h-5 bg-muted rounded w-24" }
                for i in 0..2 {
                    div {
                        key: "{i}",
                        class: "p-4 border border-border rounded-lg",
                        div { class: "h-4 bg-muted rounded w-1/4 mb-2" }
                        div { class: "h-3 bg-muted rounded w-3/4" }
                    }
                }
            }
        }
    }
}

/// Conflict detection banner for PRs.
///
/// Shows the parent commit info and runs heuristic conflict detection
/// against the current state of the repository.
#[component]
fn ConflictDetectionBanner(
    parent_commit: String,
    diff_content: String,
    repo: Option<Repository>,
) -> Element {
    let short_parent = truncate_commit(&parent_commit);
    let mut conflicts = use_signal(Vec::<ConflictInfo>::new);
    let mut checking = use_signal(|| false);
    let mut show_details = use_signal(|| false);
    let mut detect_gen = use_signal(|| 0u32);

    // Run conflict detection when repo is available
    let repo_key = repo.as_ref().map(|r| r.name.clone()).unwrap_or_default();
    use_effect(use_reactive(
        (&diff_content, &parent_commit, &repo_key),
        move |(diff, parent, _repo_key)| {
            let gen = detect_gen.peek().wrapping_add(1);
            detect_gen.set(gen);
            conflicts.set(Vec::new());
            if diff.is_empty() {
                return;
            }
            if let Some(r) = repo.as_ref() {
                let r = r.clone();
                checking.set(true);
                spawn(async move {
                    let results = detect_conflicts(&r, &diff, &parent).await;
                    if *detect_gen.peek() != gen { return; }
                    conflicts.set(results);
                    checking.set(false);
                });
            }
        },
    ));

    let conflict_count = conflicts.read().len();
    let (banner_class, icon_class) = if conflict_count > 0 {
        (
            "p-3 rounded-lg border bg-orange-500/10 border-orange-500/20 text-sm",
            "w-4 h-4 text-orange-400 shrink-0 mt-0.5",
        )
    } else {
        (
            "p-3 rounded-lg border bg-blue-500/10 border-blue-500/20 text-sm",
            "w-4 h-4 text-blue-500 shrink-0 mt-0.5",
        )
    };

    rsx! {
        div { class: "{banner_class}",
            div { class: "flex items-start gap-2",
                svg {
                    class: "{icon_class}",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "12", cy: "12", r: "10" }
                    path { d: "M12 16v-4" }
                    path { d: "M12 8h.01" }
                }
                div { class: "flex-1",
                    p { class: if conflict_count > 0 { "text-orange-400" } else { "text-blue-500" },
                        "Based on commit "
                        code { class: "px-1 py-0.5 bg-blue-500/10 rounded text-xs font-mono", "{short_parent}" }
                        if checking() {
                            " — checking for conflicts..."
                        } else if conflict_count > 0 {
                            " — {conflict_count} potential conflict(s) detected"
                        }
                    }
                    // Conflict details
                    if conflict_count > 0 && !checking() {
                        button {
                            class: "text-xs text-orange-400/80 hover:text-orange-400 mt-1 underline",
                            onclick: move |_| {
                                let current = *show_details.read();
                                show_details.set(!current);
                            },
                            if *show_details.read() { "Hide details" } else { "Show details" }
                        }
                        if *show_details.read() {
                            div { class: "mt-2 space-y-1",
                                for conflict in conflicts.read().iter() {
                                    {
                                        let (type_label, type_class) = match conflict.conflict_type {
                                            ConflictType::ContextMismatch => ("Changed", "text-orange-400"),
                                            ConflictType::FileDeleted => ("Deleted", "text-red-400"),
                                            ConflictType::FileAlreadyExists => ("Exists", "text-yellow-400"),
                                        };
                                        rsx! {
                                            div {
                                                key: "{conflict.file_path}-{type_label}",
                                                class: "flex items-start gap-2 text-xs",
                                                span { class: "px-1.5 py-0.5 rounded bg-muted font-medium shrink-0 {type_class}",
                                                    "{type_label}"
                                                }
                                                div {
                                                    code { class: "font-mono text-muted-foreground", "{conflict.file_path}" }
                                                    p { class: "text-muted-foreground/70 mt-0.5", "{conflict.details}" }
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
    }
}
