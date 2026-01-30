//! Bible Routes Module
//! Routes for the Bible reading app

mod chapter;
mod search;

pub use chapter::BibleChapter;
pub use search::BibleSearch;

use dioxus::prelude::*;

use crate::stores::bible_store::{
    self, split_books_by_testament, Book, BIBLE_STORE_INITIALIZED, CURRENT_BOOKS,
    CURRENT_TRANSLATION, ENGLISH_TRANSLATIONS, LOADING_BOOKS, LOADING_TRANSLATIONS,
};

/// Bible Home Page - Book/Chapter picker
#[component]
pub fn BibleHome() -> Element {
    // State
    let mut selected_translation = use_signal(|| bible_store::DEFAULT_TRANSLATION.to_string());
    let mut show_all_translations = use_signal(|| false);
    let mut active_tab = use_signal(|| "ot"); // "ot" or "nt"

    // Initialize store on mount
    use_effect(move || {
        if !*BIBLE_STORE_INITIALIZED.read() {
            spawn(async move {
                if let Err(e) = bible_store::initialize().await {
                    log::error!("Failed to initialize Bible store: {}", e);
                }
            });
        }
    });

    // Load books when translation changes (only after store is initialized)
    use_effect(move || {
        let translation = selected_translation.read().clone();
        let store_initialized = *BIBLE_STORE_INITIALIZED.read();
        let current_translation = CURRENT_TRANSLATION.read().clone();
        let books_loaded = !CURRENT_BOOKS.read().is_empty();

        // Skip if:
        // 1. Store not initialized yet (initialize() will handle default translation)
        // 2. Translation already loaded (current_translation matches)
        // 3. Books already loaded for default translation (prevents race with initialize())
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

    let translations = ENGLISH_TRANSLATIONS.read();
    let books = CURRENT_BOOKS.read();
    let loading_translations = *LOADING_TRANSLATIONS.read();
    let loading_books = *LOADING_BOOKS.read();

    // Split books into testaments
    let (old_testament, new_testament) = split_books_by_testament(&books);

    rsx! {
        div { class: "max-w-5xl mx-auto p-4 space-y-6",

            // Header
            div { class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4",
                h1 { class: "text-3xl font-bold", "Bible" }

                // Search link
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
                            d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                        }
                    }
                    "Search"
                }
            }

            // Translation Selector
            div { class: "space-y-2",
                label { class: "text-sm font-medium text-muted-foreground", "Translation" }

                if loading_translations {
                    div { class: "h-10 bg-muted animate-pulse rounded-lg" }
                } else {
                    div { class: "flex flex-wrap gap-2",
                        // Show recommended translations first
                        {
                            let display_translations = if *show_all_translations.read() {
                                translations.clone()
                            } else {
                                translations.iter().take(6).cloned().collect::<Vec<_>>()
                            };

                            // Read selection once before loop to avoid per-iteration subscriptions
                            let current_selection = selected_translation.read().clone();
                            rsx! {
                                for t in display_translations {
                                    {
                                        let is_selected = current_selection == t.id;
                                        let tid = t.id.clone();
                                        rsx! {
                                            button {
                                                key: "{t.id}",
                                                class: if is_selected {
                                                    "px-3 py-2 rounded-lg text-sm font-medium transition bg-primary text-primary-foreground"
                                                } else {
                                                    "px-3 py-2 rounded-lg text-sm font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground"
                                                },
                                                onclick: move |_| selected_translation.set(tid.clone()),
                                                "{t.short_name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Show more button
                        if translations.len() > 6 {
                            button {
                                class: "px-3 py-2 rounded-lg text-sm font-medium transition bg-muted/30 hover:bg-muted text-muted-foreground",
                                onclick: move |_| {
                                    let current = *show_all_translations.read();
                                    show_all_translations.set(!current);
                                },
                                if *show_all_translations.read() { "Show Less" } else { "More..." }
                            }
                        }
                    }
                }
            }

            // Testament Tabs
            div { class: "flex gap-2 border-b border-border",
                button {
                    class: if *active_tab.read() == "ot" {
                        "px-4 py-2 font-medium border-b-2 border-primary text-primary"
                    } else {
                        "px-4 py-2 font-medium text-muted-foreground hover:text-foreground"
                    },
                    onclick: move |_| active_tab.set("ot"),
                    "Old Testament"
                }
                button {
                    class: if *active_tab.read() == "nt" {
                        "px-4 py-2 font-medium border-b-2 border-primary text-primary"
                    } else {
                        "px-4 py-2 font-medium text-muted-foreground hover:text-foreground"
                    },
                    onclick: move |_| active_tab.set("nt"),
                    "New Testament"
                }
            }

            // Books Grid
            if loading_books {
                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                    for i in 0..20 {
                        div {
                            key: "{i}",
                            class: "h-20 bg-muted animate-pulse rounded-lg"
                        }
                    }
                }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                    {
                        let books_to_show = if *active_tab.read() == "ot" {
                            &old_testament
                        } else {
                            &new_testament
                        };

                        // Read selection once before loop to avoid per-iteration subscriptions
                        let current_translation = selected_translation.read().clone();
                        rsx! {
                            for book in books_to_show.iter() {
                                BookCard {
                                    key: "{book.id}",
                                    book: book.clone(),
                                    translation: current_translation.clone()
                                }
                            }
                        }
                    }
                }
            }

            // Continue Reading (if we have a last position)
            if let Some((last_trans, last_book, last_book_name, last_chapter)) = bible_store::LAST_POSITION.read().clone() {
                div { class: "mt-8 p-4 bg-muted/50 rounded-lg",
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "text-sm text-muted-foreground", "Continue Reading" }
                            p { class: "font-medium",
                                // Use stored book name directly (correct translation)
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
    }
}

/// Book card component
#[component]
fn BookCard(book: Book, translation: String) -> Element {
    let mut expanded = use_signal(|| false);

    rsx! {
        div { class: "relative",
            // Main book button
            button {
                class: "w-full p-3 bg-card border border-border rounded-lg hover:border-primary/50 hover:bg-accent/50 transition text-left",
                onclick: move |_| {
                    let current = *expanded.read();
                    expanded.set(!current);
                },

                div { class: "font-medium text-sm truncate", "{book.common_name}" }
                div { class: "text-xs text-muted-foreground mt-1",
                    "{book.number_of_chapters} chapters"
                }
            }

            // Chapter selector dropdown
            if *expanded.read() {
                div {
                    class: "absolute top-full left-0 right-0 mt-1 p-2 bg-card border border-border rounded-lg shadow-lg z-20 max-h-48 overflow-y-auto",

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
