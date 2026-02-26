//! Discussion Detail Page
//!
//! View a single discussion with comments.
//! Follows patterns from code_issue_detail.rs.
use crate::components::{icons, ReactionButton};
use crate::hooks::use_reaction;
use crate::routes::Route;
use crate::services::git_hosting::discussions::{
    fetch_discussion, fetch_discussion_comments_by_id, publish_discussion_comment_by_id,
};
use crate::services::git_hosting::fetch_repository;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::profiles::PROFILE_CACHE;
use crate::stores::{auth_store, nostr_client};
use crate::utils::format_relative_time_or;
use crate::utils::nip34::{Discussion, GitComment, Repository};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
/// Discussion detail page component
#[component]
pub fn CodeDiscussionDetail(note_id: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut discussion_result = use_signal(|| None::<Result<Discussion, String>>);
    let mut loading = use_signal(|| true);
    let mut request_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&note_id, move |note_id| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let gen = request_gen.peek().wrapping_add(1);
        request_gen.set(gen);
        discussion_result.set(None);
        spawn(async move {
            loading.set(true);
            let result = fetch_discussion(&note_id).await;
            if *request_gen.peek() != gen { return; }
            discussion_result.set(Some(result));
            loading.set(false);
        });
    }));
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    button {
                        r#type: "button",
                        aria_label: "Go back",
                        class: "text-muted-foreground hover:text-foreground",
                        onclick: move |_| {
                            let nav = navigator();
                            nav.go_back();
                        },
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
                            path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                        }
                        "Discussion"
                    }
                }
            }
            div { class: "p-4",
                if !*nostr_client::CLIENT_INITIALIZED.read()
                    || (*loading.read() && discussion_result.read().is_none())
                {
                    LoadingSkeleton {}
                } else {
                    match discussion_result.read().as_ref() {
                        Some(Ok(discussion)) => rsx! {
                            DiscussionContent {
                                discussion: discussion.clone(),
                                is_authenticated: auth.is_authenticated,
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
fn DiscussionContent(discussion: Discussion, is_authenticated: bool) -> Element {
    let discussion_id = discussion.event_id.clone();
    let discussion_pubkey = discussion.pubkey.clone();
    let repo_naddr = discussion.repository_naddr.clone();
    let author_profile = PROFILE_CACHE.read().peek(&discussion.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| discussion.pubkey_display());
    // Fetch repository for role badges
    let mut repo_data = use_signal(|| None::<Repository>);
    let mut repo_gen = use_signal(|| 0u64);
    use_effect(use_reactive(&repo_naddr, move |naddr| {
        let gen = repo_gen.peek().wrapping_add(1);
        repo_gen.set(gen);
        repo_data.set(None);
        if naddr.is_empty() {
            return;
        }
        spawn(async move {
            if let Ok(repo) = fetch_repository(&naddr).await {
                if *repo_gen.peek() != gen { return; }
                repo_data.set(Some(repo));
            }
        });
    }));
    let has_signer = *HAS_SIGNER.read();
    let reaction = use_reaction(discussion.event_id.clone(), discussion.pubkey.clone(), None);
    let mut new_comment = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut comment_error = use_signal(|| None::<String>);
    let mut comments = use_resource(use_reactive(&discussion_id, move |id| {
        async move { fetch_discussion_comments_by_id(&id).await }
    }));
    let category_label = discussion.category.as_deref().map(|c| match c {
        "general" => "General",
        "ideas" => "Ideas",
        "q-a" => "Q&A",
        "show-and-tell" => "Show & Tell",
        other => other,
    });
    let handle_submit_comment = {
        let discussion_id = discussion_id.clone();
        let discussion_pubkey = discussion_pubkey.clone();
        move |_| {
            if *is_submitting.peek() { return; }
            let content = new_comment.read().clone();
            let id = discussion_id.clone();
            let author = discussion_pubkey.clone();
            if content.trim().is_empty() {
                return;
            }
            is_submitting.set(true);
            comment_error.set(None);
            spawn(async move {
                match publish_discussion_comment_by_id(&id, &author, &content).await {
                    Ok(_) => {
                        new_comment.set(String::new());
                        comments.restart();
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
        div { class: "space-y-6",
            div { class: "space-y-4",
                div { class: "flex items-start justify-between gap-4",
                    h1 { class: "text-xl font-semibold",
                        if let Some(subject) = &discussion.subject {
                            "{subject}"
                        } else {
                            "Discussion #{discussion.event_id.chars().take(8).collect::<String>()}"
                        }
                    }
                    if let Some(cat) = category_label {
                        span { class: "px-2 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-500 border border-blue-500/20 shrink-0",
                            "{cat}"
                        }
                    }
                }
                div { class: "flex items-center gap-3",
                    Link {
                        to: Route::Profile {
                            pubkey: discussion.pubkey.clone(),
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
                    if let Some(repo) = repo_data.read().as_ref() {
                        if let Some(role) = crate::utils::permissions::user_role_label(&discussion.pubkey, repo) {
                            span { class: if role == "Owner" { "px-1.5 py-0.5 text-xs rounded-full bg-purple-500/10 text-purple-500 border border-purple-500/20" } else { "px-1.5 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-500 border border-blue-500/20" },
                                "{role}"
                            }
                        }
                    }
                    span { class: "text-sm text-muted-foreground",
                        "started "
                        {format_relative_time_or(discussion.created_at, "Unknown")}
                    }
                }
            }
            div { class: "p-4 border border-border rounded-lg bg-card",
                div { class: "prose prose-sm max-w-none dark:prose-invert",
                    p { "{discussion.content}" }
                }
            }
            ReactionButton { reaction: reaction.clone(), has_signer }
            div { class: "space-y-4",
                h3 { class: "font-semibold flex items-center gap-2",
                    "Comments"
                    span { class: "px-1.5 py-0.5 text-xs rounded-full bg-muted",
                        {match &*comments.read() {
                            Some(Ok(list)) => list.len().to_string(),
                            _ => discussion.comment_count.to_string(),
                        }}
                    }
                }
                match &*comments.read() {
                    Some(Ok(comment_list)) => rsx! {
                        div { class: "space-y-4",
                            for comment in comment_list.iter() {
                                CommentCard { key: "{comment.event_id}", comment: comment.clone(), repo: repo_data.read().clone() }
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
                if is_authenticated {
                    div { class: "border border-border rounded-lg overflow-hidden",
                        textarea {
                            class: "w-full p-3 text-sm bg-background resize-none focus:outline-hidden",
                            placeholder: "Write a comment...",
                            rows: 3,
                            value: "{new_comment}",
                            oninput: move |e| new_comment.set(e.value()),
                        }
                        div { class: "px-3 py-2 bg-muted/50 border-t border-border flex items-center justify-between",
                            if let Some(error) = comment_error.read().as_ref() {
                                span { class: "text-xs text-destructive", "{error}" }
                            } else {
                                span { class: "text-xs text-muted-foreground", "Text formatting is not supported" }
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
            div { class: "pt-4 border-t border-border text-xs text-muted-foreground space-y-1",
                p { "Event ID: {discussion.event_id}" }
                if !discussion.repository_naddr.is_empty() {
                    p {
                        "Repository: "
                        Link {
                            to: Route::CodeRepo {
                                naddr: discussion.repository_naddr.clone(),
                            },
                            class: "text-primary hover:underline",
                            "{discussion.repository_naddr.chars().take(20).collect::<String>()}..."
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn CommentCard(comment: GitComment, #[props(default = None)] repo: Option<Repository>) -> Element {
    let has_signer = *HAS_SIGNER.read();
    let reaction = use_reaction(comment.event_id.clone(), comment.pubkey.clone(), None);
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
                if let Some(repo) = &repo {
                    if let Some(role) = crate::utils::permissions::user_role_label(&comment.pubkey, repo) {
                        span { class: if role == "Owner" { "px-1.5 py-0.5 text-xs rounded-full bg-purple-500/10 text-purple-500 border border-purple-500/20" } else { "px-1.5 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-500 border border-blue-500/20" },
                            "{role}"
                        }
                    }
                }
                span { class: "text-xs text-muted-foreground",
                    {format_relative_time_or(comment.created_at, "Unknown")}
                }
            }
            div { class: "text-sm", "{comment.content}" }
            ReactionButton { reaction, has_signer }
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
                    line { x1: "12", y1: "8", x2: "12", y2: "12" }
                    line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "Discussion Not Found" }
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
            div { class: "h-32 bg-muted rounded-lg" }
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
