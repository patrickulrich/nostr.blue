//! Issue Detail Page
//!
//! View a single NIP-34 Git issue (Kind 1621) with comments.

use crate::components::{icons, CodeStatusBadge};
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_comments_by_id, fetch_issue, publish_comment_by_id, update_issue_status_by_id,
};
use crate::stores::profiles::PROFILE_CACHE;
use crate::stores::{auth_store, nostr_client};
use crate::utils::format_relative_time_or;
use crate::utils::nip34::{GitComment, Issue, IssueStatus};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

/// Issue detail page component
#[component]
pub fn CodeIssueDetail(note_id: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut issue_result = use_signal(|| None::<Result<Issue, String>>);
    let mut loading = use_signal(|| true);

    // Clone for effect and render
    let note_id_for_effect = note_id.clone();

    // Wait for client initialization before fetching
    use_effect(move || {
        let id = note_id_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        spawn(async move {
            loading.set(true);
            let result = fetch_issue(&id).await;
            issue_result.set(Some(result));
            loading.set(false);
        });
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeHome {},
                        class: "text-muted-foreground hover:text-foreground",
                        dangerous_inner_html: icons::ARROW_LEFT
                    }
                    h1 {
                        class: "text-xl font-bold flex items-center gap-2",
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
                            circle { cx: "12", cy: "12", r: "10" }
                            line { x1: "12", y1: "8", x2: "12", y2: "12" }
                            line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                        }
                        "Issue"
                    }
                }
            }

            // Content
            div {
                class: "p-4",
                if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && issue_result.read().is_none()) {
                    LoadingSkeleton {}
                } else {
                    match issue_result.read().as_ref() {
                        Some(Ok(issue)) => rsx! {
                            IssueContent {
                                issue: issue.clone(),
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
fn IssueContent(issue: Issue, is_authenticated: bool, user_pubkey: String) -> Element {
    let issue_id = issue.event_id.clone();
    let issue_pubkey = issue.pubkey.clone();
    let issue_status = issue.status;

    // Get author profile
    let author_profile = PROFILE_CACHE.read().peek(&issue.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| issue.pubkey_display());

    // Check if user can update status (is author or maintainer)
    let can_update_status = is_authenticated && user_pubkey == issue.pubkey;

    // Comment state
    let mut new_comment = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut comment_error = use_signal(|| None::<String>);

    // Status update state
    let mut is_updating_status = use_signal(|| false);

    // Comments resource
    let issue_id_for_comments = issue_id.clone();
    let comments = use_resource(move || {
        let id = issue_id_for_comments.clone();
        async move { fetch_comments_by_id(&id).await }
    });

    let handle_status_change = {
        let issue_id = issue_id.clone();
        move |new_status: IssueStatus| {
            let id = issue_id.clone();
            spawn(async move {
                is_updating_status.set(true);
                match update_issue_status_by_id(&id, new_status).await {
                    Ok(_) => {
                        // Refresh would happen via cache update
                    }
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

    let handle_submit_comment = {
        let issue_id = issue_id.clone();
        let issue_pubkey = issue_pubkey.clone();
        move |_| {
            let content = new_comment.read().clone();
            let id = issue_id.clone();
            let author = issue_pubkey.clone();

            if content.trim().is_empty() {
                return;
            }

            spawn(async move {
                is_submitting.set(true);
                comment_error.set(None);

                match publish_comment_by_id(&id, &author, &content).await {
                    Ok(_) => {
                        new_comment.set(String::new());
                        // Comments will refresh
                    }
                    Err(e) => {
                        comment_error.set(Some(e));
                    }
                }
                is_submitting.set(false);
            });
        }
    };

    rsx! {
        div {
            class: "space-y-6",

            // Issue header
            div {
                class: "space-y-4",

                // Title with status
                div {
                    class: "flex items-start justify-between gap-4",
                    h1 {
                        class: "text-xl font-semibold",
                        if let Some(subject) = &issue.subject {
                            "{subject}"
                        } else {
                            "Issue #{issue.event_id.chars().take(8).collect::<String>()}"
                        }
                    }
                    CodeStatusBadge { status: issue_status }
                }

                // Author and date
                div {
                    class: "flex items-center gap-3",
                    Link {
                        to: Route::Profile { pubkey: issue.pubkey.clone() },
                        class: "flex items-center gap-2 hover:underline",
                        div {
                            class: "w-6 h-6 rounded-full bg-muted flex items-center justify-center overflow-hidden",
                            if let Some(picture) = author_profile.as_ref().and_then(|p| p.picture.as_ref()) {
                                img {
                                    class: "w-full h-full object-cover",
                                    src: "{picture}",
                                    alt: "Author"
                                }
                            } else {
                                span { class: "text-xs", "{author_name.chars().next().unwrap_or('?')}" }
                            }
                        }
                        span { class: "text-sm font-medium", "{author_name}" }
                    }
                    span { class: "text-sm text-muted-foreground", "opened " {format_relative_time_or(issue.created_at, "Unknown")} }
                }

                // Labels
                if !issue.labels.is_empty() {
                    div {
                        class: "flex flex-wrap gap-2",
                        for label in issue.labels.iter() {
                            span {
                                key: "{label}",
                                class: "px-2 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-500 border border-blue-500/20",
                                "{label}"
                            }
                        }
                    }
                }
            }

            // Status update buttons
            if can_update_status {
                div {
                    class: "flex flex-wrap gap-2",
                    if issue_status != IssueStatus::Closed {
                        button {
                            class: "px-3 py-1.5 text-sm bg-destructive/10 text-destructive rounded-lg hover:bg-destructive/20 transition disabled:opacity-50",
                            disabled: *is_updating_status.read(),
                            onclick: {
                                let handler = handle_status_change.clone();
                                move |_| handler(IssueStatus::Closed)
                            },
                            "Close Issue"
                        }
                    }
                    if issue_status == IssueStatus::Closed {
                        button {
                            class: "px-3 py-1.5 text-sm bg-green-500/10 text-green-500 rounded-lg hover:bg-green-500/20 transition disabled:opacity-50",
                            disabled: *is_updating_status.read(),
                            onclick: {
                                let handler = handle_status_change.clone();
                                move |_| handler(IssueStatus::Open)
                            },
                            "Reopen Issue"
                        }
                    }
                }
            }

            // Issue content
            div {
                class: "p-4 border border-border rounded-lg bg-card",
                div {
                    class: "prose prose-sm max-w-none dark:prose-invert",
                    // Simple markdown-ish rendering
                    p { "{issue.content}" }
                }
            }

            // Comments section
            div {
                class: "space-y-4",
                h3 {
                    class: "font-semibold flex items-center gap-2",
                    "Comments"
                    span {
                        class: "px-1.5 py-0.5 text-xs rounded-full bg-muted",
                        "{issue.comment_count}"
                    }
                }

                // Comments list
                match &*comments.read() {
                    Some(Ok(comment_list)) => rsx! {
                        div {
                            class: "space-y-4",
                            for comment in comment_list.iter() {
                                CommentCard {
                                    key: "{comment.event_id}",
                                    comment: comment.clone()
                                }
                            }
                            if comment_list.is_empty() {
                                p {
                                    class: "text-sm text-muted-foreground text-center py-4",
                                    "No comments yet. Be the first to comment!"
                                }
                            }
                        }
                    },
                    Some(Err(e)) => rsx! {
                        p {
                            class: "text-sm text-destructive",
                            "Failed to load comments: {e}"
                        }
                    },
                    None => rsx! {
                        div {
                            class: "space-y-3",
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

                // New comment form
                if is_authenticated {
                    div {
                        class: "border border-border rounded-lg overflow-hidden",
                        textarea {
                            class: "w-full p-3 text-sm bg-background resize-none focus:outline-hidden",
                            placeholder: "Write a comment...",
                            rows: 3,
                            value: "{new_comment}",
                            oninput: move |e| new_comment.set(e.value())
                        }
                        div {
                            class: "px-3 py-2 bg-muted/50 border-t border-border flex items-center justify-between",
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
                    div {
                        class: "p-4 bg-muted rounded-lg text-center",
                        p {
                            class: "text-sm text-muted-foreground",
                            "Sign in to leave a comment"
                        }
                    }
                }
            }

            // Metadata
            div {
                class: "pt-4 border-t border-border text-xs text-muted-foreground space-y-1",
                p { "Event ID: {issue.event_id}" }
                if !issue.repository_naddr.is_empty() {
                    p {
                        "Repository: "
                        Link {
                            to: Route::CodeRepo { naddr: issue.repository_naddr.clone() },
                            class: "text-primary hover:underline",
                            "{issue.repository_naddr.chars().take(20).collect::<String>()}..."
                        }
                    }
                }
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
        div {
            class: "p-4 border border-border rounded-lg",

            // Comment header
            div {
                class: "flex items-center gap-2 mb-2",
                Link {
                    to: Route::Profile { pubkey: comment.pubkey.clone() },
                    class: "flex items-center gap-2 hover:underline",
                    div {
                        class: "w-5 h-5 rounded-full bg-muted flex items-center justify-center overflow-hidden",
                        if let Some(picture) = author_profile.as_ref().and_then(|p| p.picture.as_ref()) {
                            img {
                                class: "w-full h-full object-cover",
                                src: "{picture}",
                                alt: "Author"
                            }
                        } else {
                            span { class: "text-[10px]", "{author_name.chars().next().unwrap_or('?')}" }
                        }
                    }
                    span { class: "text-sm font-medium", "{author_name}" }
                }
                span { class: "text-xs text-muted-foreground", {format_relative_time_or(comment.created_at, "Unknown")} }
            }

            // Comment content
            div {
                class: "text-sm",
                "{comment.content}"
            }
        }
    }
}

#[component]
fn ErrorState(message: String) -> Element {
    rsx! {
        div {
            class: "text-center py-12",
            div {
                class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
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
                    line { x1: "12", y1: "8", x2: "12", y2: "12" }
                    line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "Issue Not Found" }
            p { class: "text-muted-foreground text-sm mb-4", "{message}" }
            Link {
                to: Route::CodeHome {},
                class: "text-primary hover:underline",
                "Back to Code"
            }
        }
    }
}

#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div {
            class: "space-y-6 animate-pulse",

            // Header skeleton
            div {
                class: "space-y-4",
                div { class: "h-6 bg-muted rounded w-2/3" }
                div {
                    class: "flex items-center gap-3",
                    div { class: "w-6 h-6 rounded-full bg-muted" }
                    div { class: "h-4 bg-muted rounded w-24" }
                    div { class: "h-4 bg-muted rounded w-20" }
                }
            }

            // Content skeleton
            div { class: "h-32 bg-muted rounded-lg" }

            // Comments skeleton
            div {
                class: "space-y-4",
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
