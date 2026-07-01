use crate::components::rich_content::NostrUriRenderer;
use crate::components::SensitiveContent;
use crate::routes::nips::registry::SpecType;
use crate::routes::nips::spec_links::rewrite_spec_link_html;
use crate::utils::markdown::{extract_nostr_uris, render_markdown};
use dioxus::prelude::*;

const PROSE_CLASSES: &str = "article-content prose prose-lg prose-neutral dark:prose-invert max-w-none break-words
    [&_h1]:text-4xl [&_h1]:font-bold [&_h1]:mt-8 [&_h1]:mb-4
    [&_h2]:text-3xl [&_h2]:font-bold [&_h2]:mt-6 [&_h2]:mb-3
    [&_h3]:text-2xl [&_h3]:font-semibold [&_h3]:mt-5 [&_h3]:mb-2
    [&_p]:my-4 [&_p]:leading-relaxed
    [&_a]:text-primary [&_a]:underline hover:[&_a]:text-primary/80
    [&_ul]:my-4 [&_ul]:pl-6 [&_ul]:list-disc
    [&_ol]:my-4 [&_ol]:pl-6 [&_ol]:list-decimal
    [&_li]:my-2
    [&_blockquote]:border-l-4 [&_blockquote]:border-primary [&_blockquote]:pl-4 [&_blockquote]:my-4 [&_blockquote]:italic
    [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-sm
    [&_pre]:bg-muted [&_pre]:p-4 [&_pre]:rounded-lg [&_pre]:overflow-x-auto [&_pre]:my-4
    [&_img]:max-w-full [&_img]:h-auto [&_img]:rounded-lg [&_img]:my-6
    [&_table]:w-full [&_table]:my-4
    [&_th]:border [&_th]:border-border [&_th]:bg-muted [&_th]:px-4 [&_th]:py-2 [&_th]:font-semibold
    [&_td]:border [&_td]:border-border [&_td]:px-4 [&_td]:py-2";

enum ContentSegment {
    Html(String),
    NostrUri(usize),
}

fn split_html_on_markers(html: &str, uri_count: usize) -> Vec<ContentSegment> {
    let mut segments = Vec::new();
    let mut remaining = html;
    for idx in 0..uri_count {
        let marker = format!("%%NOSTR_BLUE_EMBED_{}%%", idx);
        if let Some(pos) = remaining.find(&marker) {
            if pos > 0 {
                segments.push(ContentSegment::Html(remaining[..pos].to_string()));
            }
            segments.push(ContentSegment::NostrUri(idx));
            remaining = &remaining[pos + marker.len()..];
        }
    }
    if !remaining.is_empty() {
        segments.push(ContentSegment::Html(remaining.to_string()));
    }
    segments
}

#[component]
pub fn ArticleContent(
    content: String,
    #[props(default)] content_warning: Option<Option<String>>,
    /// When set, cross-spec `.md` links in the rendered content are rewritten:
    /// supported specs → in-app `/nips/<route_id>`, unsupported → upstream URL
    /// in a new tab. Used by the `/nips` detail page.
    #[props(default)]
    spec_source: Option<SpecType>,
) -> Element {
    let (content_with_markers, nostr_uris) = extract_nostr_uris(&content);
    let html_content = render_markdown(&content_with_markers);
    let html_content = match spec_source {
        Some(source) => rewrite_spec_link_html(&html_content, source),
        None => html_content,
    };
    let segments = split_html_on_markers(&html_content, nostr_uris.len());

    let rendered = rsx! {
        for (seg_idx, segment) in segments.into_iter().enumerate() {
            match segment {
                ContentSegment::Html(html) => {
                    let key = format!("html-{seg_idx}");
                    rsx! {
                        div {
                            key: "{key}",
                            dangerous_inner_html: "{html}",
                            class: PROSE_CLASSES,
                        }
                    }
                }
                ContentSegment::NostrUri(idx) => {
                    let uri = nostr_uris[idx].clone();
                    let key = format!("nostr-{idx}");
                    rsx! {
                        div {
                            key: "{key}",
                            class: "my-2",
                            NostrUriRenderer { uri }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        if let Some(reason) = content_warning {
            SensitiveContent { reason, {rendered} }
        } else {
            {rendered}
        }
    }
}
