//! Wiki New/Edit Route
//! Create or edit NIP-54 wiki pages (Kind 30818)
use crate::components::icons::{
    AlertTriangleIcon, ArrowLeftIcon, BookOpenIcon, BookmarkIcon, CheckIcon, Link2Icon,
    PenSquareIcon,
};
use crate::components::{
    AsciiDocContent, BookPickerModal, BookSelection, CitationPickerModal,
    CitationSelection, WikilinksList,
};
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, wiki_store};
use crate::utils::nip54::normalize_wiki_dtag;
use dioxus::events::{KeyboardData, MouseData};
use dioxus::prelude::*;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use web_sys::HtmlTextAreaElement;
/// Wiki page editor
#[component]
pub fn WikiNew() -> Element {
    let nav = use_navigator();
    let auth = auth_store::AUTH_STATE.read();
    let mut title = use_signal(String::new);
    let mut identifier = use_signal(String::new);
    let mut summary = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut auto_identifier = use_signal(|| true);
    let mut show_preview = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut show_citation_picker = use_signal(|| false);
    let mut show_book_picker = use_signal(|| false);
    #[allow(unused_mut)]
    let mut cursor_position = use_signal(|| 0usize);
    let mut inline_trigger_prefix = use_signal(String::new);
    if auth.pubkey.is_none() {
        return rsx! {
            div { class: "max-w-4xl mx-auto px-4 py-16 text-center",
                AlertTriangleIcon { class: "w-16 h-16 text-muted-foreground mx-auto mb-4" }
                h2 { class: "text-xl font-semibold text-foreground mb-2", "Login Required" }
                p { class: "text-muted-foreground mb-6",
                    "You need to be logged in to create or edit wiki pages."
                }
                Link {
                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                    to: Route::WikiHome {},
                    "Back to Wiki"
                }
            }
        };
    }
    use_effect(move || {
        if *auto_identifier.read() {
            let normalized = normalize_wiki_dtag(&title.read());
            identifier.set(normalized);
        }
    });
    let go_back = move |_| {
        nav.push(Route::WikiHome {});
    };
    let toggle_preview = move |_| {
        let current = *show_preview.read();
        show_preview.set(!current);
    };
    let handle_title_change = move |e: Event<FormData>| {
        title.set(e.value().clone());
    };
    let handle_identifier_change = move |e: Event<FormData>| {
        auto_identifier.set(false);
        identifier.set(e.value().clone());
    };
    let handle_summary_change = move |e: Event<FormData>| {
        summary.set(e.value().clone());
    };
    let handle_content_change = move |e: Event<FormData>| {
        let value = e.value().clone();
        content.set(value.clone());
        if value.ends_with("[[citation:") {
            inline_trigger_prefix.set("[[citation:".to_string());
            show_citation_picker.set(true);
        } else if value.ends_with("[[book:") {
            inline_trigger_prefix.set("[[book:".to_string());
            show_book_picker.set(true);
        }
    };
    let sync_cursor_click = move |_: Event<MouseData>| {
        #[cfg(feature = "web")]
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(elem) = document.get_element_by_id("wiki-content-editor") {
                    if let Some(textarea) = elem.dyn_ref::<HtmlTextAreaElement>() {
                        let pos = textarea.selection_start().ok().flatten().unwrap_or(0)
                            as usize;
                        cursor_position.set(pos);
                    }
                }
            }
        }
    };
    let sync_cursor_keyup = move |_: Event<KeyboardData>| {
        #[cfg(feature = "web")]
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(elem) = document.get_element_by_id("wiki-content-editor") {
                    if let Some(textarea) = elem.dyn_ref::<HtmlTextAreaElement>() {
                        let pos = textarea.selection_start().ok().flatten().unwrap_or(0)
                            as usize;
                        cursor_position.set(pos);
                    }
                }
            }
        }
    };
    let mut insert_at_cursor = move |text: &str| {
        let current = content.read().clone();
        let pos = *cursor_position.read();
        let prefix = inline_trigger_prefix.read().clone();
        let (adjusted_content, adjusted_pos) = if !prefix.is_empty()
            && current.ends_with(&prefix)
        {
            let new_content = current[..current.len() - prefix.len()].to_string();
            let new_pos = pos.saturating_sub(prefix.len());
            inline_trigger_prefix.set(String::new());
            (new_content, new_pos)
        } else {
            (current, pos)
        };
        let (before, after) = adjusted_content
            .split_at(adjusted_pos.min(adjusted_content.len()));
        let new_content = format!("{}{}{}", before, text, after);
        content.set(new_content);
    };
    let handle_citation_selected = move |selection: CitationSelection| {
        log::info!(
            "Inserting citation: {} (style: {:?})", selection.identifier, selection.style
        );
        insert_at_cursor(&selection.markup);
    };
    let handle_book_selected = move |selection: BookSelection| {
        log::info!("Inserting book reference: {}", selection.reference.display_text());
        insert_at_cursor(&selection.markup);
    };
    let handle_submit = move |_| {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            error.set(Some("Client not initialized yet".to_string()));
            return;
        }
        let title_val = title.read().clone();
        let identifier_val = identifier.read().clone();
        let summary_val = summary.read().clone();
        let content_val = content.read().clone();
        if title_val.trim().is_empty() {
            error.set(Some("Title is required".to_string()));
            return;
        }
        if identifier_val.trim().is_empty() {
            error.set(Some("Identifier is required".to_string()));
            return;
        }
        if content_val.trim().is_empty() {
            error.set(Some("Content is required".to_string()));
            return;
        }
        spawn(async move {
            saving.set(true);
            error.set(None);
            let summary_opt = if summary_val.trim().is_empty() {
                None
            } else {
                Some(summary_val)
            };
            match wiki_store::publish_wiki_page(
                    &title_val,
                    &content_val,
                    Some(&identifier_val),
                    summary_opt.as_deref(),
                )
                .await
            {
                Ok(naddr) => {
                    log::info!("Published wiki page: {}", naddr);
                    nav.push(Route::WikiDetail {
                        identifier: identifier_val,
                    });
                }
                Err(e) => {
                    error.set(Some(e));
                    saving.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "max-w-4xl mx-auto px-4 py-6",
            div { class: "flex items-center justify-between mb-6",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Back to Wiki"
                }
                div { class: "flex items-center gap-2",
                    button {
                        class: format!(
                            "flex items-center gap-2 px-3 py-1.5 text-sm rounded-md transition-colors {}",
                            if *show_preview.read() {
                                "bg-primary text-primary-foreground"
                            } else {
                                "bg-accent hover:bg-accent/80"
                            },
                        ),
                        onclick: toggle_preview,
                        if *show_preview.read() {
                            PenSquareIcon { class: "w-4 h-4" }
                            "Edit"
                        } else {
                            "Preview"
                        }
                    }
                    button {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50",
                        disabled: *saving.read(),
                        onclick: handle_submit,
                        CheckIcon { class: "w-5 h-5" }
                        if *saving.read() {
                            "Saving..."
                        } else {
                            "Publish"
                        }
                    }
                }
            }
            if let Some(ref e) = *error.read() {
                div { class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "{e}"
                }
            }
            if *show_preview.read() {
                div { class: "bg-card border border-border rounded-lg p-6",
                    h1 { class: "text-3xl font-bold text-foreground mb-2",
                        if title.read().is_empty() {
                            "Untitled Page"
                        } else {
                            "{title}"
                        }
                    }
                    if !summary.read().is_empty() {
                        p { class: "text-muted-foreground mb-4", "{summary}" }
                    }
                    div { class: "border-t border-border pt-4",
                        AsciiDocContent {
                            content: content.read().clone(),
                            enable_wikilinks: true,
                        }
                    }
                }
            } else {
                div { class: "space-y-6",
                    div {
                        label { class: "block text-sm font-medium text-foreground mb-2",
                            "Title"
                        }
                        input {
                            class: "w-full px-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                            r#type: "text",
                            placeholder: "Page title",
                            value: "{title}",
                            oninput: handle_title_change,
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-foreground mb-2",
                            "Identifier (d-tag)"
                        }
                        input {
                            class: "w-full px-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring font-mono text-sm",
                            r#type: "text",
                            placeholder: "page-identifier",
                            value: "{identifier}",
                            oninput: handle_identifier_change,
                        }
                        p { class: "text-xs text-muted-foreground mt-1",
                            "This is used in wikilinks: [[{identifier}]] or [[{identifier}|display text]]"
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-foreground mb-2",
                            "Summary (optional)"
                        }
                        input {
                            class: "w-full px-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                            r#type: "text",
                            placeholder: "Brief description of the page",
                            value: "{summary}",
                            oninput: handle_summary_change,
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-foreground mb-2",
                            "Content (AsciiDoc)"
                        }
                        div { class: "flex items-center gap-2 mb-2",
                            button {
                                class: "px-3 py-1.5 text-sm bg-accent hover:bg-accent/80 rounded-md flex items-center gap-2 transition-colors",
                                onclick: move |_| show_citation_picker.set(true),
                                r#type: "button",
                                BookmarkIcon { class: "w-4 h-4" }
                                "Insert Citation"
                            }
                            button {
                                class: "px-3 py-1.5 text-sm bg-accent hover:bg-accent/80 rounded-md flex items-center gap-2 transition-colors",
                                onclick: move |_| show_book_picker.set(true),
                                r#type: "button",
                                BookOpenIcon { class: "w-4 h-4" }
                                "Insert Book Reference"
                            }
                            span { class: "text-xs text-muted-foreground ml-2",
                                "or type [[citation: or [[book: to trigger"
                            }
                        }
                        div { class: "flex gap-4",
                            div { class: "flex-1",
                                textarea {
                                    id: "wiki-content-editor",
                                    class: "w-full h-96 px-4 py-3 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring font-mono text-sm resize-y",
                                    placeholder: "= Section Title\n\nWrite your content here using AsciiDoc markup.\n\nLink to other pages with [[wikilinks]].\n\nUse [[citation:...]] or [[book:...]] for references.",
                                    value: "{content}",
                                    oninput: handle_content_change,
                                    onclick: sync_cursor_click,
                                    onkeyup: sync_cursor_keyup,
                                }
                                div { class: "text-xs text-muted-foreground mt-2 space-y-1",
                                    p {
                                        "Use AsciiDoc markup: = heading, *bold*, _italic_, `code`, [[wikilink]]"
                                    }
                                    p { "Links: [[target]] or [[target|display text]]" }
                                }
                                if !content.read().is_empty() {
                                    div { class: "mt-3 pt-3 border-t border-border",
                                        div { class: "flex items-center gap-2 text-xs text-muted-foreground mb-2",
                                            Link2Icon { class: "w-3 h-3" }
                                            "Links in this page:"
                                        }
                                        WikilinksList {
                                            content: content.read().clone(),
                                            class: "".to_string(),
                                        }
                                    }
                                }
                            }
                            div { class: "hidden lg:block flex-1",
                                div { class: "h-96 overflow-y-auto border border-border rounded-lg p-4 bg-muted/20",
                                    div { class: "text-xs text-muted-foreground uppercase tracking-wider mb-2",
                                        "Live Preview"
                                    }
                                    if content.read().is_empty() {
                                        p { class: "text-muted-foreground italic",
                                            "Start typing to see a preview..."
                                        }
                                    } else {
                                        AsciiDocContent {
                                            content: content.read().clone(),
                                            enable_wikilinks: true,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            CitationPickerModal {
                show: show_citation_picker,
                on_select: handle_citation_selected,
            }
            BookPickerModal { show: show_book_picker, on_select: handle_book_selected }
        }
    }
}
