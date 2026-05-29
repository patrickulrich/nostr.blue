//! Quran Routes Module
//! Routes for the Quran reading app
mod surah;
mod search;

use crate::stores::quran_store::{
    self, split_surahs_by_revelation, FAVORITE_EDITIONS, TRANSLATION_EDITIONS,
    DOWNLOADED_EDITIONS, QURAN_STORE_INITIALIZED, LOADING_EDITIONS,
    RECOMMENDED_TRANSLATIONS,
};
pub use surah::QuranSurah;
pub use search::QuranSearch;
use dioxus::prelude::*;

#[component]
pub fn QuranHome() -> Element {
    let selected_translation =
        use_signal(|| quran_store::DEFAULT_TRANSLATION_EDITION.to_string());
    let mut active_tab = use_signal(|| "all");
    let surah_list = quran_store::SURAH_LIST.read().clone();

    use_effect(move || {
        if !*QURAN_STORE_INITIALIZED.read() {
            spawn(async move {
                if let Err(e) = quran_store::initialize().await {
                    log::error!("Failed to initialize Quran store: {}", e);
                }
            });
        }
    });

    let translations = TRANSLATION_EDITIONS.read();
    let loading_editions = *LOADING_EDITIONS.read();
    let favorites = FAVORITE_EDITIONS.read();
    let downloaded = DOWNLOADED_EDITIONS.read();
    let downloaded_translations: Vec<_> = translations
        .iter()
        .filter(|t| downloaded.contains(&t.identifier))
        .collect();
    let fav_not_downloaded: Vec<_> = translations
        .iter()
        .filter(|t| !downloaded.contains(&t.identifier) && favorites.contains(&t.identifier))
        .collect();
    let recommended: Vec<_> = translations
        .iter()
        .filter(|t| RECOMMENDED_TRANSLATIONS.contains(&t.identifier.as_str()))
        .collect();
    let _total_count = translations.len();

    let (meccan, medinan) = split_surahs_by_revelation(&surah_list);

    rsx! {
        div { class: "max-w-5xl mx-auto p-4 space-y-6",
            div { class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4",
                h1 { class: "text-3xl font-bold", "Quran" }
                Link {
                    to: crate::routes::Route::QuranSearch {},
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
                if loading_editions {
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
                        }
                    }
                }
            }

            div { class: "flex gap-2 border-b border-border",
                button {
                    class: if *active_tab.read() == "all" { "px-4 py-2 font-medium border-b-2 border-primary text-primary" } else { "px-4 py-2 font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("all"),
                    "All"
                }
                button {
                    class: if *active_tab.read() == "meccan" { "px-4 py-2 font-medium border-b-2 border-primary text-primary" } else { "px-4 py-2 font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("meccan"),
                    "Meccan"
                }
                button {
                    class: if *active_tab.read() == "medinan" { "px-4 py-2 font-medium border-b-2 border-primary text-primary" } else { "px-4 py-2 font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("medinan"),
                    "Medinan"
                }
                button {
                    class: if *active_tab.read() == "juz" { "px-4 py-2 font-medium border-b-2 border-primary text-primary" } else { "px-4 py-2 font-medium text-muted-foreground hover:text-foreground" },
                    onclick: move |_| active_tab.set("juz"),
                    "Juz"
                }
            }

            if *active_tab.read() == "juz" {
                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-5 lg:grid-cols-6 gap-3",
                    for juz in 1..=30 {
                        JuzCard { juz }
                    }
                }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                    {
                        let surahs_to_show = if *active_tab.read() == "meccan" {
                            meccan.clone()
                        } else if *active_tab.read() == "medinan" {
                            medinan.clone()
                        } else {
                            surah_list.clone()
                        };
                        let current_translation = selected_translation.read().clone();
                        rsx! {
                            for surah in surahs_to_show.iter() {
                                SurahCard {
                                    key: "{surah.number}",
                                    surah: surah.clone(),
                                    translation: current_translation.clone(),
                                }
                            }
                        }
                    }
                }
            }

            if let Some((last_surah, last_edition)) = quran_store::LAST_POSITION.read().clone() {
                if let Some(surah_ref) = quran_store::get_surah_ref(last_surah) {
                    div { class: "mt-8 p-4 bg-muted/50 rounded-lg",
                        div { class: "flex items-center justify-between",
                            div {
                                p { class: "text-sm text-muted-foreground", "Continue Reading" }
                                p { class: "font-medium",
                                    {format!("{} ({})", surah_ref.english_name, last_edition)}
                                }
                            }
                            Link {
                                to: crate::routes::Route::QuranSurah { surah: last_surah },
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                                "Continue"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn translation_chip(
    t: crate::services::quran_api::Edition,
    mut selected: Signal<String>,
    is_favorite: bool,
    is_downloaded: bool,
) -> Element {
    let current_selection = selected.read().clone();
    let is_selected = current_selection == t.identifier;
    let tid = t.identifier.clone();
    let display_name = if t.english_name.len() <= 20 {
        t.english_name.clone()
    } else {
        format!("{}...", &t.english_name[..17.min(t.english_name.len())])
    };
    rsx! {
        button {
            key: "{t.identifier}",
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
fn SurahCard(
    surah: crate::services::quran_api::SurahRef,
    translation: String,
) -> Element {
    rsx! {
        Link {
            to: crate::routes::Route::QuranSurah { surah: surah.number },
            class: "w-full p-3 bg-card border border-border rounded-lg hover:border-primary/50 hover:bg-accent/50 transition text-left",
            div { class: "flex items-start justify-between",
                span { class: "text-sm text-muted-foreground font-medium", "{surah.number}" }
                span { class: "text-xs text-muted-foreground",
                    "{surah.number_of_ayahs} ayahs"
                }
            }
            div { class: "font-medium text-sm mt-1 truncate", "{surah.english_name}" }
            div {
                class: "text-sm text-right mt-1 leading-relaxed truncate",
                dir: "rtl",
                "{surah.name}"
            }
            div { class: "text-xs text-muted-foreground mt-1 truncate",
                "{surah.english_name_translation}"
            }
        }
    }
}

#[component]
fn JuzCard(juz: u32) -> Element {
    let surah_info = quran_store::get_juz_surahs(juz);
    let first_surah = surah_info.first().map(|s| {
        quran_store::get_surah_ref(s.0)
            .map(|sr| sr.english_name.clone())
            .unwrap_or_else(|| format!("Surah {}", s.0))
    });
    let label = first_surah.unwrap_or_else(|| format!("Juz {}", juz));
    rsx! {
        Link {
            to: crate::routes::Route::QuranSurah {
                surah: surah_info.first().map(|s| s.0).unwrap_or(1),
            },
            class: "w-full p-3 bg-card border border-border rounded-lg hover:border-primary/50 hover:bg-accent/50 transition text-left",
            div { class: "text-sm text-muted-foreground font-medium", "Juz {juz}" }
            div { class: "font-medium text-sm mt-1 truncate", "{label}" }
        }
    }
}
