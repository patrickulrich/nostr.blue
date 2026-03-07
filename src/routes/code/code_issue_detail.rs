//! Issue Detail Page
//!
//! View a single NIP-34 Git issue (Kind 1621) with comments,
//! bounty display, and permission-based status controls.
use crate::components::{icons, CodeStatusBadge};
use crate::routes::Route;
use crate::services::git_hosting::bounties::{
    claim_bounty, fetch_bounties_for_issue, release_bounty,
};
use crate::services::git_hosting::{
    fetch_comments_by_id, fetch_issue, fetch_repository, publish_comment_by_id,
    update_issue_status_by_id,
};
use crate::stores::profiles::PROFILE_CACHE;
use crate::stores::{auth_store, nostr_client};
use crate::utils::format_relative_time_or;
use crate::utils::nip34::{Bounty, GitComment, Issue, IssueStatus, Repository};
use crate::utils::permissions;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
/// Issue detail page component
#[component]
pub fn CodeIssueDetail(note_id: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut issue_result = use_signal(|| None::<Result<Issue, String>>);
    let mut loading = use_signal(|| true);
    let mut fetch_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&note_id, move |note_id| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let gen = fetch_gen.peek().wrapping_add(1);
        fetch_gen.set(gen);
        issue_result.set(None);
        loading.set(true);
        spawn(async move {
            let result = fetch_issue(&note_id).await;
            if *fetch_gen.peek() != gen {
                return;
            }
            issue_result.set(Some(result));
            loading.set(false);
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
                        "Issue"
                    }
                }
            }
            div { class: "p-4",
                if !*nostr_client::CLIENT_INITIALIZED.read()
                    || (*loading.read() && issue_result.read().is_none())
                {
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
    let repo_naddr_for_bounty = issue.repository_naddr.clone();
    let mut display_status = use_signal(|| issue.status);
    use_effect(use_reactive(&issue.status, move |status| {
        display_status.set(status);
    }));
    let author_profile = PROFILE_CACHE.read().peek(&issue.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| issue.pubkey_display());

    // Fetch repository for permission checks
    let mut repo = use_signal(|| None::<Repository>);
    let mut repo_gen = use_signal(|| 0u32);
    let repo_naddr = issue.repository_naddr.clone();
    use_effect(use_reactive(&repo_naddr, move |naddr| {
        repo.set(None); // clear stale data
        let gen = repo_gen.peek().wrapping_add(1);
        repo_gen.set(gen);
        if naddr.is_empty() {
            return;
        }
        spawn(async move {
            match fetch_repository(&naddr).await {
                Ok(r) => {
                    if *repo_gen.peek() == gen {
                        repo.set(Some(r));
                    }
                }
                Err(e) => {
                    log::warn!("Failed to fetch repository {}: {}", naddr, e);
                }
            }
        });
    }));

    // Fetch bounties for this issue
    let mut bounties = use_signal(Vec::<Bounty>::new);
    let mut bounties_gen = use_signal(|| 0u32);
    let mut bounties_error = use_signal(|| None::<String>);
    let mut bounties_loading = use_signal(|| false);
    let issue_id_for_bounties = issue_id.clone();
    use_effect(use_reactive(&issue_id_for_bounties, move |id| {
        bounties.set(Vec::new()); // clear stale data
        bounties_error.set(None);
        bounties_loading.set(true);
        let gen = bounties_gen.peek().wrapping_add(1);
        bounties_gen.set(gen);
        spawn(async move {
            match fetch_bounties_for_issue(&id).await {
                Ok(b) => {
                    if *bounties_gen.peek() == gen {
                        bounties.set(b);
                        bounties_loading.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch bounties: {e}");
                    if *bounties_gen.peek() == gen {
                        bounties_error.set(Some(format!("Failed to load bounties: {e}")));
                        bounties_loading.set(false);
                    }
                }
            }
        });
    }));

    // Permission checks: can_change_status includes author check; fall back to author-only when repo not loaded
    let can_update_status = is_authenticated
        && repo
            .read()
            .as_ref()
            .map(|r| permissions::can_change_status(&user_pubkey, r, &issue.pubkey))
            .unwrap_or(user_pubkey == issue.pubkey);

    let mut new_comment = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut comment_error = use_signal(|| None::<String>);
    let mut is_updating_status = use_signal(|| false);
    let mut status_error = use_signal(|| None::<String>);
    let mut claiming_bounty = use_signal(|| Option::<String>::None);
    let mut bounty_error = use_signal(|| Option::<String>::None);
    let issue_id_for_comments = issue_id.clone();
    let mut comments = use_resource(move || {
        let id = issue_id_for_comments.clone();
        async move { fetch_comments_by_id(&id).await }
    });
    let handle_status_change = {
        let issue_id = issue_id.clone();
        move |new_status: IssueStatus| {
            if *is_updating_status.read() {
                return;
            }
            is_updating_status.set(true);
            let id = issue_id.clone();
            spawn(async move {
                status_error.set(None);
                match update_issue_status_by_id(&id, new_status).await {
                    Ok(_) => {
                        display_status.set(new_status);
                    }
                    Err(e) => {
                        status_error.set(Some(format!("Failed to update status: {}", e)));
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
            if *is_submitting.read() {
                return;
            }
            let content = new_comment.read().clone();
            let id = issue_id.clone();
            let issue_author = issue_pubkey.clone();
            if content.trim().is_empty() {
                return;
            }
            is_submitting.set(true);
            spawn(async move {
                comment_error.set(None);
                match publish_comment_by_id(&id, &issue_author, &content).await {
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
            // Header section
            div { class: "space-y-4",
                div { class: "flex items-start justify-between gap-4",
                    h1 { class: "text-xl font-semibold",
                        if let Some(subject) = &issue.subject {
                            "{subject}"
                        } else {
                            "Issue #{issue.event_id.chars().take(8).collect::<String>()}"
                        }
                    }
                    CodeStatusBadge { status: display_status() }
                }
                div { class: "flex items-center gap-3",
                    Link {
                        to: Route::Profile {
                            pubkey: issue.pubkey.clone(),
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
                        {format_relative_time_or(issue.created_at, "Unknown")}
                    }
                }
            }

            // Status actions
            if can_update_status {
                if let Some(err) = status_error.read().as_ref() {
                    div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{err}"
                    }
                }
                div { class: "flex flex-wrap gap-2",
                    if display_status() != IssueStatus::Closed {
                        button {
                            r#type: "button",
                            class: "px-3 py-1.5 text-sm bg-destructive/10 text-destructive rounded-lg hover:bg-destructive/20 transition disabled:opacity-50",
                            disabled: *is_updating_status.read(),
                            onclick: {
                                let mut handler = handle_status_change.clone();
                                move |_| handler(IssueStatus::Closed)
                            },
                            "Close Issue"
                        }
                    }
                    if display_status() == IssueStatus::Closed {
                        button {
                            r#type: "button",
                            class: "px-3 py-1.5 text-sm bg-green-500/10 text-green-500 rounded-lg hover:bg-green-500/20 transition disabled:opacity-50",
                            disabled: *is_updating_status.read(),
                            onclick: {
                                let mut handler = handle_status_change.clone();
                                move |_| handler(IssueStatus::Open)
                            },
                            "Reopen Issue"
                        }
                    }
                }
            }

            // Two-column layout
            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Left column (2/3) - main content
                div { class: "lg:col-span-2 space-y-6",
                    // Issue content card
                    div { class: "p-4 border border-border rounded-lg bg-card",
                        div { class: "prose prose-sm max-w-none dark:prose-invert",
                            p { "{issue.content}" }
                        }
                    }

                    // Comments section
                    div { class: "space-y-4",
                        {
                            let displayed_comment_count = match comments.read().as_ref() {
                                Some(Ok(list)) => list.len(),
                                _ => issue.comment_count as usize,
                            };
                            rsx! {
                                h3 { class: "font-semibold flex items-center gap-2",
                                    "Comments"
                                    span { class: "px-1.5 py-0.5 text-xs rounded-full bg-muted", "{displayed_comment_count}" }
                                }
                            }
                        }
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
                }

                // Right column (1/3) - sidebar
                div { class: "space-y-4",
                    // Bounty error
                    if let Some(err) = bounties_error.read().as_ref() {
                        div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                            "{err}"
                        }
                    }

                    // Bounty loading skeleton
                    if *bounties_loading.read() && bounties.read().is_empty() {
                        div { class: "bg-card border border-border rounded-lg",
                            div { class: "px-4 py-3 border-b border-border bg-muted/30",
                                h3 { class: "font-semibold text-sm", "Bounties" }
                            }
                            div { class: "p-4 space-y-3 animate-pulse",
                                div { class: "h-4 bg-muted rounded w-2/3" }
                                div { class: "h-4 bg-muted rounded w-1/2" }
                            }
                        }
                    }

                    // Bounty section
                    if !bounties.read().is_empty() {
                        div { class: "bg-card border border-border rounded-lg",
                            div { class: "px-4 py-3 border-b border-border bg-muted/30",
                                h3 { class: "font-semibold text-sm", "Bounties" }
                            }
                            div { class: "p-4 space-y-3",
                                if let Some(error) = bounty_error.read().as_ref() {
                                    span { class: "text-xs text-destructive", "{error}" }
                                }
                                for bounty in bounties.read().iter() {
                                    {
                                        let bounty_eid = bounty.event_id.clone();
                                        let bounty_amount = bounty.amount_sats;
                                        let bounty_status = bounty.status;
                                        let issue_eid = issue_id.clone();
                                        let is_owner = is_authenticated && repo.read().as_ref().map(|r| permissions::is_owner(&user_pubkey, r)).unwrap_or(false);
                                        rsx! {
                                            div {
                                                key: "{bounty.event_id}",
                                                class: "space-y-2",
                                                div { class: "flex items-center justify-between text-sm",
                                                    span { class: "inline-flex items-center gap-1",
                                                        span { class: "text-yellow-400", "\u{26A1}" }
                                                        span { class: "font-mono", "{bounty_amount} sats" }
                                                    }
                                                    span {
                                                        class: match bounty_status {
                                                            crate::utils::nip34::BountyStatus::Pending => "text-xs px-2 py-0.5 rounded-full border border-yellow-500/30 bg-yellow-500/10 text-yellow-500",
                                                            crate::utils::nip34::BountyStatus::Claimed => "text-xs px-2 py-0.5 rounded-full border border-blue-500/30 bg-blue-500/10 text-blue-500",
                                                            crate::utils::nip34::BountyStatus::Paid => "text-xs px-2 py-0.5 rounded-full border border-green-500/30 bg-green-500/10 text-green-500",
                                                        },
                                                        "{bounty_status.label()}"
                                                    }
                                                }
                                                // Bounty actions
                                                if is_authenticated && bounty_status == crate::utils::nip34::BountyStatus::Pending {
                                                    button {
                                                        class: "w-full px-3 py-1.5 text-xs bg-blue-500/10 text-blue-500 border border-blue-500/20 rounded-lg hover:bg-blue-500/20 transition disabled:opacity-50",
                                                        disabled: claiming_bounty.read().is_some(),
                                                        onclick: {
                                                            let b_eid = bounty_eid.clone();
                                                            let i_eid = issue_eid.clone();
                                                            let b_amount = bounty_amount;
                                                            let repo_naddr = repo_naddr_for_bounty.clone();
                                                            move |_| {
                                                                if claiming_bounty.read().is_some() {
                                                                    return;
                                                                }
                                                                bounty_error.set(None);
                                                                claiming_bounty.set(Some(b_eid.clone()));
                                                                let b = b_eid.clone();
                                                                let i = i_eid.clone();
                                                                let repo_naddr = repo_naddr.clone();
                                                                spawn(async move {
                                                                    let b_id = match nostr_sdk::EventId::from_hex(&b) {
                                                                        Ok(id) => id,
                                                                        Err(e) => {
                                                                            bounty_error.set(Some(format!("Invalid bounty event ID: {}", e)));
                                                                            claiming_bounty.set(None);
                                                                            return;
                                                                        }
                                                                    };
                                                                    let i_id = match nostr_sdk::EventId::from_hex(&i) {
                                                                        Ok(id) => id,
                                                                        Err(e) => {
                                                                            bounty_error.set(Some(format!("Invalid issue event ID: {}", e)));
                                                                            claiming_bounty.set(None);
                                                                            return;
                                                                        }
                                                                    };
                                                                    let repo_coord = crate::utils::nip34::decode_naddr(&repo_naddr).ok();
                                                                    if let Err(e) = claim_bounty(b_id, i_id, b_amount, repo_coord.as_ref()).await {
                                                                        bounty_error.set(Some(format!("Failed to claim bounty: {}", e)));
                                                                    } else {
                                                                        bounty_error.set(None);
                                                                        let gen_before = *bounties_gen.peek();
                                                                        if let Ok(refreshed) = fetch_bounties_for_issue(&i).await {
                                                                            if *bounties_gen.peek() == gen_before {
                                                                                bounties.set(refreshed);
                                                                            }
                                                                        }
                                                                    }
                                                                    claiming_bounty.set(None);
                                                                });
                                                            }
                                                        },
                                                        "Claim Bounty"
                                                    }
                                                }
                                                if is_owner && bounty_status == crate::utils::nip34::BountyStatus::Claimed {
                                                    button {
                                                        class: "w-full px-3 py-1.5 text-xs bg-green-500/10 text-green-500 border border-green-500/20 rounded-lg hover:bg-green-500/20 transition disabled:opacity-50",
                                                        disabled: claiming_bounty.read().is_some(),
                                                        onclick: {
                                                            let b_eid = bounty_eid.clone();
                                                            let i_eid = issue_eid.clone();
                                                            let amount = bounty_amount;
                                                            let claimer = bounty.claimer.clone();
                                                            let repo_naddr = repo_naddr_for_bounty.clone();
                                                            move |_| {
                                                                if claiming_bounty.read().is_some() {
                                                                    return;
                                                                }
                                                                bounty_error.set(None);
                                                                claiming_bounty.set(Some(b_eid.clone()));
                                                                let b = b_eid.clone();
                                                                let i = i_eid.clone();
                                                                let c = claimer.clone();
                                                                let repo_naddr = repo_naddr.clone();
                                                                spawn(async move {
                                                                    let c = match c {
                                                                        Some(ref c) if !c.is_empty() => c.clone(),
                                                                        _ => {
                                                                            log::error!("Cannot release bounty: no claimer found");
                                                                            bounty_error.set(Some("Cannot release bounty: no claimer found".to_string()));
                                                                            claiming_bounty.set(None);
                                                                            return;
                                                                        }
                                                                    };
                                                                    let b_id = match nostr_sdk::EventId::from_hex(&b) {
                                                                        Ok(id) => id,
                                                                        Err(e) => {
                                                                            bounty_error.set(Some(format!("Invalid bounty event ID: {}", e)));
                                                                            claiming_bounty.set(None);
                                                                            return;
                                                                        }
                                                                    };
                                                                    let i_id = match nostr_sdk::EventId::from_hex(&i) {
                                                                        Ok(id) => id,
                                                                        Err(e) => {
                                                                            bounty_error.set(Some(format!("Invalid issue event ID: {}", e)));
                                                                            claiming_bounty.set(None);
                                                                            return;
                                                                        }
                                                                    };
                                                                    let repo_coord = crate::utils::nip34::decode_naddr(&repo_naddr).ok();
                                                                    match release_bounty(b_id, i_id, &c, amount, repo_coord.as_ref()).await {
                                                                        Ok(_) => {
                                                                            bounty_error.set(None);
                                                                            let gen_before = *bounties_gen.peek();
                                                                            if let Ok(refreshed) = fetch_bounties_for_issue(&i).await {
                                                                                if *bounties_gen.peek() == gen_before {
                                                                                    bounties.set(refreshed);
                                                                                }
                                                                            }
                                                                        }
                                                                        Err(e) => {
                                                                            bounty_error.set(Some(format!("Failed to release bounty: {}", e)));
                                                                        }
                                                                    }
                                                                    claiming_bounty.set(None);
                                                                });
                                                            }
                                                        },
                                                        "Release Payment"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Labels section
                    if !issue.labels.is_empty() {
                        div { class: "bg-card border border-border rounded-lg",
                            div { class: "px-4 py-3 border-b border-border bg-muted/30",
                                h3 { class: "font-semibold text-sm", "Labels" }
                            }
                            div { class: "p-4",
                                div { class: "flex flex-wrap gap-2",
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
                    }

                    // Repository link
                    if !issue.repository_naddr.is_empty() {
                        div { class: "bg-card border border-border rounded-lg p-3",
                            h4 { class: "text-xs font-medium text-muted-foreground mb-2", "Repository" }
                            Link {
                                to: Route::CodeRepo {
                                    naddr: issue.repository_naddr.clone(),
                                },
                                class: "text-sm text-primary hover:underline",
                                {repo.read().as_ref().map(|r| r.name.clone().unwrap_or_else(|| r.id.clone())).unwrap_or_else(|| "View Repository".to_string())}
                            }
                        }
                    }
                }
            }

            // Footer
            div { class: "pt-4 border-t border-border text-xs text-muted-foreground space-y-1",
                p { "Event ID: {issue.event_id}" }
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
            h3 { class: "font-semibold text-lg mb-2", "Issue Not Found" }
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
