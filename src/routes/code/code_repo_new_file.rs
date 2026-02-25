//! In-Browser File Creation
//!
//! Create new files directly in the browser and publish them as
//! Kind 1617 (GitPatch) events against a repository.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::fetch_repository;
use crate::stores::nostr_client::{get_client, HAS_SIGNER};
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::{decode_repo_naddr, Repository};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::{EventBuilder, Kind, Tag, TagKind};

/// Validate a file path for repository use.
/// Returns an error message if the path is invalid.
fn validate_file_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("File path is required".to_string());
    }
    if path.starts_with('/') {
        return Some("Path must not start with /".to_string());
    }
    if path.contains('\0') {
        return Some("Path must not contain null bytes".to_string());
    }
    if path.split('/').any(|segment| segment == ".." || segment == ".") {
        return Some("Path must not contain . or .. segments".to_string());
    }
    if path.contains('\n') || path.contains('\r') {
        return Some("Path must not contain newline characters".to_string());
    }
    if path.contains('\\') {
        return Some("Backslash path separators are not allowed".to_string());
    }
    for segment in path.split('/') {
        if segment.is_empty() {
            return Some("Path must not contain empty segments (double slashes)".to_string());
        }
    }
    None
}

/// Extract the filename from a file path (last segment).
fn filename_from_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Build a unified diff for a new file creation.
fn build_new_file_diff(path: &str, content: &str, author_pubkey: &str, commit_message: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let line_count = if content.is_empty() { 0 } else { lines.len() };

    let mut diff = String::new();
    diff.push_str(&format!("From: {}\n", author_pubkey));
    diff.push_str(&format!("Subject: {}\n", commit_message));
    diff.push_str("\n---\n");
    diff.push_str(&format!("diff --git a/{path} b/{path}\n"));
    diff.push_str("new file mode 100644\n");
    diff.push_str("--- /dev/null\n");
    diff.push_str(&format!("+++ b/{path}\n"));
    if line_count > 0 {
        diff.push_str(&format!("@@ -0,0 +1,{line_count} @@\n"));
        for (i, line) in lines.iter().enumerate() {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
            if i == line_count - 1 && !content.ends_with('\n') {
                diff.push_str("\\ No newline at end of file\n");
            }
        }
    } else {
        diff.push_str("@@ -0,0 +0,0 @@\n");
    }
    diff
}

