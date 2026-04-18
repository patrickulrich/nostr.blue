use crate::services::bible_api::{fetch_commentary_chapter, CommentaryChapterResponse};
use dioxus::prelude::*;

static DEFAULT_COMMENTARY: &str = "matthew-henry";

#[component]
pub fn BibleCommentaryPanel(
    translation: String,
    book: String,
    chapter: u32,
) -> Element {
    let mut selected_commentary = use_signal(|| DEFAULT_COMMENTARY.to_string());
    let mut commentary_data: Signal<Option<Result<CommentaryChapterResponse, String>>> =
        use_signal(|| None);
    let mut loading = use_signal(|| false);
    let mut show_intro = use_signal(|| true);

    let commentary_id = selected_commentary.read().clone();
    let cache_key = format!("{}:{}:{}:{}", commentary_id, translation, book, chapter);

    use_effect(move || {
        let key = cache_key.clone();
        let cid = selected_commentary.read().clone();
        let b = book.clone();
        loading.set(true);
        commentary_data.set(None);
        spawn(async move {
            let result = fetch_commentary_chapter(&cid, &b, chapter).await;
            let _ = key;
            commentary_data.set(Some(result));
            loading.set(false);
        });
    });

    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center gap-2",
                select {
                    class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-sm",
                    value: "{selected_commentary}",
                    onchange: move |evt| selected_commentary.set(evt.value()),
                    option { value: "matthew-henry", "Matthew Henry" }
                    option { value: "adam-clarke", "Adam Clarke" }
                    option { value: "jamieson-fausset-brown", "Jamieson-Fausset-Brown" }
                    option { value: "john-gill", "John Gill" }
                    option { value: "keil-delitzsch", "Keil & Delitzsch (OT)" }
                    option { value: "tyndale", "Tyndale Study Notes" }
                }
            }

            if *loading.read() {
                div { class: "space-y-4 animate-pulse",
                    for i in 0..5 {
                        div {
                            key: "{i}",
                            class: "h-20 bg-muted rounded",
                        }
                    }
                }
            } else {
                match &*commentary_data.read() {
                    None => rsx! {
                        div { class: "text-center py-8 text-muted-foreground",
                            "Select a commentary to view"
                        }
                    },
                    Some(Err(err)) => rsx! {
                        div { class: "text-center py-8",
                            p { class: "text-destructive", "Failed to load commentary" }
                            p { class: "text-sm text-muted-foreground mt-1", "{err}" }
                        }
                    },
                    Some(Ok(data)) => {
                        let intro = data.chapter.introduction.clone();
                        let has_intro = intro.is_some();
                        rsx! {
                            div { class: "space-y-4",
                                if has_intro {
                                    {
                                        let intro_text = intro.unwrap_or_default();
                                        rsx! {
                                            div { class: "bg-muted/30 rounded-lg overflow-hidden",
                                                button {
                                                    class: "w-full flex items-center justify-between px-4 py-3 text-left hover:bg-muted/50 transition",
                                                    onclick: move |_| {
                                                        let current = *show_intro.read();
                                                        show_intro.set(!current);
                                                    },
                                                    span { class: "text-sm font-medium", "Chapter Introduction" }
                                                    span { class: "text-xs text-muted-foreground",
                                                        if *show_intro.read() { "▼" } else { "▶" }
                                                    }
                                                }
                                                if *show_intro.read() {
                                                    div { class: "px-4 pb-3 text-sm text-muted-foreground leading-relaxed",
                                                        "{intro_text}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                for verse in data.chapter.content.iter() {
                                    {
                                        let verse_num = verse.number;
                                        let has_content = !verse.content.is_empty();
                                        rsx! {
                                            div {
                                                key: "cv-{verse_num}",
                                                class: "space-y-1",
                                                if has_content {
                                                    h4 { class: "text-sm font-semibold text-primary",
                                                        "Verse {verse_num}"
                                                    }
                                                    for (idx, para) in verse.content.iter().enumerate() {
                                                        p {
                                                            key: "cp-{verse_num}-{idx}",
                                                            class: "text-sm text-foreground/80 leading-relaxed",
                                                            "{para}"
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
}
