//! New Pull Request Page
//!
//! Create a new NIP-34 Git patch/PR (Kind 1617) for a repository.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, publish_patch_by_naddr};
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
/// Validate a git branch name per git-check-ref-format rules.
/// Rejects `HEAD`, leading `-`, and other invalid ref characters.
fn is_valid_git_refname(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name == "@" {
        return false;
    }
    if name == "HEAD" {
        return false;
    }
    if name.starts_with('-') || name.starts_with('.') || name.starts_with('/') {
        return false;
    }
    if name.ends_with(".lock") || name.ends_with('/') || name.ends_with('.') {
        return false;
    }
    if name.contains("@{")
        || name.contains("//")
        || name.contains("..")
        || name.contains('~')
        || name.contains('^')
        || name.contains(':')
        || name.contains('?')
        || name.contains('*')
        || name.contains('\\')
        || name.contains('[')
        || name.contains(' ')
    {
        return false;
    }
    if name.chars().any(|c| (c as u32) < 0x20 || c as u32 == 0x7f) {
        return false;
    }
    for component in name.split('/') {
        if component.is_empty() {
            return false;
        }
        if component.starts_with('.') {
            return false;
        }
        if component.ends_with(".lock") {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::is_valid_git_refname;

    #[test]
    fn is_valid_git_refname_accepts_valid_names() {
        let cases = [
            "feature/foo",
            "main",
            "v1.0.0",
            "bugfix/login-flow",
            "release/2026.03",
            "topic_123",
        ];

        for case in cases {
            assert!(
                is_valid_git_refname(case),
                "expected '{case}' to be accepted"
            );
        }
    }

    #[test]
    fn is_valid_git_refname_rejects_invalid_names() {
        let cases = [
            "",
            "@",
            "HEAD",
            "has space",
            "foo..bar",
            "foo~bar",
            "foo^bar",
            "foo:bar",
            "bad\u{0001}char",
            "bad\u{007f}char",
            "branch.lock",
            "-branch",
            ".hidden",
            "/start",
            "trailing/",
            "trailing.",
            "refs@{1}",
            "double//slash",
            "wild?card",
            "glob*name",
            r"back\slash",
            "bracket[name",
            "foo/.hidden",
            "foo/bar.lock",
            "topic.lock/x",
        ];

        for case in cases {
            assert!(
                !is_valid_git_refname(case),
                "expected '{case}' to be rejected"
            );
        }
    }
}

/// New pull request page component
#[component]
pub fn CodePullNew(naddr: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut content = use_signal(String::new);
    let mut commit = use_signal(String::new);
    let mut parent_commit = use_signal(String::new);
    let mut labels = use_signal(String::new);
    let mut closes_issues = use_signal(String::new);
    let mut branch_name = use_signal(String::new);
    let mut is_cover_letter = use_signal(|| true);
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let nav = use_navigator();
    let naddr_for_effect = naddr.clone();
    let mut repo_gen = use_signal(|| 0u32);
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let gen = repo_gen.peek().wrapping_add(1);
        repo_gen.set(gen);
        repo_result.set(None);
        spawn(async move {
            let result = fetch_repository(&n).await;
            if *repo_gen.peek() == gen {
                repo_result.set(Some(result));
            }
        });
    });
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState { naddr: naddr.clone() }
        };
    }
    let handle_submit = {
        let naddr = naddr.clone();
        move |_| {
            if *is_publishing.peek() {
                return;
            }
            let content_val = content.read().clone();
            let commit_val = commit.read().trim().to_string();
            let parent_val = parent_commit.read().trim().to_string();
            let labels_val = labels.read().clone();
            let closes_val = closes_issues.read().clone();
            let branch_val = branch_name.read().trim().to_string();
            let is_cover = *is_cover_letter.read();
            let naddr = naddr.clone();
            if content_val.trim().is_empty() {
                error_message.set(Some("Please enter patch content".to_string()));
                return;
            }
            if !branch_val.is_empty() && !is_valid_git_refname(&branch_val) {
                error_message.set(Some(format!(
                    "Invalid branch name '{}': contains invalid characters",
                    branch_val
                )));
                return;
            }
            let closes_list: Vec<String> = if closes_val.is_empty() {
                vec![]
            } else {
                closes_val
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            for id in &closes_list {
                if nostr_sdk::EventId::from_hex(id).is_err() {
                    error_message.set(Some(format!(
                        "Invalid event ID '{}': must be exactly 64 hex characters",
                        id
                    )));
                    return;
                }
            }
            is_publishing.set(true);
            error_message.set(None);
            spawn(async move {
                let label_list: Vec<&str> = if labels_val.is_empty() {
                    vec![]
                } else {
                    labels_val
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect()
                };
                let (commit_ref, parent_ref) = if is_cover {
                    (None, None)
                } else {
                    let cr = if commit_val.is_empty() {
                        None
                    } else if (commit_val.len() != 40 && commit_val.len() != 64)
                        || !commit_val.chars().all(|c| c.is_ascii_hexdigit())
                    {
                        error_message.set(Some(format!("Invalid commit hash '{}': must be exactly 40 (SHA-1) or 64 (SHA-256) hex characters", commit_val)));
                        is_publishing.set(false);
                        return;
                    } else {
                        Some(commit_val.as_str())
                    };
                    let pr = if parent_val.is_empty() {
                        None
                    } else if (parent_val.len() != 40 && parent_val.len() != 64)
                        || !parent_val.chars().all(|c| c.is_ascii_hexdigit())
                    {
                        error_message.set(Some(format!("Invalid parent commit hash '{}': must be exactly 40 (SHA-1) or 64 (SHA-256) hex characters", parent_val)));
                        is_publishing.set(false);
                        return;
                    } else {
                        Some(parent_val.as_str())
                    };
                    (cr, pr)
                };
                let branch_ref = if branch_val.is_empty() {
                    None
                } else {
                    Some(branch_val.as_str())
                };
                let closes_refs: Vec<&str> = closes_list.iter().map(|s| s.as_str()).collect();
                match publish_patch_by_naddr(
                    &naddr,
                    &content_val,
                    commit_ref,
                    parent_ref,
                    is_cover,
                    &label_list,
                    &closes_refs,
                    branch_ref,
                )
                .await
                {
                    Ok(event_id) => {
                        nav.push(Route::CodePullDetail { note_id: event_id });
                    }
                    Err(e) => {
                        error_message.set(Some(e));
                        is_publishing.set(false);
                    }
                }
            });
        }
    };
    let repo_name = match &*repo_result.read() {
        Some(Ok(r)) => r.name.clone().unwrap_or_else(|| r.id.clone()),
        _ => "Repository".to_string(),
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeRepo {
                                naddr: naddr.clone(),
                            },
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
                            "New Pull Request"
                        }
                    }
                    button {
                        class: "px-4 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                        disabled: *is_publishing.read(),
                        onclick: handle_submit,
                        if *is_publishing.read() {
                            svg {
                                class: "w-4 h-4 animate-spin",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4",
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                                }
                            }
                            "Publishing..."
                        } else {
                            "Submit PR"
                        }
                    }
                }
            }
            div { class: "p-4 space-y-6",
                if let Some(error) = error_message.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }
                div { class: "p-4 bg-muted rounded-lg",
                    p { class: "text-sm text-muted-foreground",
                        "Creating PR for "
                        span { class: "font-medium text-foreground", "{repo_name}" }
                    }
                }
                div { class: "p-4 bg-purple-500/10 rounded-lg border border-purple-500/20",
                    div { class: "flex items-start gap-3",
                        div { class: "w-8 h-8 rounded-lg bg-purple-500/20 flex items-center justify-center shrink-0",
                            svg {
                                class: "w-4 h-4 text-purple-500",
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
                        }
                        div {
                            p { class: "text-sm",
                                span { class: "font-medium", "NIP-34 Git Patch" }
                                span { class: "text-muted-foreground",
                                    " - Your pull request will be published as a Kind 1617 event on Nostr. Include your patch content or a cover letter describing your changes."
                                }
                            }
                        }
                    }
                }
                div {
                    label { class: "block text-sm font-medium mb-3", "Patch Type" }
                    div { class: "flex gap-3",
                        button {
                            class: if *is_cover_letter.read() { "flex-1 p-3 rounded-lg border-2 border-primary bg-primary/10 text-sm text-left" } else { "flex-1 p-3 rounded-lg border border-border bg-muted text-sm text-left hover:bg-accent transition" },
                            onclick: move |_| is_cover_letter.set(true),
                            div { class: "font-medium mb-1", "Cover Letter" }
                            div { class: "text-xs text-muted-foreground", "Describe your patch set" }
                        }
                        button {
                            class: if !*is_cover_letter.read() { "flex-1 p-3 rounded-lg border-2 border-primary bg-primary/10 text-sm text-left" } else { "flex-1 p-3 rounded-lg border border-border bg-muted text-sm text-left hover:bg-accent transition" },
                            onclick: move |_| is_cover_letter.set(false),
                            div { class: "font-medium mb-1", "Patch" }
                            div { class: "text-xs text-muted-foreground", "Single commit diff" }
                        }
                    }
                }
                if !*is_cover_letter.read() {
                    div { class: "grid grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Commit Hash "
                                span { class: "text-muted-foreground font-normal", "(optional)" }
                            }
                            input {
                                class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "Full git commit SHA (e.g., a1b2c3d4...)",
                                value: "{commit}",
                                oninput: move |e| commit.set(e.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Parent Commit "
                                span { class: "text-muted-foreground font-normal", "(optional)" }
                            }
                            input {
                                class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "Full git commit SHA (e.g., e5f6a7b8...)",
                                value: "{parent_commit}",
                                oninput: move |e| parent_commit.set(e.value()),
                            }
                        }
                    }
                }
                div {
                    label { class: "block text-sm font-medium mb-2",
                        if *is_cover_letter.read() {
                            "Cover Letter "
                        } else {
                            "Patch Content "
                        }
                        span { class: "text-destructive", "*" }
                    }
                    textarea {
                        class: "w-full h-64 px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary resize-y",
                        placeholder: if *is_cover_letter.read() { "Describe the changes in this patch set...\n\nFor example:\n- What problem does this solve?\n- What approach did you take?\n- Any relevant context or notes" } else { "Paste your git diff or patch content here...\n\ne.g., output from `git format-patch` or `git diff`" },
                        value: "{content}",
                        oninput: move |e| content.set(e.value()),
                    }
                    p { class: "text-xs text-muted-foreground mt-1",
                        if *is_cover_letter.read() {
                            "The first line will be used as the title"
                        } else {
                            "Paste the output of git format-patch or git diff"
                        }
                    }
                }
                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Labels "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "e.g., feature, refactor, bugfix",
                        value: "{labels}",
                        oninput: move |e| labels.set(e.value()),
                    }
                    p { class: "text-xs text-muted-foreground mt-1", "Comma-separated list of labels" }
                }
                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Branch Name "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "e.g., feature/my-branch",
                        value: "{branch_name}",
                        oninput: move |e| branch_name.set(e.value()),
                    }
                    p { class: "text-xs text-muted-foreground mt-1", "Source branch for this patch" }
                }
                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Closes Issues "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "64-char hex event ID (e.g., a1b2c3d4e5f6...)",
                        value: "{closes_issues}",
                        oninput: move |e| closes_issues.set(e.value()),
                    }
                    p { class: "text-xs text-muted-foreground mt-1", "Comma-separated 64-character hex event IDs of issues this PR closes" }
                }
            }
        }
    }
}
#[component]
fn NotAuthenticatedState(naddr: String) -> Element {
    rsx! {
        div { class: "min-h-screen flex items-center justify-center p-4",
            div { class: "text-center max-w-md",
                div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                }
                h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                p { class: "text-muted-foreground mb-6",
                    "Connect with your Nostr identity to create pull requests."
                }
                Link {
                    to: Route::CodeRepo { naddr },
                    class: "text-primary hover:underline",
                    "Back to Repository"
                }
            }
        }
    }
}