/// Repository new file page component
#[component]
pub fn CodeRepoNewFile(naddr: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut file_path = use_signal(String::new);
    let mut file_content = use_signal(String::new);
    let mut commit_message = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut path_error = use_signal(|| None::<String>);
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut loading = use_signal(|| true);

    // Fetch repository data
    let mut repo_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&naddr, move |n| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let gen = repo_gen.peek().wrapping_add(1);
        repo_gen.set(gen);
        repo_result.set(None);
        spawn(async move {
            loading.set(true);
            let result = fetch_repository(&n).await;
            if *repo_gen.peek() == gen {
                repo_result.set(Some(result));
                loading.set(false);
            }
        });
    }));

    // Auth gate
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState { naddr: naddr.clone() }
        };
    }

    let repo_name = match &*repo_result.read() {
        Some(Ok(r)) => r.name.clone().unwrap_or_else(|| r.id.clone()),
        _ => "Repository".to_string(),
    };

    // Derive default commit message from file path
    let current_path = file_path.read().clone();
    let current_commit = commit_message.read().clone();
    let default_commit_msg = if current_path.is_empty() {
        "Add new file".to_string()
    } else {
        format!("Create {}", filename_from_path(&current_path))
    };

    // Handle form submission
    let naddr_for_submit = naddr.clone();
    let handle_submit = move |_| {
        if *submitting.peek() { return; }
        let path = file_path.read().clone();
        let content = file_content.read().clone();
        let msg = commit_message.read().clone();
        let naddr_inner = naddr_for_submit.clone();

        // Validate path
        if let Some(err) = validate_file_path(&path) {
            path_error.set(Some(err));
            return;
        }
        path_error.set(None);

        // Use custom message or default
        let final_msg = if msg.trim().is_empty() {
            if path.is_empty() {
                "Add new file".to_string()
            } else {
                format!("Create {}", filename_from_path(&path))
            }
        } else {
            msg
        };

        submitting.set(true);
        error_message.set(None);

        spawn(async move {
            match submit_new_file(&naddr_inner, &path, &content, &final_msg).await {
                Ok(()) => {
                    success.set(true);
                    submitting.set(false);
                }
                Err(e) => {
                    error_message.set(Some(e));
                    submitting.set(false);
                }
            }
        });
    };

    // Handle path input change with live validation
    let on_path_change = move |evt: Event<FormData>| {
        let val = evt.value();
        file_path.set(val.clone());
        if !val.is_empty() {
            path_error.set(validate_file_path(&val));
        } else {
            path_error.set(None);
        }
    };

    // Success state
    if *success.read() {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center p-4",
                div { class: "text-center max-w-md",
                    div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-green-500/10 flex items-center justify-center",
                        svg {
                            class: "w-10 h-10 text-green-400",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                            polyline { points: "22 4 12 14.01 9 11.01" }
                        }
                    }
                    h2 { class: "font-semibold text-xl mb-2", "File Created" }
                    p { class: "text-muted-foreground mb-6",
                        "Your new file patch has been published successfully."
                    }
                    Link {
                        to: Route::CodeRepo { naddr: naddr.clone() },
                        class: "inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                        "Back to Repository"
                    }
                }
            }
        };
    }

    let can_submit = !current_path.is_empty() && path_error.read().is_none() && !*submitting.read();

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeRepoTree {
                            naddr: naddr.clone(),
                            git_ref: "HEAD".to_string(),
                            path: vec![],
                        },
                        class: "text-muted-foreground hover:text-foreground",
                        aria_label: "Back to repository",
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
                            path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                            polyline { points: "14 2 14 8 20 8" }
                            line { x1: "12", y1: "18", x2: "12", y2: "12" }
                            line { x1: "9", y1: "15", x2: "15", y2: "15" }
                        }
                        "New File"
                    }
                }
            }

            div { class: "p-4 space-y-6 max-w-3xl mx-auto",
                // Repository context
                if *loading.read() {
                    div { class: "p-4 bg-muted rounded-lg animate-pulse",
                        div { class: "h-4 bg-muted-foreground/20 rounded w-48" }
                    }
                } else {
                    div { class: "p-4 bg-muted rounded-lg",
                        p { class: "text-sm text-muted-foreground",
                            "Creating new file in "
                            span { class: "font-medium text-foreground", "{repo_name}" }
                        }
                    }
                }

                // Error message
                if let Some(error) = error_message.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }

                // File path input
                div { class: "space-y-2",
                    label { class: "block text-sm font-medium", "File path" }
                    input {
                        class: {
                            let base = "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2";
                            if path_error.read().is_some() {
                                format!("{base} focus:ring-destructive border border-destructive/50")
                            } else {
                                format!("{base} focus:ring-primary")
                            }
                        },
                        r#type: "text",
                        placeholder: "path/to/file.rs",
                        value: "{current_path}",
                        oninput: on_path_change,
                    }
                    if let Some(err) = path_error.read().as_ref() {
                        p { class: "text-xs text-destructive", "{err}" }
                    } else {
                        p { class: "text-xs text-muted-foreground",
                            "Enter the path relative to the repository root"
                        }
                    }
                }

                // File content textarea
                div { class: "space-y-2",
                    label { class: "block text-sm font-medium", "Content" }
                    textarea {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary min-h-[400px] resize-y",
                        placeholder: "File content...",
                        value: "{file_content}",
                        oninput: move |evt: Event<FormData>| file_content.set(evt.value()),
                    }
                }

                // Commit message input
                div { class: "space-y-2",
                    label { class: "block text-sm font-medium", "Commit message" }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "{default_commit_msg}",
                        value: "{current_commit}",
                        oninput: move |evt: Event<FormData>| commit_message.set(evt.value()),
                    }
                    p { class: "text-xs text-muted-foreground",
                        "Leave blank to use the default message"
                    }
                }

                // Submit button
                div { class: "flex items-center gap-3",
                    button {
                        class: {
                            if can_submit {
                                "px-6 py-2.5 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition font-medium flex items-center gap-2"
                            } else {
                                "px-6 py-2.5 bg-primary/50 text-primary-foreground/50 rounded-lg font-medium flex items-center gap-2 cursor-not-allowed"
                            }
                        },
                        disabled: !can_submit,
                        onclick: handle_submit,
                        if *submitting.read() {
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
                            "Creating..."
                        } else {
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
                                line { x1: "12", y1: "18", x2: "12", y2: "12" }
                                line { x1: "9", y1: "15", x2: "15", y2: "15" }
                            }
                            "Create File"
                        }
                    }
                    Link {
                        to: Route::CodeRepoTree {
                            naddr: naddr.clone(),
                            git_ref: "HEAD".to_string(),
                            path: vec![],
                        },
                        class: "px-4 py-2.5 text-muted-foreground hover:text-foreground transition text-sm",
                        "Cancel"
                    }
                }

                // Info note about patches
                div { class: "p-4 bg-blue-500/10 rounded-lg border border-blue-500/20",
                    div { class: "flex items-start gap-3",
                        div { class: "w-8 h-8 rounded-lg bg-blue-500/20 flex items-center justify-center shrink-0",
                            svg {
                                class: "w-4 h-4 text-blue-500",
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
                                span { class: "font-medium", "Git Patch" }
                                span { class: "text-muted-foreground",
                                    " - This creates a new file patch (NIP-34 Kind 1617) that will be submitted to the repository maintainers for review."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Submit the new file as a Kind 1617 GitPatch event
async fn submit_new_file(
    naddr: &str,
    path: &str,
    content: &str,
    commit_message: &str,
) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Please sign in first.".to_string());
    }

    if let Some(err) = validate_file_path(path) {
        return Err(err);
    }

    if commit_message.contains('\n') || commit_message.contains('\r') {
        return Err("Commit message must not contain newline characters".to_string());
    }

    // Decode the repository coordinate
    let (coordinate, _relay_hints) =
        decode_repo_naddr(naddr).map_err(|e| format!("Invalid repository address: {}", e))?;

    // Get current user's public key for the From header
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let pubkey = signer
        .get_public_key()
        .await
        .map_err(|e| format!("Failed to get public key: {}", e))?;

    // Build the unified diff content
    let diff_content =
        build_new_file_diff(path, content, &pubkey.to_hex(), commit_message);

    // Build and publish the GitPatch event
    let builder = EventBuilder::new(Kind::GitPatch, &diff_content)
        .tag(Tag::coordinate(coordinate.clone(), None))
        .tag(Tag::public_key(coordinate.public_key))
        .tag(Tag::custom(
            TagKind::Subject,
            [commit_message],
        ))
        .tag(Tag::hashtag("new-file"));

    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish patch: {}", e))?;

    Ok(())
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
                    "Connect with your Nostr identity to create files."
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
