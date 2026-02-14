//! Fuzzy Finder Component
//!
//! Modal file finder with fuzzy subsequence search for repository browsing.
use crate::routes::Route;
use dioxus::prelude::*;

/// Modal fuzzy file finder for repository file navigation
#[component]
pub fn FuzzyFinder(
    files: Vec<String>,
    naddr: String,
    git_ref: String,
    on_close: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut selected_index = use_signal(|| 0usize);
    let nav = use_navigator();

    let naddr_nav = naddr.clone();
    let git_ref_nav = git_ref.clone();

    let filtered = use_memo(move || {
        let q = query.read().to_lowercase();
        if q.is_empty() {
            return files.iter().take(20).cloned().collect::<Vec<_>>();
        }
        files
            .iter()
            .filter(|f| subsequence_match(&q, &f.to_lowercase()))
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
    });

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-start justify-center pt-[20vh]",
            onclick: move |_| on_close.call(()),
            tabindex: 0,
            onkeydown: move |evt: Event<KeyboardData>| {
                let key = evt.key();
                match key {
                    Key::Escape => on_close.call(()),
                    Key::ArrowDown => {
                        let max = filtered.read().len().saturating_sub(1);
                        let cur = *selected_index.read();
                        if cur < max {
                            selected_index.set(cur + 1);
                        }
                    }
                    Key::ArrowUp => {
                        let cur = *selected_index.read();
                        if cur > 0 {
                            selected_index.set(cur - 1);
                        }
                    }
                    Key::Enter => {
                        let items = filtered.read();
                        let idx = *selected_index.read();
                        if let Some(file_path) = items.get(idx) {
                            nav.push(Route::CodeRepoBlob {
                                naddr: naddr_nav.clone(),
                                git_ref: git_ref_nav.clone(),
                                path: file_path.clone(),
                            });
                            on_close.call(());
                        }
                    }
                    _ => {}
                }
            },
            div {
                class: "w-full max-w-lg bg-background border border-border rounded-lg shadow-2xl overflow-hidden",
                onclick: move |evt| evt.stop_propagation(),
                input {
                    class: "w-full px-4 py-3 bg-background text-foreground border-b border-border outline-none text-sm",
                    placeholder: "Search files...",
                    autofocus: true,
                    value: "{query}",
                    oninput: move |evt| {
                        query.set(evt.value().clone());
                        selected_index.set(0);
                    },
                }
                div { class: "max-h-80 overflow-y-auto",
                    for (idx , file_path) in filtered.read().iter().enumerate() {
                        {
                            let is_selected = idx == *selected_index.read();
                            let q = query.read().to_lowercase();
                            let nav_naddr = naddr.clone();
                            let nav_ref = git_ref.clone();
                            let nav_path = file_path.clone();
                            rsx! {
                                div {
                                    key: "{file_path}",
                                    class: if is_selected {
                                        "px-4 py-2 text-sm bg-accent text-accent-foreground cursor-pointer"
                                    } else {
                                        "px-4 py-2 text-sm cursor-pointer hover:bg-accent transition"
                                    },
                                    onclick: move |_| {
                                        nav.push(Route::CodeRepoBlob {
                                            naddr: nav_naddr.clone(),
                                            git_ref: nav_ref.clone(),
                                            path: nav_path.clone(),
                                        });
                                        on_close.call(());
                                    },
                                    onmouseenter: move |_| {
                                        selected_index.set(idx);
                                    },
                                    {highlight_matches(file_path, &q)}
                                }
                            }
                        }
                    }
                    if filtered.read().is_empty() {
                        div { class: "px-4 py-8 text-sm text-muted-foreground text-center",
                            "No files found"
                        }
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn subsequence_match(query: &str, target: &str) -> bool {
    let mut chars = target.chars();
    for qc in query.chars() {
        loop {
            match chars.next() {
                Some(tc) if tc == qc => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

#[allow(dead_code)]
fn highlight_matches(text: &str, query: &str) -> Element {
    if query.is_empty() {
        return rsx! { span { "{text}" } };
    }

    let mut parts: Vec<(String, bool)> = Vec::new();
    let mut current = String::new();
    let mut query_chars = query.chars().peekable();
    let text_lower = text.to_lowercase();

    for (tc, original) in text_lower.chars().zip(text.chars()) {
        if let Some(&qc) = query_chars.peek() {
            if tc == qc {
                if !current.is_empty() {
                    parts.push((current.clone(), false));
                    current.clear();
                }
                parts.push((original.to_string(), true));
                query_chars.next();
                continue;
            }
        }
        current.push(original);
    }
    if !current.is_empty() {
        parts.push((current, false));
    }

    rsx! {
        span {
            for (idx , (text , matched)) in parts.iter().enumerate() {
                if *matched {
                    span {
                        key: "{idx}",
                        class: "text-primary font-bold",
                        "{text}"
                    }
                } else {
                    span { key: "{idx}", "{text}" }
                }
            }
        }
    }
}
