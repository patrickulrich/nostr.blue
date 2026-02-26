//! In-Browser File Editing
//!
//! Edit existing files directly in the browser and publish changes as
//! Kind 1617 (GitPatch) events against a repository.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, git_service};
use crate::stores::nostr_client::{get_client, HAS_SIGNER};
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::{decode_repo_naddr, Repository};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::{EventBuilder, Kind, Tag, TagKind};

/// Build a unified diff for editing an existing file.
fn build_edit_file_diff(
    path: &str,
    old_content: &str,
    new_content: &str,
    author_pubkey: &str,
    commit_message: &str,
) -> String {
    let old_lines: Vec<&str> = if old_content.is_empty() {
        Vec::new()
    } else {
        old_content.lines().collect()
    };
    let new_lines: Vec<&str> = if new_content.is_empty() {
        Vec::new()
    } else {
        new_content.lines().collect()
    };

    let mut diff = String::new();
    diff.push_str(&format!("From: {}\n", author_pubkey));
    diff.push_str(&format!("Subject: {}\n", commit_message));
    diff.push_str("\n---\n");
    diff.push_str(&format!("diff --git a/{path} b/{path}\n"));
    diff.push_str(&format!("--- a/{path}\n"));
    diff.push_str(&format!("+++ b/{path}\n"));
    diff.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        1.min(old_lines.len()),
        old_lines.len(),
        1.min(new_lines.len()),
        new_lines.len()
    ));
    for line in &old_lines {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    if !old_content.is_empty() && !old_content.ends_with('\n') {
        diff.push_str("\\ No newline at end of file\n");
    }
    for line in &new_lines {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        diff.push_str("\\ No newline at end of file\n");
    }
    diff
}

