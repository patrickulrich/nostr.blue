//! Bible Chapter Reading View
//! Displays a chapter with verse selection and highlighting
use crate::components::bible_commentary_panel::BibleCommentaryPanel;
use crate::components::bible_cross_ref_panel::BibleCrossRefPanel;
use crate::components::content_share_modal::{ContentShareModal, ContentType};
use crate::components::icons::SparklesIcon;
use crate::components::offline_download_indicator::DownloadProgressButton;
use crate::components::{ConfirmModal, HighlightModal};
use crate::routes::Route;
use crate::services::bible_api::format_selected_verses_reference;
use crate::services::bible_api::verse_to_plain_text;
use crate::stores::ai_chat_seed_store::{queue_ai_chat_seed, AiChatSeedPayload};
use crate::stores::bible_store::{self, ChapterContent, VerseContent};
use crate::stores::{ai_chat_store, auth_store};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::collections::HashMap;

/// Build a HashMap mapping verse numbers to plain text for efficient lookup
fn build_verse_text_map(content: &[ChapterContent]) -> HashMap<u32, String> {
    content
        .iter()
        .filter_map(|c| {
            if let ChapterContent::Verse {
                number,
                content: verse_content,
            } = c
            {
                Some((*number, verse_to_plain_text(verse_content)))
            } else {
                None
            }
        })
        .collect()
}

fn format_bible_context_for_ai(
    reference: &str,
    verses: &[u32],
    verse_text_map: &HashMap<u32, String>,
) -> String {
    let verse_lines = verses
        .iter()
        .filter_map(|verse| {
            verse_text_map
                .get(verse)
                .map(|text| format!("{verse} {text}"))
        })
        .collect::<Vec<_>>();

    if verse_lines.is_empty() {
        format!("Bible passage: {reference}")
    } else {
        format!("Bible passage: {reference}\n\n{}", verse_lines.join("\n"))
    }
}

