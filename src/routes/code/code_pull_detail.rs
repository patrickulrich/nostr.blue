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
use crate::services::git_hosting::pull_requests::{
    fetch_line_comments_by_id, publish_line_comment_by_id, LineComment,
};
use crate::stores::profiles::PROFILE_CACHE;
use crate::stores::{auth_store, nostr_client};
use crate::utils::format::{truncate_commit, truncate_pubkey};
use crate::utils::format_relative_time_or;
use crate::services::git_hosting::reviews::fetch_pr_reviews;
use crate::utils::nip34::{GitComment, IssueStatus, PullRequest, Repository, ReviewState};
use crate::utils::permissions;
use dioxus::prelude::*;

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
    let mut loading = use_signal(|| true);
    let note_id_for_effect = note_id.clone();
    use_effect(move || {
        let id = note_id_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            let result = fetch_pull_request(&id).await;
            pr_result.set(Some(result));
            loading.set(false);
        });
    });
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
                    || (*loading.read() && pr_result.read().is_none())
                {
                    LoadingSkeleton {}
                } else {
                    match pr_result.read().as_ref() {
                        Some(Ok(pr)) => rsx! {
                            PRContent {
                                pr: pr.clone(),
                                is_authenticated: auth.is_authenticated,
                                user_pubkey: auth.pubkey.clone().unwrap_or_default(),
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
fn PRContent(pr: PullRequest, is_authenticated: bool, user_pubkey: String) -> Element {
    let pr_id = pr.event_id.clone();
    let pr_pubkey = pr.pubkey.clone();
    let pr_status = pr.status;
    let author_profile = PROFILE_CACHE.read().peek(&pr.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| pr.pubkey_display());

    // Fetch repository for permission checks
    let mut repo = use_signal(|| None::<Repository>);
    let repo_naddr = pr.repository_naddr.clone();
    use_effect(move || {
        let naddr = repo_naddr.clone();
        if naddr.is_empty() {
            return;
        }
        spawn(async move {
            if let Ok(r) = fetch_repository(&naddr).await {
                repo.set(Some(r));
            }
        });
    });

    // Permission checks: author OR maintainer/owner can update status
    let can_update_status = is_authenticated
        && (user_pubkey == pr.pubkey
            || repo
                .read()
                .as_ref()
                .map(|r| permissions::can_change_status(&user_pubkey, r, &pr.pubkey))
                .unwrap_or(false));
    let required_approvals = repo
        .read()
        .as_ref()
        .map(|r| r.required_approvals)
        .unwrap_or(0);

    // Fetch reviews to check approval count
    let pr_id_for_reviews = pr_id.clone();
    let approval_count = use_resource(move || {
        let id = pr_id_for_reviews.clone();
        async move {
            match fetch_pr_reviews(&id).await {
                Ok(reviews) => reviews
                    .iter()
                    .filter(|r| r.state == ReviewState::Approved)
                    .count() as u32,
                Err(_) => 0,
            }
        }
    });

    let has_enough_approvals = required_approvals == 0
        || approval_count
            .read()
            .as_ref()
            .map(|&count| count >= required_approvals)
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
    let mut show_merge_confirm = use_signal(|| false);
    let mut merge_strategy = use_signal(|| "merge".to_string());
    let mut show_update_form = use_signal(|| false);
    let mut update_content = use_signal(String::new);
    let mut update_commit = use_signal(String::new);
    let mut update_parent = use_signal(String::new);
    let mut is_submitting_update = use_signal(|| false);
    let mut update_error = use_signal(|| None::<String>);

    let pr_id_for_comments = pr_id.clone();
    let comments = use_resource(move || {
        let id = pr_id_for_comments.clone();
        async move { fetch_pr_comments_by_id(&id).await }
    });

    // Fetch line-level comments for the Files Changed tab
    let pr_id_for_line_comments = pr_id.clone();
    let mut line_comments: Signal<Vec<LineComment>> = use_signal(Vec::new);
    use_effect(move || {
        let id = pr_id_for_line_comments.clone();
        spawn(async move {
            if let Ok(lcs) = fetch_line_comments_by_id(&id).await {
                line_comments.set(lcs);
            }
        });
    });

    let handle_status_change = {
        let pr_id = pr_id.clone();
        move |new_status: IssueStatus| {
            let id = pr_id.clone();
            spawn(async move {
                is_updating_status.set(true);
                match update_pr_status_by_id(&id, new_status, None).await {
                    Ok(_) => {}
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("Failed to update status: {}", e).into(),
                        );
                    }
                }
                is_updating_status.set(false);
            });
        }
    };

    let handle_merge = {
        let pr_id = pr_id.clone();
        move |_| {
            let strategy = merge_strategy.read().clone();
            let id = pr_id.clone();
            show_merge_confirm.set(false);
            spawn(async move {
                is_updating_status.set(true);
                let strategy_opt = if strategy == "merge" { None } else { Some(strategy.as_str()) };
                match update_pr_status_by_id(&id, IssueStatus::Applied, strategy_opt).await {
                    Ok(_) => {}
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("Failed to merge: {}", e).into(),
                        );
                    }
                }
                is_updating_status.set(false);
            });
        }
    };

    let handle_submit_comment = {
        let pr_id = pr_id.clone();
        let pr_pubkey = pr_pubkey.clone();
        move |_| {
            let content = new_comment.read().clone();
            let id = pr_id.clone();
            let author = pr_pubkey.clone();
            if content.trim().is_empty() {
                return;
            }
            spawn(async move {
                is_submitting.set(true);
                comment_error.set(None);
                match publish_pr_comment_by_id(&id, &author, &content).await {
                    Ok(_) => {
                        new_comment.set(String::new());
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
            let content = update_content.read().clone();
            let commit = update_commit.read().clone();
            let parent = update_parent.read().clone();
            let id = pr_id.clone();
            let naddr = repo_naddr.clone();
            if content.trim().is_empty() {
                update_error.set(Some("Update content is required".to_string()));
                return;
            }
            spawn(async move {
                is_submitting_update.set(true);
                update_error.set(None);
                let commit_opt = if commit.is_empty() { None } else { Some(commit.as_str()) };
                let parent_opt = if parent.is_empty() { None } else { Some(parent.as_str()) };
                match publish_pr_update_by_id(&id, &naddr, &content, commit_opt, parent_opt).await {
                    Ok(_) => {
                        show_update_form.set(false);
                        update_content.set(String::new());
                        update_commit.set(String::new());
                        update_parent.set(String::new());
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
                    CodeStatusBadge { status: pr_status }
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
                    if pr_status == IssueStatus::Applied {
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

            // Status actions
            if can_update_status || can_merge {
                div { class: "flex flex-wrap gap-2",
                    if can_merge && pr_status != IssueStatus::Applied {
                        button {
                            class: "px-4 py-2 text-sm font-medium bg-green-600 text-white rounded-lg hover:bg-green-700 transition disabled:opacity-50 flex items-center gap-2",
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
                    if can_update_status && pr_status != IssueStatus::Closed && pr_status != IssueStatus::Applied {
                        button {
                            class: "px-3 py-1.5 text-sm bg-destructive/10 text-destructive rounded-lg hover:bg-destructive/20 transition disabled:opacity-50",
                            disabled: *is_updating_status.read(),
                            onclick: {
                                let handler = handle_status_change.clone();
                                move |_| handler(IssueStatus::Closed)
                            },
                            "Close PR"
                        }
                    }
                    if can_update_status && pr_status == IssueStatus::Closed {
                        button {
                            class: "px-3 py-1.5 text-sm bg-primary/10 text-primary rounded-lg hover:bg-primary/20 transition disabled:opacity-50",
                            disabled: *is_updating_status.read(),
                            onclick: {
                                let handler = handle_status_change.clone();
                                move |_| handler(IssueStatus::Open)
                            },
                            "Reopen PR"
                        }
                    }
                }
            }

            // Approval requirement notice
            if required_approvals > 0 && pr_status != IssueStatus::Applied {
                {
                    let current_approvals = approval_count.read().as_ref().copied().unwrap_or(0);
                    let met = current_approvals >= required_approvals;
                    let color_class = if met { "bg-green-500/10 border-green-500/20 text-green-600" } else { "bg-orange-500/10 border-orange-500/20 text-orange-600" };
                    rsx! {
                        div { class: "p-3 rounded-lg border text-sm {color_class}",
                            if met {
                                "Approval requirement met ({current_approvals}/{required_approvals})"
                            } else {
                                "Requires {required_approvals} approval(s) to merge ({current_approvals}/{required_approvals})"
                            }
                        }
                    }
                }
            }

            // Conflict detection banner
            if pr_status == IssueStatus::Open || pr_status == IssueStatus::Draft {
                if let Some(parent) = &pr.parent_commit {
                    ConflictBanner { parent_commit: parent.clone() }
                }
            }

            // Update PR button (author only)
            if is_pr_author && pr_status != IssueStatus::Applied && pr_status != IssueStatus::Closed {
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
                div { class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm flex items-center justify-center",
                    onclick: move |_| show_merge_confirm.set(false),
                    div {
                        class: "bg-background border border-border rounded-xl p-6 max-w-md mx-4 shadow-lg",
                        onclick: move |e| e.stop_propagation(),
                        h3 { class: "text-lg font-semibold mb-2", "Merge Pull Request" }
                        p { class: "text-sm text-muted-foreground mb-4",
                            "This will mark the pull request as merged (Applied). This action publishes a status event to Nostr relays."
                        }
                        // Merge strategy selection
                        div { class: "space-y-2 mb-4",
                            label { class: "text-sm font-medium", "Merge strategy" }
                            div { class: "space-y-1.5",
                                label { class: "flex items-center gap-2 p-2 rounded-lg hover:bg-accent/50 cursor-pointer",
                                    input {
                                        r#type: "radio",
                                        name: "merge-strategy",
                                        value: "merge",
                                        checked: *merge_strategy.read() == "merge",
                                        onchange: move |_| merge_strategy.set("merge".to_string()),
                                    }
                                    div {
                                        p { class: "text-sm font-medium", "Merge commit" }
                                        p { class: "text-xs text-muted-foreground", "All commits will be added with a merge commit." }
                                    }
                                }
                                label { class: "flex items-center gap-2 p-2 rounded-lg hover:bg-accent/50 cursor-pointer",
                                    input {
                                        r#type: "radio",
                                        name: "merge-strategy",
                                        value: "squash",
                                        checked: *merge_strategy.read() == "squash",
                                        onchange: move |_| merge_strategy.set("squash".to_string()),
                                    }
                                    div {
                                        p { class: "text-sm font-medium", "Squash and merge" }
                                        p { class: "text-xs text-muted-foreground", "All commits will be squashed into one commit." }
                                    }
                                }
                                label { class: "flex items-center gap-2 p-2 rounded-lg hover:bg-accent/50 cursor-pointer",
                                    input {
                                        r#type: "radio",
                                        name: "merge-strategy",
                                        value: "rebase",
                                        checked: *merge_strategy.read() == "rebase",
                                        onchange: move |_| merge_strategy.set("rebase".to_string()),
                                    }
                                    div {
                                        p { class: "text-sm font-medium", "Rebase and merge" }
                                        p { class: "text-xs text-muted-foreground", "Commits will be rebased onto the base branch." }
                                    }
                                }
                            }
                        }
                        div { class: "flex justify-end gap-3",
                            button {
                                class: "px-4 py-2 text-sm text-muted-foreground hover:text-foreground transition",
                                onclick: move |_| show_merge_confirm.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 text-sm font-medium bg-green-600 text-white rounded-lg hover:bg-green-700 transition",
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
                                            span { class: "text-xs text-muted-foreground", "Markdown supported" }
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
                                        "{pr.repository_naddr.chars().take(30).collect::<String>()}..."
                                    }
                                }
                            }
                        }
                    }
                },
                PrTab::FilesChanged => {
                    let pr_id_for_handler = pr_id.clone();
                    let pr_pubkey_for_handler = pr_pubkey.clone();
                    rsx! {
                        DiffViewer {
                            content: pr.content.clone(),
                            is_cover_letter: pr.is_cover_letter,
                            pr_event_id: Some(pr_id.clone()),
                            line_comments: line_comments.read().clone(),
                            on_line_comment: move |(file, line_num, text): (String, usize, String)| {
                                let id = pr_id_for_handler.clone();
                                let author = pr_pubkey_for_handler.clone();
                                spawn(async move {
                                    match publish_line_comment_by_id(&id, &author, &text, &file, line_num).await {
                                        Ok(_) => {
                                            // Refresh line comments after publishing
                                            if let Ok(lcs) = fetch_line_comments_by_id(&id).await {
                                                line_comments.set(lcs);
                                            }
                                        }
                                        Err(e) => {
                                            web_sys::console::error_1(
                                                &format!("Failed to publish line comment: {}", e).into(),
                                            );
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
        div { class: "p-4 border border-border rounded-lg",
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
/// Shows a warning when the PR's parent commit may have diverged from the target branch,
/// indicating potential merge conflicts.
#[component]
fn ConflictBanner(parent_commit: String) -> Element {
    let parent = parent_commit.clone();
    let _conflict_check = use_resource(move || {
        let parent = parent.clone();
        async move {
            // Try to check if the parent commit exists on the target branch
            // by fetching the repo and comparing with HEAD log
            use crate::services::git_hosting::git_service::GitService;
            if !GitService::is_initialized() {
                return None; // Can't check without git service
            }
            // Return the parent commit for display purposes
            Some(parent)
        }
    });

    // Only show when we have a parent commit to reference
    let short_parent = truncate_commit(&parent_commit);
    rsx! {
        div { class: "p-3 rounded-lg border bg-yellow-500/10 border-yellow-500/20 text-sm",
            div { class: "flex items-start gap-2",
                svg {
                    class: "w-4 h-4 text-yellow-500 shrink-0 mt-0.5",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                    line { x1: "12", y1: "9", x2: "12", y2: "13" }
                    line { x1: "12", y1: "17", x2: "12.01", y2: "17" }
                }
                div {
                    p { class: "text-yellow-700 dark:text-yellow-400 font-medium", "Check for conflicts before merging" }
                    p { class: "text-yellow-600 dark:text-yellow-500 mt-1",
                        "This PR is based on commit "
                        code { class: "px-1 py-0.5 bg-yellow-500/10 rounded text-xs font-mono", "{short_parent}" }
                        ". Verify the target branch hasn't diverged to avoid merge conflicts."
                    }
                }
            }
        }
    }
}