/// File editing page component
#[component]
pub fn CodeRepoEditFile(naddr: String, git_ref: String, path: Vec<String>) -> Element {
    let file_path = path.join("/");
    let auth = auth_store::AUTH_STATE.read();
    let mut file_content = use_signal(String::new);
    let mut original_content = use_signal(String::new);
    let mut commit_message = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut file_load_failed = use_signal(|| false);
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut loading = use_signal(|| true);
    let mut file_loading = use_signal(|| true);

    // Fetch repository and file content
    let mut gen = use_signal(|| 0u32);
    {
        let naddr = naddr.clone();
        let git_ref = git_ref.clone();
        let file_path = file_path.clone();
        use_effect(use_reactive(
            (&naddr, &git_ref, &file_path),
            move |(naddr_val, git_ref_val, file_path_val)| {
                let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
                if !client_initialized {
                    return;
                }
                let current_gen = gen.peek().wrapping_add(1);
                gen.set(current_gen);
                success.set(false);
                file_content.set(String::new());
                original_content.set(String::new());
                commit_message.set(String::new());
                repo_result.set(None);
                error_message.set(None);
                file_load_failed.set(false);
                spawn(async move {
                    loading.set(true);
                    file_loading.set(true);
                    let result = fetch_repository(&naddr_val).await;
                    if *gen.peek() != current_gen {
                        return;
                    }
                    match result {
                        Ok(repo) => {
                            repo_result.set(Some(Ok(repo.clone())));
                            loading.set(false);
                            if !git_service::GitService::is_initialized() {
                                if let Err(e) = git_service::GitService::init().await {
                                    if *gen.peek() != current_gen {
                                        return;
                                    }
                                    error_message
                                        .set(Some(format!("Failed to initialize git: {}", e)));
                                    file_loading.set(false);
                                    return;
                                }
                            }
                            if *gen.peek() != current_gen {
                                return;
                            }
                            match git_service()
                                .read_file(&repo, &file_path_val, Some(&git_ref_val))
                                .await
                            {
                                Ok(content) => {
                                    if *gen.peek() != current_gen {
                                        return;
                                    }
                                    original_content.set(content.clone());
                                    file_content.set(content);
                                }
                                Err(e) => {
                                    if *gen.peek() != current_gen {
                                        return;
                                    }
                                    error_message
                                        .set(Some(format!("Failed to load file: {}", e)));
                                    file_load_failed.set(true);
                                }
                            }
                            file_loading.set(false);
                        }
                        Err(e) => {
                            if *gen.peek() != current_gen {
                                return;
                            }
                            error_message.set(Some(format!(
                                "Failed to load repository: {}",
                                e
                            )));
                            repo_result.set(Some(Err(e)));
                            loading.set(false);
                            file_loading.set(false);
                        }
                    }
                });
            },
        ));
    }

    // Auth gate
    if !auth.is_authenticated {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center p-4",
                div { class: "text-center max-w-md",
                    h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                    p { class: "text-muted-foreground mb-6",
                        "Connect with your Nostr identity to edit files."
                    }
                    Link {
                        to: Route::CodeRepo { naddr: naddr.clone() },
                        class: "text-primary hover:underline",
                        "Back to Repository"
                    }
                }
            }
        };
    }

    let repo_name = match &*repo_result.read() {
        Some(Ok(r)) => r.name.clone().unwrap_or_else(|| r.id.clone()),
        _ => "Repository".to_string(),
    };

    let filename = file_path.rsplit('/').next().unwrap_or(&file_path);
    let default_commit_msg = format!("Edit {}", filename);
    let current_commit = commit_message.read().clone();

    let naddr_for_submit = naddr.clone();
    let file_path_for_submit = file_path.clone();
    let handle_submit = move |_| {
        if *submitting.peek() {
            return;
        }
        if *file_load_failed.peek() {
            error_message.set(Some("Cannot submit: base file failed to load".to_string()));
            return;
        }
        let path_val = file_path_for_submit.clone();
        let content = file_content.read().clone();
        let old = original_content.read().clone();
        let msg = commit_message.read().clone();
        let naddr_inner = naddr_for_submit.clone();

        if content == old {
            error_message.set(Some("No changes made to the file".to_string()));
            return;
        }

        let final_msg = if msg.trim().is_empty() {
            format!("Edit {}", path_val.rsplit('/').next().unwrap_or(&path_val))
        } else {
            msg
        };

        submitting.set(true);
        error_message.set(None);

        spawn(async move {
            match submit_edit_file(&naddr_inner, &path_val, &old, &content, &final_msg).await {
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
                    h2 { class: "font-semibold text-xl mb-2", "Edit Submitted" }
                    p { class: "text-muted-foreground mb-6",
                        "Your file edit patch has been published successfully."
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

    let has_changes = *file_content.read() != *original_content.read();
    let can_submit = has_changes && !*submitting.read() && !*file_load_failed.read();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeRepoBlob {
                            naddr: naddr.clone(),
                            git_ref: git_ref.clone(),
                            path: path.clone(),
                        },
                        class: "text-muted-foreground hover:text-foreground",
                        aria_label: "Back to file",
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
                            path { d: "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" }
                            path { d: "M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" }
                        }
                        "Edit File"
                    }
                }
            }

            div { class: "p-4 space-y-6 max-w-3xl mx-auto",
                if *loading.read() {
                    div { class: "p-4 bg-muted rounded-lg animate-pulse",
                        div { class: "h-4 bg-muted-foreground/20 rounded w-48" }
                    }
                } else {
                    div { class: "p-4 bg-muted rounded-lg",
                        p { class: "text-sm text-muted-foreground",
                            "Editing "
                            span { class: "font-mono text-foreground", "{file_path}" }
                            " in "
                            span { class: "font-medium text-foreground", "{repo_name}" }
                        }
                    }
                }

                if let Some(error) = error_message.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }

                if *file_loading.read() {
                    div { class: "space-y-2",
                        label { class: "block text-sm font-medium", "Content" }
                        div { class: "w-full h-[400px] bg-muted rounded-lg animate-pulse" }
                    }
                } else {
                    div { class: "space-y-2",
                        label { class: "block text-sm font-medium", "Content" }
                        textarea {
                            class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary min-h-[400px] resize-y",
                            value: "{file_content}",
                            oninput: move |evt: Event<FormData>| file_content.set(evt.value()),
                        }
                    }
                }

                div { class: "space-y-2",
                    label { class: "block text-sm font-medium", "Commit message" }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "{default_commit_msg}",
                        value: "{current_commit}",
                        oninput: move |evt: Event<FormData>| commit_message.set(evt.value()),
                    }
                }

                div { class: "flex items-center gap-3",
                    button {
                        r#type: "button",
                        class: if can_submit {
                            "px-6 py-2.5 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition font-medium flex items-center gap-2"
                        } else {
                            "px-6 py-2.5 bg-primary/50 text-primary-foreground/50 rounded-lg font-medium flex items-center gap-2 cursor-not-allowed"
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
                            "Submitting..."
                        } else {
                            "Submit Edit"
                        }
                    }
                    Link {
                        to: Route::CodeRepoBlob {
                            naddr: naddr.clone(),
                            git_ref: git_ref.clone(),
                            path: path.clone(),
                        },
                        class: "px-4 py-2.5 text-muted-foreground hover:text-foreground transition text-sm",
                        "Cancel"
                    }
                }

                div { class: "p-4 bg-primary/10 rounded-lg border border-primary/20",
                    div { class: "flex items-start gap-3",
                        div { class: "w-8 h-8 rounded-lg bg-primary/20 flex items-center justify-center shrink-0",
                            svg {
                                class: "w-4 h-4 text-primary",
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
                                    " - This creates an edit patch (NIP-34 Kind 1617) that will be submitted to the repository maintainers for review."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Submit the file edit as a Kind 1617 GitPatch event
async fn submit_edit_file(
    naddr: &str,
    path: &str,
    old_content: &str,
    new_content: &str,
    commit_message: &str,
) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Please sign in first.".to_string());
    }

    // Validate path (interpolated into diff header)
    if path.contains('\n') || path.contains('\r') || path.contains('\0')
        || path.split('/').any(|s| s == ".." || s == "." || s.is_empty())
        || path.starts_with('/') || path.contains('\\')
    {
        return Err("Invalid file path".to_string());
    }

    // Validate commit message (interpolated into Subject header)
    if commit_message.contains('\n') || commit_message.contains('\r') {
        return Err("Commit message must not contain newline characters".to_string());
    }

    let (coordinate, _relay_hints) =
        decode_repo_naddr(naddr).map_err(|e| format!("Invalid repository address: {}", e))?;

    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let pubkey = signer
        .get_public_key()
        .await
        .map_err(|e| format!("Failed to get public key: {}", e))?;

    let diff_content =
        build_edit_file_diff(path, old_content, new_content, &pubkey.to_hex(), commit_message);

    let builder = EventBuilder::new(Kind::GitPatch, &diff_content)
        .tag(Tag::coordinate(coordinate.clone(), None))
        .tag(Tag::public_key(coordinate.public_key))
        .tag(Tag::custom(TagKind::Subject, [commit_message]))
        .tag(Tag::hashtag("edit-file"));

    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish patch: {}", e))?;

    Ok(())
}
