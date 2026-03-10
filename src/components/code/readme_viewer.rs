//! README Viewer Component
//!
//! Displays repository README with markdown rendering.
//! Uses pulldown-cmark for parsing and ammonia for sanitization.
//! Styled to match gittr's readme-section.tsx pattern.
//! Supports mermaid diagram rendering via mermaid.js CDN.
use crate::utils::markdown::render_markdown;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

static COUNTER: AtomicU32 = AtomicU32::new(0);

#[cfg(feature = "web")]
#[wasm_bindgen(inline_js = r#"
export function initMermaidDiagrams(rootId) {
    const root = rootId ? document.getElementById(rootId) : document;
    if (!root) return;
    // Find mermaid divs; skip if none present
    const mermaidDivs = root.querySelectorAll('div.mermaid:not([data-processed])');
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
    if (window.__mermaidLoaderStatus === 'loading') {
        if (!window.__mermaidPendingRoots) window.__mermaidPendingRoots = new Set();
        window.__mermaidPendingRoots.add(rootId);
        return;
    }
    if (window.__mermaidLoaderStatus === 'loaded') return;
    window.__mermaidLoaderStatus = 'loading';
    window.__mermaidPendingRoots = new Set();
    window.__mermaidPendingRoots.add(rootId);
    const script = document.createElement('script');
    script.src = 'https://cdn.jsdelivr.net/npm/mermaid@10.9.5/dist/mermaid.min.js';
    script.crossOrigin = 'anonymous';
    script.integrity = 'sha384-enVdc7lTHDGtpROV85t9+VqPC2EyyB0hsRD0MrvQnHUsHmTHIz2D8SPP4EnBkstH';
    script.onload = () => {
        window.__mermaidLoaderStatus = 'loaded';
        try {
            window.mermaid.initialize({
                startOnLoad: false,
                theme: 'dark',
                securityLevel: 'strict',
            });
            // Collect all unique roots: current rootId + any pending roots
            const allRoots = new Set();
            allRoots.add(rootId);
            if (window.__mermaidPendingRoots) {
                for (const pendingId of window.__mermaidPendingRoots) {
                    allRoots.add(pendingId);
                }
            }
            // Query each root for unprocessed mermaid divs, merge into single array
            const allNodes = [];
            for (const rid of allRoots) {
                const r = rid ? document.getElementById(rid) : document;
                if (r) {
                    const divs = r.querySelectorAll('div.mermaid:not([data-processed])');
                    for (const d of divs) allNodes.push(d);
                }
            }
            if (allNodes.length > 0) {
                window.mermaid.run({ nodes: allNodes }).then(() => {
                    allNodes.forEach(el => el.setAttribute('data-processed', 'true'));
                }).catch(e => {
                    console.warn('Mermaid render error:', e);
                });
            }
            if (window.__mermaidPendingRoots) {
                window.__mermaidPendingRoots.clear();
            }
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

export function injectCodeBlockCopyButtons(rootId) {
    const root = rootId ? document.getElementById(rootId) : document;
    if (!root) return;
    root.querySelectorAll('pre:not([data-copy-injected])').forEach(pre => {
        const code = pre.querySelector('code');
        if (!code) return;
        pre.setAttribute('data-copy-injected', 'true');
        pre.style.position = 'relative';
        const btn = document.createElement('button');
        btn.className = 'code-copy-btn';
        btn.setAttribute('aria-label', 'Copy code');
        btn.setAttribute('tabindex', '0');
        btn.style.cssText = 'position:absolute;top:8px;right:8px;padding:4px 8px;border-radius:6px;border:1px solid rgba(255,255,255,0.1);background:rgba(0,0,0,0.3);color:rgba(255,255,255,0.7);cursor:pointer;opacity:0;transition:opacity 0.2s;font-size:12px;display:flex;align-items:center;gap:4px;backdrop-filter:blur(4px);z-index:1';
        btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>';
        const copySvg = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>';
        const successSvg = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';
        const errorSvg = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>';
        const doCopy = (e) => {
            e.stopPropagation();
            (navigator.clipboard ? navigator.clipboard.writeText(code.innerText) : Promise.reject()).then(() => {
                btn.innerHTML = successSvg;
                btn.style.color = '#22c55e';
                setTimeout(() => { btn.innerHTML = copySvg; btn.style.color = 'rgba(255,255,255,0.7)'; }, 2000);
            }).catch(() => {
                btn.innerHTML = errorSvg;
                btn.style.color = '#ef4444';
                setTimeout(() => { btn.innerHTML = copySvg; btn.style.color = 'rgba(255,255,255,0.7)'; }, 2000);
            });
        };
        btn.addEventListener('click', doCopy);
        btn.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); doCopy(e); } });
        pre.addEventListener('mouseenter', () => btn.style.opacity = '1');
        pre.addEventListener('mouseleave', () => btn.style.opacity = '0');
        pre.addEventListener('focusin', () => btn.style.opacity = '1');
        pre.addEventListener('focusout', () => btn.style.opacity = '0');
        pre.appendChild(btn);
    });
}
"#)]
extern "C" {
    fn initMermaidDiagrams(root_id: &str);
    fn injectCodeBlockCopyButtons(root_id: &str);
}
/// README viewer with loading/error states
#[component]
pub fn ReadmeViewer(
    #[props(default = None)] content: Option<String>,
    #[props(default = false)] loading: bool,
    #[props(default = None)] error: Option<String>,
    #[props(default = "README.md".to_string())] filename: String,
) -> Element {
    let viewer_id =
        use_hook(|| format!("readme-viewer-{}", COUNTER.fetch_add(1, Ordering::Relaxed)));
    // Initialize mermaid.js after the README HTML is rendered into the DOM.
    // Call initMermaidDiagrams whenever content is present; the JS function
    // already no-ops if there are no .mermaid nodes in the DOM.
    use_effect(use_reactive((&content, &loading, &error), {
        let viewer_id = viewer_id.clone();
        move |(content, loading, error)| {
            if content.is_some() && !loading && error.is_none() {
                let viewer_id = viewer_id.clone();
                spawn(async move {
                    // Allow DOM to settle before mermaid scans for nodes (initMermaidDiagrams/injectCodeBlockCopyButtons)
                    crate::platform::timer::sleep_ms(100).await;
                    #[cfg(feature = "web")]
                    {
                        initMermaidDiagrams(&viewer_id);
                        injectCodeBlockCopyButtons(&viewer_id);
                    }
                    #[cfg(not(feature = "web"))]
                    {
                        let _ = viewer_id;
                    }
                });
            }
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
                            id: "{viewer_id}",
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
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                let lang = trimmed.trim_start_matches('`').trim().to_lowercase();
                if lang.starts_with("mermaid") {
                    in_mermaid = true;
                    result.push_str("[Diagram]\n");
                    continue;
                }
            }
            if in_mermaid {
                if trimmed.starts_with("```") {
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
