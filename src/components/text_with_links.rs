use crate::routes::Route;
use dioxus::prelude::*;
use once_cell::sync::Lazy;
use regex::Regex;

static URL_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"https?://[^\s]+").expect("Failed to compile URL regex"));

static NOSTR_URI_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)nostr:(npub1|nprofile1|note1|nevent1|naddr1)[a-zA-Z0-9]+")
        .expect("Failed to compile nostr URI regex")
});

fn clean_url_trailing_punctuation(url: &str) -> &str {
    url.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"', '>'])
}

enum TextSegment {
    Text(String),
    Url(String),
    NostrUri(String),
}

fn segment_text(content: &str) -> Vec<TextSegment> {
    let mut matches: Vec<(usize, usize, TextSegment)> = Vec::new();

    for mat in URL_PATTERN.find_iter(content) {
        let raw_url = mat.as_str();
        let url = clean_url_trailing_punctuation(raw_url).to_string();
        let actual_end = mat.start() + url.len();
        matches.push((mat.start(), actual_end, TextSegment::Url(url)));
    }

    for mat in NOSTR_URI_PATTERN.find_iter(content) {
        let uri = mat.as_str().to_string();
        matches.push((mat.start(), mat.end(), TextSegment::NostrUri(uri)));
    }

    matches.sort_by_key(|m| m.0);

    let mut segments = Vec::new();
    let mut last_end = 0;

    for (start, end, segment) in matches {
        if start < last_end {
            continue;
        }
        if start > last_end {
            let text = content[last_end..start].to_string();
            if !text.is_empty() {
                segments.push(TextSegment::Text(text));
            }
        }
        segments.push(segment);
        last_end = end;
    }

    if last_end < content.len() {
        let text = content[last_end..].to_string();
        if !text.is_empty() {
            segments.push(TextSegment::Text(text));
        }
    }

    if segments.is_empty() && !content.is_empty() {
        segments.push(TextSegment::Text(content.to_string()));
    }

    segments
}

#[component]
pub fn TextWithLinks(content: String) -> Element {
    let segments = segment_text(&content);
    let link_class = "text-primary hover:underline break-all";
    let external_class = "text-primary hover:underline break-all";

    rsx! {
        span {
            for (idx, segment) in segments.into_iter().enumerate() {
                match segment {
                    TextSegment::Text(text) => rsx! {
                        span { key: "text-{idx}", "{text}" }
                    },
                    TextSegment::Url(url) => rsx! {
                        a {
                            key: "url-{idx}",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "{external_class}",
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            "{url}"
                        }
                    },
                    TextSegment::NostrUri(uri) => {
                        let identifier = if uri.len() > 6 && uri[..6].eq_ignore_ascii_case("nostr:") {
                            uri[6..].to_string()
                        } else {
                            uri.clone()
                        };
                        let display = if uri.len() > 6 && uri[..6].eq_ignore_ascii_case("nostr:") {
                            uri[6..].to_string()
                        } else {
                            uri.clone()
                        };
                        rsx! {
                            Link {
                                key: "nostr-{idx}",
                                to: Route::Nip19Handler { identifier },
                                class: "{link_class}",
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                "{display}"
                            }
                        }
                    }
                }
            }
        }
    }
}
