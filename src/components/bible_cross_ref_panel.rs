use crate::services::bible_api::{fetch_cross_references, CrossRefChapterResponse};
use dioxus::prelude::*;

static BOOK_NAMES: &[(&str, &str)] = &[
    ("GEN", "Genesis"), ("EXO", "Exodus"), ("LEV", "Leviticus"),
    ("NUM", "Numbers"), ("DEU", "Deuteronomy"), ("JOS", "Joshua"),
    ("JDG", "Judges"), ("RUT", "Ruth"), ("1SA", "1 Samuel"),
    ("2SA", "2 Samuel"), ("1KI", "1 Kings"), ("2KI", "2 Kings"),
    ("1CH", "1 Chronicles"), ("2CH", "2 Chronicles"), ("EZR", "Ezra"),
    ("NEH", "Nehemiah"), ("EST", "Esther"), ("JOB", "Job"),
    ("PSA", "Psalm"), ("PRO", "Proverbs"), ("ECC", "Ecclesiastes"),
    ("SOS", "Song of Solomon"), ("ISA", "Isaiah"), ("JER", "Jeremiah"),
    ("LAM", "Lamentations"), ("EZE", "Ezekiel"), ("DAN", "Daniel"),
    ("HOS", "Hosea"), ("JOE", "Joel"), ("AMO", "Amos"),
    ("OBA", "Obadiah"), ("JON", "Jonah"), ("MIC", "Micah"),
    ("NAH", "Nahum"), ("HAB", "Habakkuk"), ("ZEP", "Zephaniah"),
    ("HAG", "Haggai"), ("ZEC", "Zechariah"), ("MAL", "Malachi"),
    ("MAT", "Matthew"), ("MRK", "Mark"), ("LUK", "Luke"),
    ("JHN", "John"), ("ACT", "Acts"), ("ROM", "Romans"),
    ("1CO", "1 Corinthians"), ("2CO", "2 Corinthians"), ("GAL", "Galatians"),
    ("EPH", "Ephesians"), ("PHP", "Philippians"), ("COL", "Colossians"),
    ("1TH", "1 Thessalonians"), ("2TH", "2 Thessalonians"),
    ("1TI", "1 Timothy"), ("2TI", "2 Timothy"), ("TIT", "Titus"),
    ("PHM", "Philemon"), ("HEB", "Hebrews"), ("JAS", "James"),
    ("1PE", "1 Peter"), ("2PE", "2 Peter"), ("1JN", "1 John"),
    ("2JN", "2 John"), ("3JN", "3 John"), ("JUD", "Jude"),
    ("REV", "Revelation"),
];

fn book_display_name(id: &str) -> &str {
    BOOK_NAMES
        .iter()
        .find(|(abbr, _)| abbr == &id)
        .map(|(_, name)| *name)
        .unwrap_or(id)
}

fn format_ref(book: &str, chapter: u32, verse: u32, end_verse: Option<u32>) -> String {
    let name = book_display_name(book);
    match end_verse {
        Some(ev) => format!("{} {}:{}-{}", name, chapter, verse, ev),
        None => format!("{} {}:{}", name, chapter, verse),
    }
}

fn ref_to_route(translation: &str, book: &str, chapter: u32) -> Option<crate::routes::Route> {
    Some(crate::routes::Route::BibleChapter {
        translation: translation.to_string(),
        book: book.to_string(),
        chapter,
    })
}

#[component]
pub fn BibleCrossRefPanel(
    translation: String,
    book: String,
    chapter: u32,
) -> Element {
    let mut cross_ref_data: Signal<Option<Result<CrossRefChapterResponse, String>>> =
        use_signal(|| None);
    let mut loading = use_signal(|| false);
    let mut expanded_verses: Signal<std::collections::HashSet<u32>> =
        use_signal(std::collections::HashSet::new);

    use_effect(move || {
        loading.set(true);
        cross_ref_data.set(None);
        let b = book.clone();
        spawn(async move {
            let result = fetch_cross_references(&b, chapter).await;
            cross_ref_data.set(Some(result));
            loading.set(false);
        });
    });

    rsx! {
        div { class: "space-y-4",
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
                match &*cross_ref_data.read() {
                    None => rsx! {
                        div { class: "text-center py-8 text-muted-foreground",
                            "Loading cross-references..."
                        }
                    },
                    Some(Err(err)) => rsx! {
                        div { class: "text-center py-8",
                            p { class: "text-destructive", "Failed to load cross-references" }
                            p { class: "text-sm text-muted-foreground mt-1", "{err}" }
                        }
                    },
                    Some(Ok(data)) => {
                        let verses = &data.chapter.content;
                        let total_refs: usize = verses.iter().map(|v| v.references.len()).sum();
                        rsx! {
                            p { class: "text-sm text-muted-foreground",
                                "{total_refs} cross-references for {verses.len()} verses"
                            }
                            for verse in verses.iter() {
                                {
                                    let verse_num = verse.verse;
                                    let refs = &verse.references;
                                    let is_expanded = expanded_verses.read().contains(&verse_num);
                                    let shown_count = if is_expanded { refs.len() } else { 5.min(refs.len()) };
                                    let has_more = refs.len() > 5;
                                    rsx! {
                                        div {
                                            key: "xref-{verse_num}",
                                            class: "py-2",
                                            h4 { class: "text-sm font-semibold mb-1.5",
                                                "Verse {verse_num}"
                                                span { class: "ml-2 text-xs font-normal text-muted-foreground",
                                                    "({refs.len()} refs)"
                                                }
                                            }
                                            div { class: "flex flex-wrap gap-1.5",
                                                for xref in refs.iter().take(shown_count) {
                                                    {
                                                        let score = xref.score.unwrap_or(0);
                                                        let opacity = if score > 500 { "100" } else if score > 200 { "80" } else { "60" };
                                                        let label = format_ref(&xref.book, xref.chapter, xref.verse, xref.end_verse);
                                                        let route = ref_to_route(&translation, &xref.book, xref.chapter);
                                                        rsx! {
                                                            if let Some(r) = route {
                                                                Link {
                                                                    to: r,
                                                                    class: "inline-block px-2 py-1 text-xs rounded bg-muted/50 hover:bg-muted transition opacity-{opacity}",
                                                                    "{label}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                if has_more && !is_expanded {
                                                    {
                                                        let more_count = refs.len() - 5;
                                                        rsx! {
                                                            button {
                                                                class: "px-2 py-1 text-xs rounded bg-muted/30 hover:bg-muted transition text-muted-foreground",
                                                                onclick: move |_| {
                                                                    expanded_verses.write().insert(verse_num);
                                                                },
                                                                "+{more_count} more"
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
}