/// Parse a Bible API link to extract translation, book, and chapter
/// Format: https://bible.helloao.org/api/{translation}/{book}/{chapter}.json
fn parse_chapter_api_link(api_link: &str) -> Option<(String, String, u32)> {
    let path = api_link.strip_suffix(".json")?;
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 3 {
        return None;
    }
    let chapter_str = parts[parts.len() - 1];
    let book = parts[parts.len() - 2];
    let translation = parts[parts.len() - 3];
    let chapter = chapter_str.parse::<u32>().ok()?;
    Some((
        urlencoding::decode(translation).ok()?.into_owned(),
        urlencoding::decode(book).ok()?.into_owned(),
        chapter,
    ))
}
/// Bible Chapter Reading View
#[component]
pub fn BibleChapter(translation: String, book: String, chapter: u32) -> Element {
    let navigator = navigator();
    let translation_for_copy = translation.clone();
    let translation_for_ai = translation.clone();
    let translation_for_highlight = translation.clone();
    let translation_for_display = translation.clone();
    let translation_for_verse_lookup = translation.clone();
    let translation_for_share = translation.clone();
    let book_for_display = book.clone();
    let book_for_highlight = book.clone();
    let book_for_verse_lookup = book.clone();
    let book_for_share = book.clone();
    let mut selected_verses = use_signal(Vec::<u32>::new);
    let mut show_toolbar = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);
    let mut share_title = use_signal(String::new);
    let mut share_url = use_signal(String::new);
    let mut share_content = use_signal(String::new);
    let mut highlight_feedback = use_signal(|| None::<(bool, String)>);
    let mut show_highlight_modal = use_signal(|| false);
    let mut pending_highlight_text = use_signal(String::new);
    let mut pending_highlight_reference = use_signal(String::new);
    let mut show_ai_chat_confirm = use_signal(|| false);
    let mut pending_ai_chat_seed = use_signal(|| None::<AiChatSeedPayload>);
    let is_authenticated = auth_store::is_authenticated();
    let toast = consume_toast();
    let mut active_tab = use_signal(|| "bible");
    let current_key = format!("{}/{}/{}", translation, book, chapter);
    let mut loaded_key = use_signal(String::new);
    let mut chapter_data: Signal<
        Option<Result<crate::services::bible_api::ChapterResponse, String>>,
    > = use_signal(|| None);
    if *loaded_key.peek() != current_key {
        loaded_key.set(current_key.clone());
        selected_verses.set(Vec::new());
        show_toolbar.set(false);
        chapter_data.set(None);
        let t = translation.clone();
        let b = book.clone();
        let c = chapter;
        let request_key = current_key.clone();
        spawn(async move {
            let result = bible_store::load_chapter(&t, &b, c).await;
            if *loaded_key.peek() != request_key {
                return;
            }
            let load_succeeded = result.is_ok();
            chapter_data.set(Some(result));
            if load_succeeded {
                spawn(async move {
                    let _ = bible_store::fetch_chapter_highlights(&t, &b, c).await;
                });
                if auth_store::is_authenticated() {
                    if let Ok(pubkey) = crate::stores::nostr_client::get_cached_pubkey() {
                        spawn(async move {
                            let _ = bible_store::fetch_user_highlights(&pubkey).await;
                        });
                    }
                }
            }
        });
    }
    let mut handle_verse_click = move |verse_num: u32| {
        let mut current = selected_verses.read().clone();
        if current.contains(&verse_num) {
            current.retain(|&v| v != verse_num);
        } else {
            current.push(verse_num);
            current.sort();
        }
        selected_verses.set(current.clone());
        show_toolbar.set(!current.is_empty());
    };
    let clear_selection = move |_| {
        selected_verses.set(Vec::new());
        show_toolbar.set(false);
    };
    let copy_verses = {
        let translation = translation_for_copy;
        let selected_verses_for_copy = selected_verses;
        let mut selected_verses_for_clear = selected_verses;
        let mut show_toolbar_for_clear = show_toolbar;
        move |_| {
            if let Some(Ok(ref data)) = *chapter_data.read() {
                let verses = selected_verses_for_copy.read().clone();
                if verses.is_empty() {
                    return;
                }
                let book_name = &data.book.common_name;
                let mut text_parts = Vec::new();
                let verse_text_map = build_verse_text_map(&data.chapter.content);
                for v in verses.iter() {
                    if let Some(text) = verse_text_map.get(v) {
                        text_parts.push(format!("{} {}", v, text));
                    }
                }
                let reference =
                    format_selected_verses_reference(book_name, chapter, &verses, &translation);
                let full_text = format!("{}\n\u{2014} {}", text_parts.join(" "), reference,);
                spawn(async move {
                    if let Err(e) = crate::platform::clipboard::copy_to_clipboard(&full_text).await
                    {
                        log::error!("Clipboard write failed: {:?}", e);
                        toast.error(
                            "Failed to copy to clipboard".to_string(),
                            ToastOptions::new(),
                        );
                        return;
                    }
                    selected_verses_for_clear.set(Vec::new());
                    show_toolbar_for_clear.set(false);
                });
            }
        }
    };
    let open_highlight_modal = {
        let translation = translation_for_highlight.clone();
        move |_| {
            if let Some(Ok(ref data)) = *chapter_data.read() {
                let verses = selected_verses.read().clone();
                if verses.is_empty() {
                    return;
                }
                let book_name = data.book.common_name.clone();
                let verse_text_map = build_verse_text_map(&data.chapter.content);
                let mut text_parts = Vec::new();
                for v in &verses {
                    if let Some(text) = verse_text_map.get(v) {
                        text_parts.push(text.clone());
                    }
                }
                let reference =
                    format_selected_verses_reference(&book_name, chapter, &verses, &translation);
                let verse_text = text_parts.join(" ");
                pending_highlight_text.set(verse_text);
                pending_highlight_reference.set(reference);
                show_highlight_modal.set(true);
            }
        }
    };
    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-10 border-b border-border",
                div { class: "max-w-3xl mx-auto p-4",
                    div { class: "flex items-center justify-between gap-4",
                        Link {
                            to: crate::routes::Route::BibleHome {},
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                class: "w-5 h-5",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M15 19l-7-7 7-7",
                                }
                            }
                        }
                        div { class: "text-center flex-1",
                            match &*chapter_data.read() {
                                Some(Ok(data)) => {
                                    let lang = data.translation.language_english_name.as_deref().unwrap_or("");
                                    let label = if lang == "English" || lang.is_empty() {
                                        data.translation.english_name.clone()
                                    } else {
                                        format!("{} ({})", data.translation.english_name, lang)
                                    };
                                    rsx! {
                                        h1 { class: "text-xl font-bold", "{data.book.common_name} {chapter}" }
                                        p { class: "text-sm text-muted-foreground", "{label}" }
                                    }
                                },
                                _ => rsx! {
                                    h1 { class: "text-xl font-bold", "{book_for_display} {chapter}" }
                                },
                            }
                        }
                        div { class: "flex gap-1 items-center",
                            DownloadProgressButton { translation_id: translation_for_display.clone() }
                            if let Some(Ok(data)) = &*chapter_data.read() {
                                if let Some(ref prev_link) = data.previous_chapter_api_link {
                                    if let Some((prev_trans, prev_book, prev_ch)) = parse_chapter_api_link(prev_link) {
                                        Link {
                                            to: crate::routes::Route::BibleChapter {
                                                translation: prev_trans,
                                                book: prev_book,
                                                chapter: prev_ch,
                                            },
                                            class: "p-2 hover:bg-muted rounded-lg transition",
                                            svg {
                                                xmlns: "http://www.w3.org/2000/svg",
                                                class: "w-5 h-5",
                                                fill: "none",
                                                view_box: "0 0 24 24",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M15 19l-7-7 7-7",
                                                }
                                            }
                                        }
                                    }
                                }
                                if let Some(ref next_link) = data.next_chapter_api_link {
                                    if let Some((next_trans, next_book, next_ch)) = parse_chapter_api_link(next_link) {
                                        Link {
                                            to: crate::routes::Route::BibleChapter {
                                                translation: next_trans,
                                                book: next_book,
                                                chapter: next_ch,
                                            },
                                            class: "p-2 hover:bg-muted rounded-lg transition",
                                            svg {
                                                xmlns: "http://www.w3.org/2000/svg",
                                                class: "w-5 h-5",
                                                fill: "none",
                                                view_box: "0 0 24 24",
                                                stroke: "currentColor",
                                                stroke_width: "2",
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
            div { class: "flex gap-1 border-b border-border px-4 max-w-3xl mx-auto",
                button {
                    class: if *active_tab.read() == "bible" { "px-3 py-2 text-sm font-medium border-b-2 border-primary text-primary" } else { "px-3 py-2 text-sm font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("bible"),
                    "Bible"
                }
                button {
                    class: if *active_tab.read() == "commentary" { "px-3 py-2 text-sm font-medium border-b-2 border-primary text-primary" } else { "px-3 py-2 text-sm font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("commentary"),
                    "Commentary"
                }
                button {
                    class: if *active_tab.read() == "xrefs" { "px-3 py-2 text-sm font-medium border-b-2 border-primary text-primary" } else { "px-3 py-2 text-sm font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("xrefs"),
                    "Cross-Refs"
                }
            }
            div { class: "flex-1 overflow-y-auto",
                div { class: "max-w-3xl mx-auto p-4 pb-32",
                    match &*chapter_data.read() {
                        None => rsx! {
                            div { class: "space-y-4 animate-pulse",
                                for i in 0..10 {
                                    div {
                                        key: "{i}",
                                        class: "h-4 bg-muted rounded",
                                        style: "width: {70 + (i % 3) * 10}%",
                                    }
                                }
                            }
                        },
                        Some(Err(err)) => rsx! {
                            div { class: "text-center py-16",
                                div { class: "text-destructive font-medium", "Error loading chapter" }
                                p { class: "text-sm text-muted-foreground mt-2", "{err}" }
                                button {
                                    class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                    onclick: move |_| {
                                        loaded_key.set(String::new());
                                    },
                                    "Try Again"
                                }
                            }
                        },
                        Some(Ok(data)) => {
                            let highlight_stats = bible_store::get_chapter_highlight_stats();
                            let is_rtl = data.translation.text_direction == "rtl";
                            let dir_class = if is_rtl { "text-right" } else { "" };
                            let current_tab = active_tab.read().clone();
                            let audio_links = data.this_chapter_audio_links.clone();
                            rsx! {
                                if current_tab == "bible" {
                                    div { dir: "{data.translation.text_direction}", class: "prose prose-lg dark:prose-invert max-w-none leading-relaxed {dir_class}",
                                    for content in data.chapter.content.iter() {
                                        match content {
                                            ChapterContent::Heading { content: heading_content } => {
                                                rsx! {
                                                    h3 { class: "text-lg font-semibold mt-6 mb-3 text-muted-foreground", {heading_content.join(" ")} }
                                                }
                                            }
                                            ChapterContent::LineBreak => {
                                                rsx! {
                                                    br {}
                                                }
                                            }
                                            ChapterContent::Verse { number, content: verse_content } => {
                                                let verse_num = *number;
                                                let is_selected = selected_verses.read().contains(&verse_num);
                                                let is_highlighted = bible_store::is_verse_highlighted(
                                                    &translation_for_verse_lookup,
                                                    &book_for_verse_lookup,
                                                    chapter,
                                                    verse_num,
                                                );
                                                let highlight_count = *highlight_stats
                                                    .verse_counts
                                                    .get(&verse_num)
                                                    .unwrap_or(&0);
                                                rsx! {
                                                    span {
                                                        key: "verse-{verse_num}",
                                                        tabindex: 0,
                                                        role: "button",
                                                        class: "cursor-pointer rounded px-0.5 transition-colors inline focus:outline-hidden focus:ring-2 focus:ring-primary",
                                                        class: if is_selected { "bg-primary/20 ring-2 ring-primary" } else if is_highlighted { "bg-yellow-100 dark:bg-yellow-900/30" } else { "" },
                                                        onclick: move |_| handle_verse_click(verse_num),
                                                        onkeydown: move |evt: KeyboardEvent| {
                                                            match evt.key() {
                                                                Key::Enter => {
                                                                    handle_verse_click(verse_num);
                                                                }
                                                                Key::Character(ref ch) if ch == " " => {
                                                                    evt.prevent_default();
                                                                    handle_verse_click(verse_num);
                                                                }
                                                                _ => {}
                                                            }
                                                        },
                                                        sup { class: "text-xs text-cyan-500 mr-1 select-none", "{verse_num}" }
                                                        for vc in verse_content.iter() {
                                                            {render_verse_content(vc)}
                                                        }
                                                        if highlight_count > 0 && !is_selected {
                                                            span {
                                                                class: "ml-1 text-xs text-muted-foreground opacity-60",
                                                                title: "{highlight_count} highlights",
                                                                "{highlight_count}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            ChapterContent::HebrewSubtitle { content: subtitle_content } => {
                                                rsx! {
                                                    div { class: "italic text-muted-foreground my-2",
                                                        for vc in subtitle_content.iter() {
                                                            if let Some(text) = vc.as_text() {
                                                                "{text} "
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if !data.chapter.footnotes.is_empty() {
                                    div { class: "mt-8 pt-4 border-t border-border",
                                        h4 { class: "font-medium mb-2", "Footnotes" }
                                        div { class: "space-y-1 text-sm text-muted-foreground",
                                            for footnote in data.chapter.footnotes.iter() {
                                                div { key: "fn-{footnote.note_id}",
                                                    span { class: "text-blue-500", "[{footnote.note_id}] " }
                                                    "{footnote.text}"
                                                }
                                            }
                                        }
                                    }
                                }
                                if let Some(ref links) = audio_links {
                                    if !links.is_empty() {
                                        div { class: "mt-6 pt-4 border-t border-border",
                                            p { class: "text-sm font-medium mb-2", "Audio" }
                                            div { class: "flex flex-wrap gap-2",
                                                for (reader, url) in links.iter() {
                                                    {
                                                        let url_clone = url.clone();
                                                        let reader_clone = reader.clone();
                                                        let trans_id = translation.clone();
                                                        let book_id = book.clone();
                                                        let book_name = data.book.common_name.clone();
                                                        let next_link = data.next_chapter_api_link.clone();
                                                        rsx! {
                                                            button {
                                                                key: "{reader}",
                                                                class: "px-3 py-1.5 text-sm rounded-lg bg-muted/50 hover:bg-muted transition flex items-center gap-1.5",
                                                                onclick: move |_| {
                                                                    play_bible_chapter_audio(
                                                                        &trans_id,
                                                                        &book_id,
                                                                        &book_name,
                                                                        chapter,
                                                                        &reader_clone,
                                                                        &url_clone,
                                                                    );
                                                                    if let Some(ref nl) = next_link {
                                                                        spawn_prefetch_bible_audio(nl.clone(), reader_clone.clone(), trans_id.clone(), 3);
                                                                    }
                                                                },
                                                                svg {
                                                                    class: "w-3.5 h-3.5",
                                                                    view_box: "0 0 24 24",
                                                                    fill: "currentColor",
                                                                    polygon { points: "8,5 19,12 8,19" }
                                                                }
                                                                "{reader}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                } else if current_tab == "commentary" {
                                    BibleCommentaryPanel {
                                        translation: translation.clone(),
                                        book: book.clone(),
                                        chapter: chapter,
                                    }
                                } else if current_tab == "xrefs" {
                                    BibleCrossRefPanel {
                                        translation: translation.clone(),
                                        book: book.clone(),
                                        chapter: chapter,
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if *show_toolbar.read() && !selected_verses.read().is_empty() {
                div { class: "fixed bottom-20 left-1/2 -translate-x-1/2 bg-card border border-border rounded-xl shadow-lg p-3 z-50",
                    div { class: "flex items-center gap-3",
                        span { class: "text-sm text-muted-foreground",
                            "{selected_verses.read().len()} selected"
                        }
                        div { class: "w-px h-6 bg-border" }
                        button {
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            title: "Copy verses",
                            onclick: copy_verses,
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                class: "w-5 h-5",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z",
                                }
                            }
                        }
                        button {
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            title: "Ask AI about these verses",
                            onclick: move |_| {
                                if let Some(Ok(ref data)) = *chapter_data.read() {
                                    let verses = selected_verses.read().clone();
                                    if verses.is_empty() {
                                        return;
                                    }

                                    let book_name = &data.book.common_name;
                                    let verse_text_map = build_verse_text_map(&data.chapter.content);
                                    let reference = format_selected_verses_reference(
                                        book_name,
                                        chapter,
                                        &verses,
                                        &translation_for_ai,
                                    );
                                    let message =
                                        format_bible_context_for_ai(&reference, &verses, &verse_text_map);
                                    let payload = AiChatSeedPayload {
                                        source: "bible".to_string(),
                                        title_hint: Some(reference),
                                        message,
                                    };
                                    let nav = navigator;
                                    spawn(async move {
                                        let account_key = ai_chat_store::current_account_key();
                                        match ai_chat_store::load_chat_state(&account_key).await {
                                            Ok(state)
                                                if ai_chat_store::has_saved_conversation_context(&state) =>
                                            {
                                                pending_ai_chat_seed.set(Some(payload));
                                                show_ai_chat_confirm.set(true);
                                            }
                                            Ok(_) => {
                                                queue_ai_chat_seed(payload);
                                                nav.push(Route::AIChat {});
                                            }
                                            Err(err) => {
                                                log::warn!(
                                                    "Failed to load AI chat state before Bible seed: {err}"
                                                );
                                                queue_ai_chat_seed(payload);
                                                nav.push(Route::AIChat {});
                                            }
                                        }
                                    });
                                }
                            },
                            SparklesIcon { class: "w-5 h-5".to_string() }
                        }
                        if is_authenticated {
                            button {
                                class: "p-2 hover:bg-muted rounded-lg transition",
                                title: "Highlight verses",
                                onclick: open_highlight_modal,
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-5 h-5 text-yellow-500",
                                    fill: "currentColor",
                                    view_box: "0 0 24 24",
                                    path { d: "M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" }
                                }
                            }
                            button {
                                class: "p-2 hover:bg-muted rounded-lg transition",
                                title: "Share verses",
                                onclick: move |_| {
                                    if let Some(Ok(ref data)) = *chapter_data.read() {
                                        let verses = selected_verses.read().clone();
                                        if verses.is_empty() {
                                            return;
                                        }
                                        let book_name = &data.book.common_name;
                                        let verse_text_map = build_verse_text_map(&data.chapter.content);
                                        let mut text_parts = Vec::new();
                                        for v in verses.iter() {
                                            if let Some(text) = verse_text_map.get(v) {
                                                text_parts.push(format!("{} {}", v, text));
                                            }
                                        }
                                        let verse_text = text_parts.join(" ");
                                        let reference = format_selected_verses_reference(
                                            book_name,
                                            chapter,
                                            &verses,
                                            &translation_for_share,
                                        );
                                        let url = format!(
                                            "https://nostr.blue/bible/{}/{}/{}",
                                            urlencoding::encode(&translation_for_share),
                                            urlencoding::encode(&book_for_share),
                                            chapter,
                                        );
                                        share_title.set(reference);
                                        share_url.set(url);
                                        share_content.set(verse_text);
                                        show_share_modal.set(true);
                                    }
                                },
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-5 h-5",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z",
                                    }
                                }
                            }
                        }
                        button {
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            title: "Clear selection",
                            onclick: clear_selection,
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                class: "w-5 h-5",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M6 18L18 6M6 6l12 12",
                                }
                            }
                        }
                    }
                }
            }
            if let Some((success, message)) = highlight_feedback.read().clone() {
                div {
                    class: "fixed bottom-36 left-1/2 -translate-x-1/2 z-50 px-4 py-2 rounded-lg shadow-lg text-sm font-medium",
                    class: if success { "bg-green-100 text-green-800 dark:bg-green-900/50 dark:text-green-200" } else { "bg-red-100 text-red-800 dark:bg-red-900/50 dark:text-red-200" },
                    "{message}"
                }
            }
            if *show_share_modal.read() {
                ContentShareModal {
                    title: share_title.read().clone(),
                    url: share_url.read().clone(),
                    content_type: ContentType::BibleVerse,
                    image_url: None,
                    content: Some(share_content.read().clone()),
                    on_close: move |_| show_share_modal.set(false),
                }
            }
            if *show_ai_chat_confirm.read() {
                ConfirmModal {
                    title: "Start a new AI chat?".to_string(),
                    message: "This opens AI Chat with the selected verses as context and switches away from your current saved conversation.".to_string(),
                    confirm_text: Some("Start new chat".to_string()),
                    cancel_text: Some("Cancel".to_string()),
                    on_confirm: move |_| {
                        if let Some(payload) = pending_ai_chat_seed.read().clone() {
                            queue_ai_chat_seed(payload);
                            navigator.push(Route::AIChat {});
                        }
                        pending_ai_chat_seed.set(None);
                        show_ai_chat_confirm.set(false);
                    },
                    on_cancel: move |_| {
                        pending_ai_chat_seed.set(None);
                        show_ai_chat_confirm.set(false);
                    },
                }
            }
            if *show_highlight_modal.read() {
                {
                    let translation = translation_for_highlight.clone();
                    let book = book_for_highlight.clone();
                    rsx! {
                        HighlightModal {
                            content: pending_highlight_text.read().clone(),
                            reference: pending_highlight_reference.read().clone(),
                            on_confirm: move |comment: Option<String>| {
                                let text = pending_highlight_text.read().clone();
                                let reference = pending_highlight_reference.read().clone();
                                let translation = translation.clone();
                                let book = book.clone();
                                spawn(async move {
                                    match bible_store::create_highlight(
                                            &text,
                                            &reference,
                                            &translation,
                                            &book,
                                            chapter,
                                            comment.as_deref(),
                                        )
                                        .await
                                    {
                                        Ok(_) => {
                                            log::info!("Highlight created");
                                            highlight_feedback.set(Some((true, "Highlight saved".to_string())));
                                            spawn(async move {
                                                crate::platform::timer::sleep_ms(2000).await;
                                                highlight_feedback.set(None);
                                            });
                                        }
                                        Err(e) => {
                                            log::error!("Failed to create highlight: {}", e);
                                            highlight_feedback.set(Some((false, format!("Failed: {}", e))));
                                            spawn(async move {
                                                crate::platform::timer::sleep_ms(4000).await;
                                                highlight_feedback.set(None);
                                            });
                                        }
                                    }
                                });
                                selected_verses.set(Vec::new());
                                show_toolbar.set(false);
                                show_highlight_modal.set(false);
                            },
                            on_cancel: move |_| {
                                show_highlight_modal.set(false);
                            },
                        }
                    }
                }
            }
        }
    }
}

use crate::stores::audio::music_player::{self, MusicTrack};
use crate::stores::audio::nostr_music::TrackSource;

fn build_bible_track(
    translation: &str,
    book_id: &str,
    book_name: &str,
    chapter: u32,
    reader: &str,
    url: &str,
) -> MusicTrack {
    MusicTrack {
        id: format!("bible-{}-{}-{}-{}", translation, book_id, chapter, reader),
        title: format!("{} {}", book_name, chapter),
        artist: reader.to_string(),
        album: None,
        media_url: url.to_string(),
        album_art_url: None,
        artist_art_url: None,
        duration: None,
        artist_id: None,
        album_id: None,
        artist_npub: None,
        source: TrackSource::Bible {
            translation: translation.to_string(),
            book: book_id.to_string(),
            chapter,
            reader: reader.to_string(),
        },
        msat_total: None,
        created_at: None,
        is_podcast: true,
        is_live_stream: false,
        value_block: None,
        chapters_url: None,
        transcripts: Vec::new(),
    }
}

fn play_bible_chapter_audio(
    translation: &str,
    book_id: &str,
    book_name: &str,
    chapter: u32,
    reader: &str,
    url: &str,
) {
    let track = build_bible_track(translation, book_id, book_name, chapter, reader, url);
    let mut state = music_player::MUSIC_PLAYER.write();
    state.stop_at_end = true;
    drop(state);
    music_player::play_or_toggle_track(track, None, None);
}

fn spawn_prefetch_bible_audio(
    next_api_link: String,
    reader: String,
    translation: String,
    depth: u32,
) {
    if depth == 0 {
        return;
    }
    spawn(async move {
        let client = reqwest::Client::new();
        let url = if next_api_link.starts_with("http") {
            next_api_link.clone()
        } else {
            format!("https://bible.helloao.org{}", next_api_link)
        };
        match client.get(&url).send().await {
            Ok(resp) => match resp.json::<crate::services::bible_api::ChapterResponse>().await {
                Ok(data) => {
                    if let Some(ref audio_links) = data.this_chapter_audio_links {
                        if let Some(mp3_url) = audio_links.get(&reader) {
                            let track = build_bible_track(
                                &translation,
                                &data.book.id,
                                &data.book.common_name,
                                data.chapter.number,
                                &reader,
                                mp3_url,
                            );
                            music_player::append_to_playlist(vec![track]);
                            if let Some(ref next_link) = data.next_chapter_api_link {
                                spawn_prefetch_bible_audio(
                                    next_link.clone(),
                                    reader.clone(),
                                    translation.clone(),
                                    depth - 1,
                                );
                            }
                        }
                    }
                }
                Err(e) => log::warn!("Failed to parse prefetched chapter: {}", e),
            },
            Err(e) => log::warn!("Failed to prefetch next chapter: {}", e),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::format_bible_context_for_ai;
    use std::collections::HashMap;

    #[test]
    fn formats_bible_context_for_ai_with_reference_and_numbered_verses() {
        let mut verse_text_map = HashMap::new();
        verse_text_map.insert(16, "For God so loved the world".to_string());
        verse_text_map.insert(17, "For God did not send His Son".to_string());

        let formatted =
            format_bible_context_for_ai("John 3:16-17 (BSB)", &[16, 17], &verse_text_map);

        assert_eq!(
            formatted,
            "Bible passage: John 3:16-17 (BSB)\n\n16 For God so loved the world\n17 For God did not send His Son"
        );
    }
}
/// Helper function to render verse content
fn render_verse_content(vc: &VerseContent) -> Element {
    match vc {
        VerseContent::Plain(text) => {
            rsx! { "{text} " }
        }
        VerseContent::Formatted(fmt) => {
            let is_jesus = fmt.words_of_jesus.unwrap_or(false);
            let poem_class = match fmt.poem {
                Some(1) => "ml-4 block",
                Some(2) => "ml-8 block",
                Some(_) => "ml-12 block",
                None => "",
            };
            rsx! {
                span {
                    class: if is_jesus { "text-red-600 dark:text-red-400" } else { "" },
                    class: "{poem_class}",
                    "{fmt.text} "
                }
            }
        }
        VerseContent::FootnoteRef(fref) => {
            rsx! {
                sup {
                    class: "text-xs text-blue-500 cursor-help",
                    title: "Footnote {fref.note_id}",
                    "[{fref.note_id}]"
                }
            }
        }
        VerseContent::InlineHeading(ih) => {
            rsx! {
                strong { class: "text-muted-foreground", "{ih.heading} " }
            }
        }
        VerseContent::InlineLineBreak(_) => {
            rsx! {
                br {}
            }
        }
        VerseContent::Unknown(_) => {
            rsx! {}
        }
    }
}
