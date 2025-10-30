use dioxus::prelude::*;
use crate::utils::markdown::render_markdown;

#[component]
pub fn ArticleContent(content: String) -> Element {
    // Render markdown to sanitized HTML (render_markdown already sanitizes)
    let html_content = render_markdown(&content);

    rsx! {
        div {
            dangerous_inner_html: "{html_content}",
            class: "article-content prose prose-lg prose-neutral dark:prose-invert max-w-none
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
                   [&_td]:border [&_td]:border-border [&_td]:px-4 [&_td]:py-2",
        }
    }
}
