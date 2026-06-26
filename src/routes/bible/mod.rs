//! Bible Routes Module
//! Routes for the Bible reading app
mod chapter;
mod search;
use crate::components::translation_picker_modal::TranslationPickerModal;
use crate::stores::bible_store::{
    self, split_books_by_testament, Book, BIBLE_STORE_INITIALIZED, CURRENT_BOOKS,
    CURRENT_TRANSLATION, DOWNLOADED_TRANSLATIONS, FAVORITE_TRANSLATIONS, ALL_TRANSLATIONS,
    LOADING_BOOKS, LOADING_TRANSLATIONS, RECOMMENDED_TRANSLATIONS,
};
use crate::utils::format::safe_slice;
pub use chapter::BibleChapter;
use dioxus::prelude::*;
pub use search::BibleSearch;

#[component]
pub fn BibleHome() -> Element {
    let mut selected_translation = use_signal(|| bible_store::DEFAULT_TRANSLATION.to_string());
    let mut show_picker = use_signal(|| false);
    let mut active_tab = use_signal(|| "ot");

    use_effect(move || {
        if !*BIBLE_STORE_INITIALIZED.read() {
            spawn(async move {
                if let Err(e) = bible_store::initialize().await {
                    log::error!("Failed to initialize Bible store: {}", e);
                }
            });
        }
    });

    use_effect(move || {
        let translation = selected_translation.read().clone();
        let store_initialized = *BIBLE_STORE_INITIALIZED.read();
        let current_translation = CURRENT_TRANSLATION.read().clone();
        let books_loaded = !CURRENT_BOOKS.read().is_empty();
        if !store_initialized
            || translation == current_translation
            || (translation == bible_store::DEFAULT_TRANSLATION && books_loaded)
        {
            return;
        }
        spawn(async move {
            if let Err(e) = bible_store::load_books(&translation).await {
                log::error!("Failed to load books: {}", e);
            }
        });
    });

    let translations = ALL_TRANSLATIONS.read();
    let books = CURRENT_BOOKS.read();
    let loading_translations = *LOADING_TRANSLATIONS.read();
    let loading_books = *LOADING_BOOKS.read();
    let testaments = split_books_by_testament(&books);
    let favorites = FAVORITE_TRANSLATIONS.read();
    let downloaded = DOWNLOADED_TRANSLATIONS.read();
    let downloaded_translations: Vec<_> = translations
        .iter()
        .filter(|t| downloaded.contains(&t.id))
        .collect();
    let fav_not_downloaded: Vec<_> = translations
        .iter()
        .filter(|t| !downloaded.contains(&t.id) && favorites.contains(&t.id))
        .collect();
    let recommended: Vec<_> = translations
        .iter()
        .filter(|t| RECOMMENDED_TRANSLATIONS.contains(&t.id.as_str()))
        .collect();
    let total_count = translations.len();

    rsx! {
        div { class: "max-w-5xl mx-auto p-4 space-y-6",
            div { class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4",
                h1 { class: "text-3xl font-bold", "Bible" }
                Link {
                    to: crate::routes::Route::BibleSearch {},
                    class: "px-4 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium inline-flex items-center gap-2",
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-4 h-4",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z",
                        }
                    }
                    "Search"
                }
            }

            div { class: "space-y-2",
                label { class: "text-sm font-medium text-muted-foreground", "Translation" }
                if loading_translations {
                    div { class: "h-10 bg-muted animate-pulse rounded-lg" }
                } else {
                    div { class: "space-y-2",
                        if !downloaded_translations.is_empty() {
                            div { class: "flex flex-wrap gap-2",
                                for t in &downloaded_translations {
                                    { translation_chip((*t).clone(), selected_translation, true, true) }
                                }
                            }
                        }
                        if !fav_not_downloaded.is_empty() {
                            div { class: "flex flex-wrap gap-2",
                                for t in &fav_not_downloaded {
                                    { translation_chip((*t).clone(), selected_translation, true, false) }
                                }
                            }
                        }
                        div { class: "flex flex-wrap gap-2",
                            for t in &recommended {
                                { translation_chip((*t).clone(), selected_translation, false, false) }
                            }
                            button {
                                class: "px-3 py-2 rounded-lg text-sm font-medium transition bg-muted/30 hover:bg-muted text-muted-foreground border border-dashed border-border",
                                onclick: move |_| show_picker.set(true),
                                "Browse all {total_count} translations..."
                            }
                        }
                    }
                }
            }

            div { class: "flex gap-2 border-b border-border",
                button {
                    class: if *active_tab.read() == "ot" { "px-4 py-2 font-medium border-b-2 border-primary text-primary" } else { "px-4 py-2 font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("ot"),
                    "Old Testament"
                }
                button {
                    class: if *active_tab.read() == "nt" { "px-4 py-2 font-medium border-b-2 border-primary text-primary" } else { "px-4 py-2 font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("nt"),
                    "New Testament"
                }
                if !testaments.apocrypha.is_empty() {
                    button {
                        class: if *active_tab.read() == "apoc" { "px-4 py-2 font-medium border-b-2 border-primary text-primary" } else { "px-4 py-2 font-medium text-muted-foreground hover:text-foreground" },
                        onclick: move |_| active_tab.set("apoc"),
                        "Apocrypha"
                    }
                }
            }

            if loading_books {
                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                    for i in 0..20 {
                        div {
                            key: "{i}",
                            class: "h-20 bg-muted animate-pulse rounded-lg",
                        }
                    }
                }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                    {
                        let books_to_show = if *active_tab.read() == "ot" {
                            &testaments.old_testament
                        } else if *active_tab.read() == "apoc" {
                            &testaments.apocrypha
                        } else {
                            &testaments.new_testament
                        };
                        let current_translation = selected_translation.read().clone();
                        rsx! {
                            for book in books_to_show.iter() {
                                BookCard {
                                    key: "{book.id}",
                                    book: book.clone(),
                                    translation: current_translation.clone(),
                                }
                            }
                        }
                    }
                }
            }

            if let Some((last_trans, last_book, last_book_name, last_chapter)) = bible_store::LAST_POSITION
                .read()
                .clone()
            {
                div { class: "mt-8 p-4 bg-muted/50 rounded-lg",
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "text-sm text-muted-foreground", "Continue Reading" }
                            p { class: "font-medium",
                                {format!("{} {} ({})", last_book_name, last_chapter, last_trans)}
                            }
                        }
                        Link {
                            to: crate::routes::Route::BibleChapter {
                                translation: last_trans,
                                book: last_book,
                                chapter: last_chapter,
                            },
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                            "Continue"
                        }
                    }
                }
            }
        }

        if *show_picker.read() {
            TranslationPickerModal {
                show: show_picker,
                on_select: move |id: String| {
                    selected_translation.set(id);
                    show_picker.set(false);
                },
            }
        }
    }
}

