//! README Viewer Component
//!
//! Displays repository README with markdown rendering.
//! Uses pulldown-cmark for parsing and ammonia for sanitization.
//! Styled to match gittr's readme-section.tsx pattern.
//! Supports mermaid diagram rendering via mermaid.js CDN.
use crate::utils::markdown::render_markdown;
use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(
    inline_js = r#"
export function initMermaidDiagrams() {
    // Find mermaid divs; skip if none present
    const mermaidDivs = document.querySelectorAll('div.mermaid:not([data-processed])');
    if (mermaidDivs.length === 0) return;

    if (window.mermaid) {
        try {
            const nodes = Array.from(mermaidDivs);
            window.mermaid.run({ nodes }).then(() => {
                nodes.forEach(el => el.setAttribute('data-processed', 'true'));
            }).catch(e => {
                console.warn('Mermaid render error:', e);
            });
        } catch (e) {
            console.warn('Mermaid render error:', e);
        }
        return;
    }

    // Load mermaid.js from CDN (skip if already loading/loaded)
    if (window.__mermaidLoaderStatus === 'loading' || window.__mermaidLoaderStatus === 'loaded') return;
    window.__mermaidLoaderStatus = 'loading';
    const script = document.createElement('script');
    script.src = 'https://cdn.jsdelivr.net/npm/mermaid@10.9.3/dist/mermaid.min.js';
    script.crossOrigin = 'anonymous';
    script.onload = () => {
        window.__mermaidLoaderStatus = 'loaded';
        try {
            window.mermaid.initialize({
                startOnLoad: false,
                theme: 'dark',
                securityLevel: 'strict',
            });
            // Re-query to catch diagrams added while script was loading
            const freshDivs = document.querySelectorAll('div.mermaid:not([data-processed])');
            if (freshDivs.length === 0) return;
            const freshNodes = Array.from(freshDivs);
            window.mermaid.run({ nodes: freshNodes }).then(() => {
                freshNodes.forEach(el => el.setAttribute('data-processed', 'true'));
            }).catch(e => {
                console.warn('Mermaid render error:', e);
            });
        } catch (e) {
            console.warn('Mermaid init error:', e);
        }
    };
    script.onerror = () => {
        window.__mermaidLoaderStatus = 'error';
        console.warn('Failed to load mermaid.js from CDN');
    };
    document.head.appendChild(script);
}

export function injectCodeBlockCopyButtons() {
    document.querySelectorAll('pre:not([data-copy-injected])').forEach(pre => {
        const code = pre.querySelector('code');
        if (!code) return;
        pre.setAttribute('data-copy-injected', 'true');
        pre.style.position = 'relative';
        const btn = document.createElement('button');
        btn.className = 'code-copy-btn';
        btn.setAttribute('aria-label', 'Copy code');
        btn.style.cssText = 'position:absolute;top:8px;right:8px;padding:4px 8px;border-radius:6px;border:1px solid rgba(255,255,255,0.1);background:rgba(0,0,0,0.3);color:rgba(255,255,255,0.7);cursor:pointer;opacity:0;transition:opacity 0.2s;font-size:12px;display:flex;align-items:center;gap:4px;backdrop-filter:blur(4px);z-index:1';
        btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>';
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            navigator.clipboard.writeText(code.innerText).then(() => {
                btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';
                btn.style.color = '#22c55e';
                setTimeout(() => {
                    btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>';
                    btn.style.color = 'rgba(255,255,255,0.7)';
                }, 2000);
            });
        });
        pre.addEventListener('mouseenter', () => btn.style.opacity = '1');
        pre.addEventListener('mouseleave', () => btn.style.opacity = '0');
        pre.appendChild(btn);
    });
}
"#
)]
extern "C" {
    fn initMermaidDiagrams();
    fn injectCodeBlockCopyButtons();
}
/// README viewer with loading/error states
#[component]
pub fn ReadmeViewer(
    #[props(default = None)]
    content: Option<String>,
    #[props(default = false)]
    loading: bool,
    #[props(default = None)]
    error: Option<String>,
    #[props(default = "README.md".to_string())]
    filename: String,
) -> Element {
    // Initialize mermaid.js after the README HTML is rendered into the DOM.
    // Call initMermaidDiagrams whenever content is present; the JS function
    // already no-ops if there are no .mermaid nodes in the DOM.
    use_effect(use_reactive(&content, move |content| {
        if content.is_some() {
            // Small delay to ensure dangerous_inner_html has been applied to the DOM
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(100).await;
                initMermaidDiagrams();
                injectCodeBlockCopyButtons();
            });
        }
    }));

    rsx! {
        div { class: "border border-border rounded-lg overflow-hidden",
            div { class: "flex items-center gap-2 px-4 py-3 bg-muted/50 border-b border-border",
                svg {
                    class: "w-4 h-4 text-muted-foreground",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
                    polyline { points: "14 2 14 8 20 8" }
                }
                span { class: "text-sm font-medium", "{filename}" }
            }
            div { class: "p-6",
                if loading {
                    ReadmeSkeleton {}
                } else if let Some(err) = error {
                    div { class: "text-center py-8 text-muted-foreground",
                        svg {
                            class: "w-12 h-12 mx-auto mb-3 text-muted-foreground/50",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "12", cy: "12", r: "10" }
                            line {
                                x1: "12",
                                y1: "8",
                                x2: "12",
                                y2: "12",
                            }
                            line {
                                x1: "12",
                                y1: "16",
                                x2: "12.01",
                                y2: "16",
                            }
                        }
                        p { class: "text-sm", "{err}" }
                    }
                } else if let Some(markdown) = content {
                    if markdown.is_empty() {
                        div { class: "text-center py-8 text-muted-foreground",
                            svg {
                                class: "w-12 h-12 mx-auto mb-3 text-muted-foreground/50",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
                                polyline { points: "14 2 14 8 20 8" }
                                line {
                                    x1: "9",
                                    y1: "15",
                                    x2: "15",
                                    y2: "15",
                                }
                            }
                            p { class: "text-sm", "README is empty" }
                        }
                    } else {
                        div {
                            class: "prose prose-neutral dark:prose-invert max-w-none prose-headings:font-semibold prose-headings:text-foreground prose-a:text-primary prose-a:no-underline hover:prose-a:underline prose-code:bg-muted prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-code:before:content-[''] prose-code:after:content-[''] prose-pre:bg-muted prose-pre:border prose-pre:border-border prose-img:rounded-lg prose-hr:border-border",
                            dangerous_inner_html: "{render_markdown(&markdown)}",
                        }
                    }
                } else {
                    NoReadme {}
                }
            }
        }
    }
}
/// Skeleton loader for README
#[component]
pub fn ReadmeSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse space-y-4",
            div { class: "h-8 bg-muted rounded w-1/3" }
            div { class: "space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }
            div { class: "h-6 bg-muted rounded w-1/4 mt-6" }
            div { class: "space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
            }
            div { class: "h-24 bg-muted rounded mt-4" }
            div { class: "space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-4 bg-muted rounded w-1/2" }
            }
        }
    }
}
/// Placeholder when no README exists
#[component]
fn NoReadme() -> Element {
    rsx! {
        div { class: "text-center py-12",
            svg {
                class: "w-16 h-16 mx-auto mb-4 text-muted-foreground/30",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
            }
            h3 { class: "text-lg font-medium text-muted-foreground mb-2", "No README found" }
            p { class: "text-sm text-muted-foreground",
                "Add a README file to help others understand this repository."
            }
        }
    }
}
/// Inline README preview (for compact displays)
/// Uses CSS line-clamp for visual truncation to avoid breaking markdown constructs.
/// Mermaid code blocks are replaced with a placeholder since the CDN won't render in previews.
#[component]
pub fn ReadmePreview(content: String) -> Element {
    // Replace mermaid code blocks with a placeholder for preview
    let preview_content = {
        let mut result = String::new();
        let mut in_mermaid = false;
        for line in content.lines() {
            if line.trim() == "```mermaid" {
                in_mermaid = true;
                result.push_str("[Diagram]\n");
                continue;
            }
            if in_mermaid {
                if line.trim().starts_with("```") {
                    in_mermaid = false;
                }
                continue;
            }
            result.push_str(line);
            result.push('\n');
        }
        result
    };
    rsx! {
        div {
            class: "text-sm text-muted-foreground line-clamp-3",
            dangerous_inner_html: "{render_markdown(&preview_content)}",
        }
    }
}
