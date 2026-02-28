//! Article Editor Route
//!
//! Full-featured article editor with NIP-37 draft support, auto-save,
//! image upload, publish confirmation, and formatting toolbar.
use dioxus::prelude::*;
use crate::components::icons::ArrowLeftIcon;
use crate::components::{
    ArticleCoverUploader, ImageInsertData, ImageUploadDialog, MarkdownEditor,
    MentionSelection, NostrMentionDialog, PublishConfig, PublishConfirmDialog,
};
use crate::hooks::{calculate_multi_hash, use_unsaved_changes};
use crate::stores::auth_store;
use crate::stores::draft_store::{
    delete_draft, load_drafts, save_draft, ArticleDraft, DraftStatus, LoadedDraft,
};
use crate::utils::format_time_ago;
/// Draft selection modal for when there are existing drafts
#[derive(Clone, Copy, PartialEq)]
enum EditorState {
    /// Loading drafts to check if any exist
    Loading,
    /// Showing draft selection
    SelectingDraft,
    /// Editing (new or loaded draft)
    Editing,
}
#[component]
pub fn ArticleNew() -> Element {
    let navigator = navigator();
    let mut editor_state = use_signal(|| EditorState::Loading);
    let mut available_drafts = use_signal(Vec::<LoadedDraft>::new);
    let mut title = use_signal(String::new);
    let mut summary = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut identifier = use_signal(String::new);
    let mut cover_image = use_signal(String::new);
    let mut hashtags = use_signal(String::new);
    let mut draft_status = use_signal(|| DraftStatus::Clean);
    let mut loaded_draft_id = use_signal(|| Option::<String>::None);
    let mut last_auto_save = use_signal(|| 0u64);
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut show_publish_dialog = use_signal(|| false);
    let mut show_mention_dialog = use_signal(|| false);
    let mut show_inline_image_upload = use_signal(|| false);
    let content_hash = use_memo(move || {
        calculate_multi_hash(
            &[
                &title.read(),
                &summary.read(),
                &content.read(),
                &identifier.read(),
                &cover_image.read(),
                &hashtags.read(),
            ],
        )
    });
    let unsaved = use_unsaved_changes(content_hash);
    let mut is_dirty = unsaved.is_dirty;
    let mut last_saved_hash = unsaved.last_saved_hash;
    use_effect(move || {
        let dirty = *is_dirty.read();
        let current_status = draft_status.read().clone();
        if dirty && !matches!(current_status, DraftStatus::Saving) {
            draft_status.set(DraftStatus::Dirty);
        }
    });
    let is_authenticated = use_memo(move || {
        auth_store::AUTH_STATE.read().is_authenticated
    });
    let title_chars = title.read().chars().count();
    let content_chars = content.read().chars().count();
    let can_publish = title_chars > 0 && content_chars > 0
        && !identifier.read().is_empty() && !*is_publishing.read();
    use_effect(move || {
        if *is_authenticated.read() {
            spawn(async move {
                match load_drafts().await {
                    Ok(drafts) => {
                        if drafts.is_empty() {
                            editor_state.set(EditorState::Editing);
                        } else {
                            available_drafts.set(drafts);
                            editor_state.set(EditorState::SelectingDraft);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to load drafts: {}", e);
                        editor_state.set(EditorState::Editing);
                    }
                }
            });
        }
    });
    use_effect(move || {
        if identifier.read().is_empty() && !title.read().is_empty() {
            let slug = title
                .read()
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .split('-')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            if !slug.is_empty() {
                identifier.set(slug);
            }
        }
    });
    use_effect(move || {
        if !*is_dirty.read() {
            return;
        }
        let title_val = title.read().clone();
        if title_val.is_empty() {
            return;
        }
        let now = crate::platform::timestamp::now_secs();
        let last_save = *last_auto_save.read();
        if now - last_save < 30 {
            return;
        }
        let draft = ArticleDraft {
            title: title.read().clone(),
            summary: summary.read().clone(),
            content: content.read().clone(),
            identifier: identifier.read().clone(),
            cover_image: cover_image.read().clone(),
            hashtags: hashtags
                .read()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            created_at: 0,
            updated_at: 0,
        };
        let saved_hash = *content_hash.read();
        last_auto_save.set(now);
        draft_status.set(DraftStatus::Saving);
        spawn(async move {
            match save_draft(&draft).await {
                Ok(event_id) => {
                    loaded_draft_id.set(Some(draft.identifier.clone()));
                    last_saved_hash.set(Some(saved_hash));
                    is_dirty.set(false);
                    draft_status
                        .set(DraftStatus::Saved {
                            event_id,
                            saved_at: now,
                        });
                    log::info!("Draft auto-saved");
                }
                Err(e) => {
                    log::error!("Failed to auto-save draft: {}", e);
                    draft_status.set(DraftStatus::Error(e));
                }
            }
        });
    });
    use_effect(move || {
        if !*is_authenticated.read() {
            navigator
                .push(crate::routes::Route::Home {
                    list: String::new(),
                });
        }
    });
    let handle_close = move |_| {
        navigator.go_back();
    };
    let handle_save_draft = move |_| {
        let draft = ArticleDraft {
            title: title.read().clone(),
            summary: summary.read().clone(),
            content: content.read().clone(),
            identifier: identifier.read().clone(),
            cover_image: cover_image.read().clone(),
            hashtags: hashtags
                .read()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            created_at: 0,
            updated_at: 0,
        };
        if draft.title.is_empty() || draft.identifier.is_empty() {
            error_message
                .set(
                    Some("Title and identifier are required to save draft".to_string()),
                );
            return;
        }
        let saved_hash = *content_hash.read();
        draft_status.set(DraftStatus::Saving);
        spawn(async move {
            let now = crate::platform::timestamp::now_secs();
            match save_draft(&draft).await {
                Ok(event_id) => {
                    last_auto_save.set(now);
                    loaded_draft_id.set(Some(draft.identifier.clone()));
                    last_saved_hash.set(Some(saved_hash));
                    is_dirty.set(false);
                    draft_status
                        .set(DraftStatus::Saved {
                            event_id,
                            saved_at: now,
                        });
                }
                Err(e) => {
                    log::error!("Failed to save draft: {}", e);
                    draft_status.set(DraftStatus::Error(e.clone()));
                    error_message.set(Some(format!("Failed to save draft: {}", e)));
                }
            }
        });
    };
    let handle_publish_confirm = move |config: PublishConfig| {
        let title_val = title.read().clone();
        let summary_val = summary.read().clone();
        let content_val = content.read().clone();
        let identifier_val = identifier.read().clone();
        let cover_image_val = cover_image.read().clone();
        let hashtags_val = hashtags.read().clone();
        let draft_id = loaded_draft_id.read().clone();
        is_publishing.set(true);
        error_message.set(None);
        show_publish_dialog.set(false);
        spawn(async move {
            let tags_vec: Vec<String> = hashtags_val
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            match crate::stores::nostr_client::publish_article(
                    title_val.clone(),
                    summary_val.clone(),
                    content_val.clone(),
                    identifier_val.clone(),
                    cover_image_val,
                    tags_vec,
                )
                .await
            {
                Ok(event_id) => {
                    log::info!("Article published successfully: {}", event_id);
                    if let Some(ref id) = draft_id {
                        if let Err(e) = delete_draft(id).await {
                            log::warn!("Failed to delete draft after publish: {}", e);
                        }
                    }
                    if config.promote_to_feed {
                        let naddr_str = {
                            use nostr::prelude::*;
                            match crate::stores::nostr_client::get_cached_pubkey() {
                                Ok(pubkey) => {
                                    let coord = Coordinate::new(Kind::LongFormTextNote, pubkey)
                                        .identifier(identifier_val.clone());
                                    let nip19_coord = Nip19Coordinate::new(coord, vec![]);
                                    nip19_coord.to_bech32().unwrap_or_else(|_| event_id.clone())
                                }
                                Err(_) => event_id.clone(),
                            }
                        };
                        let promo_content = format!(
                            "New article: {}\n\nnostr:{}",
                            title_val,
                            naddr_str,
                        );
                        if let Err(e) = crate::stores::nostr_client::publish_note(
                                promo_content,
                                vec![],
                            )
                            .await
                        {
                            log::warn!("Failed to create promotion note: {}", e);
                        }
                    }
                    is_publishing.set(false);
                    navigator.push(crate::routes::Route::Articles {});
                }
                Err(e) => {
                    log::error!("Failed to publish article: {}", e);
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };
    let mut handle_load_draft = move |draft: LoadedDraft| {
        let hashtags_str = draft.draft.hashtags.join(", ");
        let hash = calculate_multi_hash(
            &[
                &draft.draft.title,
                &draft.draft.summary,
                &draft.draft.content,
                &draft.draft.identifier,
                &draft.draft.cover_image,
                &hashtags_str,
            ],
        );
        title.set(draft.draft.title);
        summary.set(draft.draft.summary);
        content.set(draft.draft.content);
        identifier.set(draft.draft.identifier.clone());
        cover_image.set(draft.draft.cover_image);
        hashtags.set(hashtags_str);
        loaded_draft_id.set(Some(draft.draft.identifier));
        last_saved_hash.set(Some(hash));
        is_dirty.set(false);
        editor_state.set(EditorState::Editing);
    };
    let handle_new_article = move |_| {
        editor_state.set(EditorState::Editing);
    };
    let handle_mention_select = move |selection: MentionSelection| {
        crate::components::markdown_editor::insert_at_cursor(
            &mut content.clone(),
            "md-editor-article-content",
            &format!(" {} ", selection.uri),
        );
        show_mention_dialog.set(false);
    };
    let handle_inline_image_uploaded = move |data: ImageInsertData| {
        crate::components::markdown_editor::insert_at_cursor(
            &mut content.clone(),
            "md-editor-article-content",
            &data.to_markdown(),
        );
    };
    if !*is_authenticated.read() {
        return rsx! {
            div { class: "flex items-center justify-center h-screen", "Redirecting..." }
        };
    }
    if *editor_state.read() == EditorState::Loading {
        return rsx! {
            div { class: "flex items-center justify-center h-screen",
                div { class: "flex flex-col items-center gap-4",
                    svg {
                        class: "w-8 h-8 animate-spin text-primary",
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
                    p { class: "text-muted-foreground", "Loading drafts..." }
                }
            }
        };
    }
    if *editor_state.read() == EditorState::SelectingDraft {
        return rsx! {
            div { class: "min-h-screen bg-background",
                div { class: "max-w-2xl mx-auto px-4 py-12",
                    h1 { class: "text-2xl font-bold mb-8", "Continue editing or start fresh?" }
                    div { class: "space-y-4",
                        button {
                            class: "w-full p-4 border border-border rounded-lg hover:bg-accent transition text-left",
                            onclick: handle_new_article,
                            div { class: "flex items-center gap-3",
                                svg {
                                    class: "w-6 h-6 text-primary",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M12 4v16m8-8H4",
                                    }
                                }
                                div {
                                    p { class: "font-medium", "Start new article" }
                                    p { class: "text-sm text-muted-foreground",
                                        "Create a fresh article"
                                    }
                                }
                            }
                        }
                        div { class: "flex items-center gap-4 py-4",
                            div { class: "flex-1 border-t border-border" }
                            span { class: "text-sm text-muted-foreground", "or continue a draft" }
                            div { class: "flex-1 border-t border-border" }
                        }
                        for draft in available_drafts.read().iter() {
                            {
                                let draft_clone = draft.clone();
                                rsx! {
                                    button {
                                        class: "w-full p-4 border border-border rounded-lg hover:bg-accent transition text-left",
                                        onclick: move |_| handle_load_draft(draft_clone.clone()),
                                        div { class: "flex items-center justify-between",
                                            div {
                                                p { class: "font-medium truncate", "{draft.draft.title}" }
                                                p { class: "text-sm text-muted-foreground",
                                                    "Last edited: {format_time_ago(draft.draft.updated_at)}"
                                                }
                                            }
                                            svg {
                                                class: "w-5 h-5 text-muted-foreground",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M9 5l7 7-7 7",
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
        };
    }
    rsx! {
        div { class: "min-h-screen bg-background",
            div { class: "border-b border-border bg-background sticky top-0 z-10",
                div { class: "max-w-6xl mx-auto px-4 py-4 flex items-center justify-between",
                    div { class: "flex items-center gap-4",
                        button {
                            class: "text-muted-foreground hover:text-foreground transition",
                            onclick: handle_close,
                            ArrowLeftIcon { class: "w-6 h-6".to_string() }
                        }
                        div {
                            h1 { class: "text-xl font-bold", "Write Article" }
                            match &*draft_status.read() {
                                DraftStatus::Clean => rsx! {
                                    span { class: "text-xs text-muted-foreground", "" }
                                },
                                DraftStatus::Dirty => rsx! {
                                    span { class: "text-xs text-amber-500", "Unsaved changes" }
                                },
                                DraftStatus::Saving => rsx! {
                                    span { class: "text-xs text-blue-500 flex items-center gap-1",
                                        svg { class: "w-3 h-3 animate-spin", fill: "none", view_box: "0 0 24 24",
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
                                                d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z",
                                            }
                                        }
                                        "Saving..."
                                    }
                                },
                                DraftStatus::Saved { saved_at, .. } => rsx! {
                                    span { class: "text-xs text-green-500", "Saved {format_time_ago(*saved_at)}" }
                                },
                                DraftStatus::Error(e) => rsx! {
                                    span { class: "text-xs text-red-500", "Save failed: {e}" }
                                },
                            }
                        }
                    }
                    div { class: "flex items-center gap-3",
                        {
                            let is_saving = matches!(*draft_status.read(), DraftStatus::Saving);
                            let has_unsaved = matches!(*draft_status.read(), DraftStatus::Dirty);
                            let can_save = !title.read().is_empty() && !identifier.read().is_empty();
                            rsx! {
                                button {
                                    class: if has_unsaved && can_save { "px-4 py-2 text-sm font-medium rounded-lg bg-amber-500 hover:bg-amber-600 text-white transition flex items-center gap-2" } else if is_saving { "px-4 py-2 text-sm font-medium rounded-lg border border-border bg-muted text-muted-foreground cursor-wait flex items-center gap-2" } else { "px-4 py-2 text-sm font-medium rounded-lg border border-border hover:bg-accent transition disabled:opacity-50 disabled:cursor-not-allowed" },
                                    disabled: is_saving || !can_save,
                                    onclick: handle_save_draft,
                                    if is_saving {
                                        svg {
                                            class: "w-4 h-4 animate-spin",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
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
                                                d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z",
                                            }
                                        }
                                        "Saving..."
                                    } else if has_unsaved {
                                        "Save Draft"
                                    } else {
                                        "Save Draft"
                                    }
                                }
                            }
                        }
                        button {
                            class: if can_publish { "px-6 py-2 bg-primary hover:bg-primary/90 text-primary-foreground font-bold rounded-full transition" } else { "px-6 py-2 bg-muted text-muted-foreground font-bold rounded-full cursor-not-allowed" },
                            disabled: !can_publish,
                            onclick: move |_| show_publish_dialog.set(true),
                            if *is_publishing.read() {
                                "Publishing..."
                            } else {
                                "Publish"
                            }
                        }
                    }
                }
            }
            div { class: "max-w-6xl mx-auto px-4 py-8",
                if let Some(err) = error_message.read().as_ref() {
                    div { class: "mb-4 p-4 bg-destructive/10 border border-destructive/30 rounded-lg text-destructive",
                        "{err}"
                    }
                }
                div { class: "space-y-6",
                    div {
                        label { class: "block text-sm font-medium mb-2", "Title *" }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring text-2xl font-bold",
                            placeholder: "Enter article title",
                            value: "{title}",
                            oninput: move |e| title.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Identifier (URL slug) *" }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                            placeholder: "unique-article-identifier",
                            value: "{identifier}",
                            oninput: move |e| identifier.set(e.value()),
                        }
                        p { class: "mt-1 text-xs text-muted-foreground",
                            "Unique identifier for this article. Auto-generated from title."
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Summary (optional)" }
                        textarea {
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring resize-none",
                            rows: 3,
                            placeholder: "Brief description of your article",
                            value: "{summary}",
                            oninput: move |e| summary.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Cover Image" }
                        ArticleCoverUploader {
                            cover_url: cover_image,
                            label: "Upload Cover Image".to_string(),
                            show_url_input: true,
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Hashtags (optional)" }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                            placeholder: "nostr, bitcoin, technology (comma separated)",
                            value: "{hashtags}",
                            oninput: move |e| hashtags.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Content *" }
                        div { style: "height: 600px;",
                            MarkdownEditor {
                                content,
                                min_height: 600,
                                placeholder: "Write your article content here... Markdown is supported.".to_string(),
                                show_toolbar: true,
                                textarea_id: Some("md-editor-article-content".to_string()),
                                on_image_upload_request: Some(
                                    EventHandler::new(move |_| {
                                        show_inline_image_upload.set(true);
                                    }),
                                ),
                                on_mention_request: Some(
                                    EventHandler::new(move |_| {
                                        show_mention_dialog.set(true);
                                    }),
                                ),
                            }
                        }
                    }
                    div { class: "flex justify-between text-sm text-muted-foreground",
                        span { "Title: {title_chars} characters" }
                        span { "Content: {content_chars} characters" }
                    }
                }
            }
            PublishConfirmDialog {
                open: show_publish_dialog,
                title: title.read().clone(),
                summary: summary.read().clone(),
                content: content.read().clone(),
                cover_image: cover_image.read().clone(),
                on_confirm: handle_publish_confirm,
                on_cancel: EventHandler::new(move |_| {}),
                is_publishing: *is_publishing.read(),
            }
            NostrMentionDialog { open: show_mention_dialog, on_select: handle_mention_select }
            ImageUploadDialog {
                open: show_inline_image_upload,
                on_insert: handle_inline_image_uploaded,
            }
        }
    }
}