fn translation_chip(
    t: crate::stores::bible_store::Translation,
    mut selected: Signal<String>,
    is_favorite: bool,
    is_downloaded: bool,
) -> Element {
    let current_selection = selected.read().clone();
    let is_selected = current_selection == t.id;
    let tid = t.id.clone();
    let lang_code = if t.language != "eng" {
        format!(" ({})", t.language)
    } else {
        String::new()
    };
    let display_name = if t.short_name.len() <= 6 {
        format!("{}{}", t.short_name, lang_code)
    } else {
        format!("{}{}", safe_slice(&t.short_name, 6), lang_code)
    };
    rsx! {
        button {
            key: "{t.id}",
            class: if is_selected {
                "px-3 py-2 rounded-lg text-sm font-medium transition bg-primary text-primary-foreground"
            } else if is_downloaded {
                "px-3 py-2 rounded-lg text-sm font-medium transition bg-green-500/10 text-green-600 dark:text-green-400 hover:bg-green-500/20 border border-green-500/30"
            } else if is_favorite {
                "px-3 py-2 rounded-lg text-sm font-medium transition bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 hover:bg-yellow-500/20 border border-yellow-500/30"
            } else {
                "px-3 py-2 rounded-lg text-sm font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground"
            },
            onclick: move |_| selected.set(tid.clone()),
            if is_downloaded { span { class: "mr-1", "✓" } }
            if is_favorite && !is_downloaded { span { class: "mr-1", "★" } }
            "{display_name}"
        }
    }
}

#[component]
fn BookCard(book: Book, translation: String) -> Element {
    let mut expanded = use_signal(|| false);
    rsx! {
        div { class: "relative",
            button {
                class: "w-full p-3 bg-card border border-border rounded-lg hover:border-primary/50 hover:bg-accent/50 transition text-left",
                onclick: move |_| {
                    let current = *expanded.read();
                    expanded.set(!current);
                },
                div { class: "font-medium text-sm truncate", "{book.common_name}" }
                div { class: "text-xs text-muted-foreground mt-1", "{book.number_of_chapters} chapters" }
            }
            if *expanded.read() {
                div { class: "absolute top-full left-0 right-0 mt-1 p-2 bg-card border border-border rounded-lg shadow-lg z-20 max-h-48 overflow-y-auto",
                    div { class: "grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 gap-1",
                        for ch in 1..=book.number_of_chapters {
                            {
                                let book_id = book.id.clone();
                                let trans = translation.clone();
                                rsx! {
                                    Link {
                                        key: "{ch}",
                                        to: crate::routes::Route::BibleChapter {
                                            translation: trans,
                                            book: book_id,
                                            chapter: ch,
                                        },
                                        class: "w-8 h-8 flex items-center justify-center text-xs rounded hover:bg-primary hover:text-primary-foreground transition",
                                        "{ch}"
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
