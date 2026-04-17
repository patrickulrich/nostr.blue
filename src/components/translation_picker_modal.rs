use crate::stores::bible_store::{
    download_translation, is_favorite, is_offline_available, remove_offline_translation,
    toggle_favorite, ALL_TRANSLATIONS, DOWNLOADED_TRANSLATIONS, DOWNLOAD_IN_PROGRESS,
    FAVORITE_TRANSLATIONS, GROUPED_TRANSLATIONS, RECOMMENDED_TRANSLATIONS,
};
use dioxus::prelude::*;

#[component]
pub fn TranslationPickerModal(show: Signal<bool>, on_select: EventHandler<String>) -> Element {
    let mut search = use_signal(String::new);
    let mut expanded_languages: Signal<Vec<String>> = use_signal(Vec::new);

    let translations = ALL_TRANSLATIONS.read();
    let favorites = FAVORITE_TRANSLATIONS.read();
    let downloaded = DOWNLOADED_TRANSLATIONS.read();

    let downloaded_favs: Vec<_> = translations
        .iter()
        .filter(|t| downloaded.contains(&t.id) && favorites.contains(&t.id))
        .collect();
    let downloaded_only: Vec<_> = translations
        .iter()
        .filter(|t| downloaded.contains(&t.id) && !favorites.contains(&t.id))
        .collect();
    let fav_not_downloaded: Vec<_> = translations
        .iter()
        .filter(|t| !downloaded.contains(&t.id) && favorites.contains(&t.id))
        .collect();
    let recommended: Vec<_> = translations
        .iter()
        .filter(|t| RECOMMENDED_TRANSLATIONS.contains(&t.id.as_str()))
        .collect();
    let grouped = GROUPED_TRANSLATIONS.read();

    let query = search.read().trim().to_lowercase();
    let has_search = query.len() >= 2;

    let search_results: Vec<_> = if has_search {
        translations
            .iter()
            .filter(|t| {
                let q = &query;
                t.id.to_lowercase().contains(q)
                    || t.english_name.to_lowercase().contains(q)
                    || t.name.to_lowercase().contains(q)
                    || t.short_name.to_lowercase().contains(q)
                    || t.language_english_name
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(q)
                    || t.language.to_lowercase().contains(q)
            })
            .take(50)
            .collect()
    } else {
        Vec::new()
    };

    let close = move |_| show.set(false);

    rsx! {
        div { class: "fixed inset-0 z-50 flex items-start justify-center pt-4 px-4",
            div { class: "fixed inset-0 bg-black/50 backdrop-blur-sm", onclick: close }
            div { class: "relative z-10 w-full max-w-2xl max-h-[85vh] bg-background border border-border rounded-xl shadow-2xl flex flex-col",
                div { class: "flex items-center gap-3 p-4 border-b border-border",
                    button {
                        class: "p-2 hover:bg-muted rounded-lg transition",
                        onclick: close,
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-5 h-5",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { stroke_linecap: "round", stroke_linejoin: "round", d: "M6 18L18 6M6 6l12 12" }
                        }
                    }
                    h2 { class: "text-lg font-semibold", "Choose Translation" }
                }

                div { class: "px-4 pt-3",
                    div { class: "relative",
                        input {
                            r#type: "text",
                            placeholder: "Search by name, language, or ID...",
                            class: "w-full px-4 py-2.5 pl-10 border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary bg-background text-sm",
                            value: "{search}",
                            oninput: move |evt| search.set(evt.value()),
                        }
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { stroke_linecap: "round", stroke_linejoin: "round", d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" }
                        }
                    }
                }

                div { class: "flex-1 overflow-y-auto p-4 space-y-4",
                    if has_search {
                        if search_results.is_empty() {
                            div { class: "text-center py-8",
                                p { class: "text-muted-foreground", "No translations found" }
                            }
                        } else {
                            div { class: "space-y-1",
                                p { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2",
                                    "{search_results.len()} results"
                                }
                                for t in &search_results {
                                    { translation_row((*t).clone(), on_select) }
                                }
                            }
                        }
                    } else {
                        if !downloaded_favs.is_empty() || !downloaded_only.is_empty() {
                            {
                                let dl_count = downloaded_favs.len() + downloaded_only.len();
                                rsx! {
                                    div { class: "space-y-2",
                                        p { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide",
                                            "⬇ Downloaded ({dl_count})"
                                        }
                                        div { class: "space-y-1",
                                            for t in &downloaded_favs {
                                                { translation_row((*t).clone(), on_select) }
                                            }
                                            for t in &downloaded_only {
                                                { translation_row((*t).clone(), on_select) }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if !fav_not_downloaded.is_empty() {
                            div { class: "space-y-2",
                                p { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide",
                                    "★ Favorites ({fav_not_downloaded.len()})"
                                }
                                div { class: "space-y-1",
                                    for t in &fav_not_downloaded {
                                        { translation_row((*t).clone(), on_select) }
                                    }
                                }
                            }
                        }

                        div { class: "space-y-2",
                            p { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide",
                                "Recommended"
                            }
                            div { class: "flex flex-wrap gap-2",
                                for t in &recommended {
                                    {
                                        let id = t.id.clone();
                                        let display = if t.language != "eng" {
                                            format!("{} ({})", t.short_name, t.language)
                                        } else {
                                            t.short_name.clone()
                                        };
                                        let is_fav = is_favorite(&t.id);
                                        let is_dl = is_offline_available(&t.id);
                                        rsx! {
                                            button {
                                                key: "{t.id}",
                                                class: "px-3 py-2 rounded-lg text-sm font-medium transition bg-muted/50 hover:bg-muted text-foreground",
                                                onclick: move |_| on_select.call(id.clone()),
                                                if is_dl { span { class: "mr-1", "✓" } }
                                                if is_fav { span { class: "mr-1", "★" } }
                                                "{display}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "space-y-2",
                            for (lang, trans) in grouped.iter() {
                                {
                                    let lang_key = lang.clone();
                                    let lang_key_for_toggle = lang.clone();
                                    let is_expanded = expanded_languages.read().contains(&lang_key);
                                    let count = trans.len();
                                    let is_english = lang == "English";
                                    rsx! {
                                        div { key: "{lang_key}",
                                            button {
                                                class: "flex items-center justify-between w-full px-2 py-2 hover:bg-muted rounded-lg transition text-left",
                                                onclick: move |_| {
                                                    let mut expanded = expanded_languages.write();
                                                    if let Some(pos) = expanded.iter().position(|l| l == &lang_key_for_toggle) {
                                                        expanded.remove(pos);
                                                    } else {
                                                        expanded.push(lang_key_for_toggle.clone());
                                                    }
                                                },
                                                div { class: "flex items-center gap-2",
                                                    span { class: "text-xs text-muted-foreground",
                                                        if is_expanded { "▼" } else { "▶" }
                                                    }
                                                    span { class: "text-sm font-medium", "{lang}" }
                                                }
                                                span { class: "text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded-full",
                                                    "{count}"
                                                }
                                            }
                                            if is_expanded || is_english {
                                                div { class: "ml-4 space-y-1 mt-1",
                                                    for t in trans {
                                                        { translation_row(t.clone(), on_select) }
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
            }
        }
    }
}

fn translation_row(
    t: crate::stores::bible_store::Translation,
    on_select: EventHandler<String>,
) -> Element {
    let id = t.id.clone();
    let id_for_star = t.id.clone();
    let id_for_dl = t.id.clone();
    let id_for_remove = t.id.clone();
    let is_fav = is_favorite(&t.id);
    let is_dl = is_offline_available(&t.id);
    let downloading = *DOWNLOAD_IN_PROGRESS.read() == Some(t.id.clone());
    let lang_label = t
        .language_english_name
        .as_deref()
        .filter(|l| *l != "English")
        .map(|l| format!(" ({})", l))
        .unwrap_or_default();
    let is_rtl = t.text_direction == "rtl";
    let book_count = if t.number_of_apocryphal_books.is_some() {
        format!(
            "{}+{} books",
            t.number_of_books,
            t.number_of_apocryphal_books.unwrap_or(0)
        )
    } else {
        format!("{} books", t.number_of_books)
    };

    rsx! {
        div {
            key: "{t.id}",
            class: "flex items-center justify-between px-3 py-2 hover:bg-accent/50 rounded-lg transition group",
            div { class: "flex-1 min-w-0 cursor-pointer",
                onclick: move |_| on_select.call(id.clone()),
                div { class: "flex items-center gap-2",
                    span { class: "text-sm font-medium", "{t.short_name}" }
                    if is_rtl {
                        span { class: "text-[10px] px-1.5 py-0.5 bg-muted rounded text-muted-foreground font-medium", "RTL" }
                    }
                }
                p { class: "text-xs text-muted-foreground truncate",
                    "{t.english_name}{lang_label} · {book_count}"
                }
            }
            div { class: "flex items-center gap-0.5 shrink-0",
                if downloading {
                    div { class: "p-1.5",
                        div { class: "w-4 h-4 border-2 border-muted-foreground border-t-transparent rounded-full animate-spin" }
                    }
                } else if is_dl {
                    button {
                        class: "p-1.5 hover:bg-muted rounded transition text-green-500",
                        title: "Downloaded - click to remove",
                        onclick: move |_| {
                            let id = id_for_remove.clone();
                            spawn(async move {
                                let _ = remove_offline_translation(&id).await;
                            });
                        },
                        "✓"
                    }
                } else {
                    button {
                        class: "p-1.5 hover:bg-muted rounded transition text-muted-foreground opacity-0 group-hover:opacity-100",
                        title: "Download for offline",
                        onclick: move |_| {
                            let id = id_for_dl.clone();
                            spawn(async move {
                                let _ = download_translation(&id).await;
                            });
                        },
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-4 h-4",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { stroke_linecap: "round", stroke_linejoin: "round", d: "M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" }
                        }
                    }
                }
                button {
                    class: "p-1.5 hover:bg-muted rounded transition text-muted-foreground",
                    onclick: move |_| toggle_favorite(&id_for_star),
                    if is_fav {
                        span { class: "text-yellow-500", "★" }
                    } else {
                        span { class: "opacity-0 group-hover:opacity-100 transition-opacity", "☆" }
                    }
                }
            }
        }
    }
}
