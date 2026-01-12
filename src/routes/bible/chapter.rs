//! Bible Chapter Reading View
//! Displays a chapter with verse selection and highlighting

use dioxus::prelude::*;

use crate::stores::bible_store::{
    self, ChapterContent, VerseContent,
    get_chapter_api_url,
};
use crate::services::bible_api::verse_to_plain_text;
use crate::stores::auth_store;
use crate::components::content_share_modal::{ContentShareModal, ContentType};

/// Bible Chapter Reading View
#[component]
pub fn BibleChapter(translation: String, book: String, chapter: u32) -> Element {
    // Clone params for closures that need them later
    let translation_for_copy = translation.clone();
    let translation_for_highlight = translation.clone();
    let book_for_highlight = book.clone();

    // State for verse selection
    let mut selected_verses = use_signal(Vec::<u32>::new);
    let mut show_toolbar = use_signal(|| false);

    // State for share modal
    let mut show_share_modal = use_signal(|| false);
    let mut share_title = use_signal(String::new);
    let mut share_url = use_signal(String::new);
    let mut share_content = use_signal(String::new);

    let is_authenticated = auth_store::is_authenticated();

    // Use a single key to track chapter identity
    let current_key = format!("{}/{}/{}", translation, book, chapter);
    let mut loaded_key = use_signal(String::new);

    // Chapter data signal
    let mut chapter_data: Signal<Option<Result<crate::services::bible_api::ChapterResponse, String>>> = use_signal(|| None);

    // Load chapter if key changed - do this in render, not in effect
    if *loaded_key.peek() != current_key {
        // Update loaded key immediately
        loaded_key.set(current_key.clone());

        // Clear selection
        selected_verses.set(Vec::new());
        show_toolbar.set(false);

        // Clear old data to show loading state
        chapter_data.set(None);

        // Clone for async
        let t = translation.clone();
        let b = book.clone();
        let c = chapter;

        // Spawn the async load
        spawn(async move {
            let result = bible_store::load_chapter(&t, &b, c).await;

            // Set chapter data immediately so content renders
            let load_succeeded = result.is_ok();
            chapter_data.set(Some(result));

            // Fetch highlights in background (don't block chapter render)
            if load_succeeded {
                let api_url = get_chapter_api_url(&t, &b, c);

                // Spawn highlight fetches separately so they don't block
                spawn(async move {
                    let _ = bible_store::fetch_chapter_highlights(&api_url).await;
                });

                // Fetch user's highlights if authenticated
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

    // Handle verse click
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

    // Clear selection
    let clear_selection = move |_| {
        selected_verses.set(Vec::new());
        show_toolbar.set(false);
    };

    // Copy selected verses (clipboard API only available on wasm32)
    #[allow(unused_variables, unused_mut)]
    let copy_verses = {
        let translation = translation_for_copy;
        let mut selected_verses = selected_verses;
        let mut show_toolbar = show_toolbar;
        move |_| {
            if let Some(Ok(ref data)) = *chapter_data.read() {
                let verses = selected_verses.read().clone();
                if verses.is_empty() {
                    return;
                }

                let book_name = &data.book.common_name;
                let mut text_parts = Vec::new();

                for v in verses.iter() {
                    for content in &data.chapter.content {
                        if let ChapterContent::Verse { number, content: verse_content } = content {
                            if number == v {
                                let text = verse_to_plain_text(verse_content);
                                text_parts.push(format!("{} {}", v, text));
                            }
                        }
                    }
                }

                // Copy to clipboard (wasm32 only)
                #[cfg(target_arch = "wasm32")]
                {
                    let first = *verses.first().unwrap_or(&0);
                    let last = *verses.last().unwrap_or(&0);
                    let reference = if first == last {
                        format!("{} {}:{} ({})", book_name, chapter, first, translation)
                    } else {
                        format!("{} {}:{}-{} ({})", book_name, chapter, first, last, translation)
                    };
                    let full_text = format!("{}\n\u{2014} {}", text_parts.join(" "), reference);

                    // Use spawn_local to properly await the clipboard Promise and handle errors
                    wasm_bindgen_futures::spawn_local(async move {
                        let window = match web_sys::window() {
                            Some(w) => w,
                            None => {
                                log::error!("Clipboard: No window object available");
                                return;
                            }
                        };
                        let clipboard = window.navigator().clipboard();
                        let promise = clipboard.write_text(&full_text);

                        if let Err(e) = wasm_bindgen_futures::JsFuture::from(promise).await {
                            log::error!("Clipboard write failed: {:?}", e);
                        }
                    });
                }

                // Clear selection after copy
                selected_verses.set(Vec::new());
                show_toolbar.set(false);
            }
        }
    };

    // Highlight selected verses
    let highlight_verses = {
        let translation = translation_for_highlight;
        let book = book_for_highlight;
        let mut selected_verses = selected_verses;
        let mut show_toolbar = show_toolbar;
        move |_| {
            if let Some(Ok(ref data)) = *chapter_data.read() {
                let verses = selected_verses.read().clone();
                if verses.is_empty() {
                    return;
                }

                let book_name = data.book.common_name.clone();
                let api_url = get_chapter_api_url(&translation, &book, chapter);

                // Build verse text
                let mut text_parts = Vec::new();
                for v in &verses {
                    for content in &data.chapter.content {
                        if let ChapterContent::Verse { number, content: verse_content } = content {
                            if number == v {
                                text_parts.push(verse_to_plain_text(verse_content));
                            }
                        }
                    }
                }

                let first = *verses.first().unwrap_or(&0);
                let last = *verses.last().unwrap_or(&0);
                let reference = if first == last {
                    format!("{} {}:{} ({})", book_name, chapter, first, translation)
                } else {
                    format!("{} {}:{}-{} ({})", book_name, chapter, first, last, translation)
                };

                let verse_text = text_parts.join(" ");

                spawn(async move {
                    match bible_store::create_highlight(&verse_text, &reference, &api_url, None).await {
                        Ok(_) => log::info!("Highlight created"),
                        Err(e) => log::error!("Failed to create highlight: {}", e),
                    }
                });

                // Clear selection
                selected_verses.set(Vec::new());
                show_toolbar.set(false);
            }
        }
    };

    rsx! {
        div { class: "flex flex-col h-full",

            // Sticky Header with Navigation
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-10 border-b border-border",
                div { class: "max-w-3xl mx-auto p-4",
                    div { class: "flex items-center justify-between gap-4",
                        // Back button
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
                                    d: "M15 19l-7-7 7-7"
                                }
                            }
                        }

                        // Chapter title
                        div { class: "text-center flex-1",
                            match &*chapter_data.read() {
                                Some(Ok(data)) => rsx! {
                                    h1 { class: "text-xl font-bold",
                                        "{data.book.common_name} {chapter}"
                                    }
                                    p { class: "text-sm text-muted-foreground",
                                        "{translation}"
                                    }
                                },
                                _ => rsx! {
                                    h1 { class: "text-xl font-bold", "{book} {chapter}" }
                                }
                            }
                        }

                        // Nav buttons
                        div { class: "flex gap-2",
                            if let Some(Ok(data)) = &*chapter_data.read() {
                                // Previous chapter
                                if data.previous_chapter_api_link.is_some() && chapter > 1 {
                                    Link {
                                        to: crate::routes::Route::BibleChapter {
                                            translation: translation.clone(),
                                            book: book.clone(),
                                            chapter: chapter - 1,
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
                                                d: "M15 19l-7-7 7-7"
                                            }
                                        }
                                    }
                                }

                                // Next chapter
                                if data.next_chapter_api_link.is_some() {
                                    Link {
                                        to: crate::routes::Route::BibleChapter {
                                            translation: translation.clone(),
                                            book: book.clone(),
                                            chapter: chapter + 1,
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
                                                d: "M9 5l7 7-7 7"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Chapter Content
            div { class: "flex-1 overflow-y-auto",
                div { class: "max-w-3xl mx-auto p-4 pb-32",

                    match &*chapter_data.read() {
                        None => rsx! {
                            // Loading skeleton
                            div { class: "space-y-4 animate-pulse",
                                for i in 0..10 {
                                    div {
                                        key: "{i}",
                                        class: "h-4 bg-muted rounded",
                                        style: "width: {70 + (i % 3) * 10}%"
                                    }
                                }
                            }
                        },
                        Some(Err(err)) => rsx! {
                            // Error state
                            div { class: "text-center py-16",
                                div { class: "text-destructive font-medium", "Error loading chapter" }
                                p { class: "text-sm text-muted-foreground mt-2", "{err}" }
                                button {
                                    class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                    onclick: move |_| {
                                        // Force reload by clearing loaded_key
                                        loaded_key.set(String::new());
                                    },
                                    "Try Again"
                                }
                            }
                        },
                        Some(Ok(data)) => rsx! {
                            // Render chapter content
                            div { class: "prose prose-lg dark:prose-invert max-w-none leading-relaxed",
                                for content in data.chapter.content.iter() {
                                    match content {
                                        ChapterContent::Heading { content: heading_content } => {
                                            rsx! {
                                                h3 { class: "text-lg font-semibold mt-6 mb-3 text-muted-foreground",
                                                    {heading_content.join(" ")}
                                                }
                                            }
                                        }
                                        ChapterContent::LineBreak => {
                                            rsx! { br {} }
                                        }
                                        ChapterContent::Verse { number, content: verse_content } => {
                                            let verse_num = *number;
                                            let is_selected = selected_verses.read().contains(&verse_num);
                                            let is_highlighted = bible_store::is_verse_highlighted(&translation, &book, chapter, verse_num);
                                            let highlight_count = bible_store::get_verse_highlight_count(verse_num);

                                            rsx! {
                                                span {
                                                    key: "verse-{verse_num}",
                                                    // Keyboard accessibility
                                                    tabindex: 0,
                                                    role: "button",
                                                    class: "cursor-pointer rounded px-0.5 transition-colors inline focus:outline-none focus:ring-2 focus:ring-primary",
                                                    class: if is_selected { "bg-primary/20 ring-2 ring-primary" } else if is_highlighted { "bg-yellow-100 dark:bg-yellow-900/30" } else { "" },
                                                    onclick: move |_| handle_verse_click(verse_num),
                                                    onkeydown: move |evt: KeyboardEvent| {
                                                        let key = evt.key();
                                                        let code = evt.code().to_string();
                                                        if key == Key::Enter || code == "Space" {
                                                            if code == "Space" {
                                                                evt.prevent_default(); // Prevent page scroll
                                                            }
                                                            handle_verse_click(verse_num);
                                                        }
                                                    },

                                                    // Verse number
                                                    sup { class: "text-xs text-cyan-500 mr-1 select-none", "{verse_num}" }

                                                    // Verse content
                                                    for vc in verse_content.iter() {
                                                        {render_verse_content(vc)}
                                                    }

                                                    // Highlight count indicator
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

                            // Footnotes section
                            if !data.chapter.footnotes.is_empty() {
                                div { class: "mt-8 pt-4 border-t border-border",
                                    h4 { class: "font-medium mb-2", "Footnotes" }
                                    div { class: "space-y-1 text-sm text-muted-foreground",
                                        for footnote in data.chapter.footnotes.iter() {
                                            div {
                                                key: "fn-{footnote.note_id}",
                                                span { class: "text-blue-500", "[{footnote.note_id}] " }
                                                "{footnote.text}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Verse Action Toolbar (fixed at bottom)
            if *show_toolbar.read() && !selected_verses.read().is_empty() {
                div { class: "fixed bottom-20 left-1/2 -translate-x-1/2 bg-card border border-border rounded-xl shadow-lg p-3 z-50",
                    div { class: "flex items-center gap-3",
                        // Selected count
                        span { class: "text-sm text-muted-foreground",
                            "{selected_verses.read().len()} selected"
                        }

                        div { class: "w-px h-6 bg-border" }

                        // Copy button
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
                                    d: "M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                                }
                            }
                        }

                        // Highlight button (requires auth)
                        if is_authenticated {
                            button {
                                class: "p-2 hover:bg-muted rounded-lg transition",
                                title: "Highlight verses",
                                onclick: highlight_verses,
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-5 h-5 text-yellow-500",
                                    fill: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        d: "M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z"
                                    }
                                }
                            }

                            // Share button (requires auth)
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
                                        let first = *verses.first().unwrap_or(&0);
                                        let last = *verses.last().unwrap_or(&0);

                                        // Build verse text (same format as copy button)
                                        let mut text_parts = Vec::new();
                                        for v in verses.iter() {
                                            for content in &data.chapter.content {
                                                if let ChapterContent::Verse { number, content: verse_content } = content {
                                                    if number == v {
                                                        let text = verse_to_plain_text(verse_content);
                                                        text_parts.push(format!("{} {}", v, text));
                                                    }
                                                }
                                            }
                                        }
                                        let verse_text = text_parts.join(" ");

                                        // Build reference string
                                        let reference = if first == last {
                                            format!("{} {}:{} ({})", book_name, chapter, first, translation)
                                        } else {
                                            format!("{} {}:{}-{} ({})", book_name, chapter, first, last, translation)
                                        };

                                        // Build nostr.blue URL
                                        let url = format!("https://nostr.blue/bible/{}/{}/{}", translation, book, chapter);

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
                                        d: "M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z"
                                    }
                                }
                            }
                        }

                        // Close button
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
                                    d: "M6 18L18 6M6 6l12 12"
                                }
                            }
                        }
                    }
                }
            }

            // Share Modal
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
        }
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
            rsx! { br {} }
        }
    }
}
