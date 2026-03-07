//! Code Repository Architecture Visualization
//!
//! Fetches all file paths from the repository and generates a Mermaid
//! flowchart showing the project structure categorized by role.
use crate::components::code::RepoTabNav;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, file_fetcher, git_service};
use crate::stores::nostr_client;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
use std::collections::HashSet;
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "web")]
#[wasm_bindgen(inline_js = r#"
export function initArchitectureMermaid() {
    let retries = 0;
    function tryInit() {
        const divs = document.querySelectorAll('div.mermaid:not([data-processed])');
        if (divs.length === 0) {
            if (++retries < 30) requestAnimationFrame(tryInit);
            return;
        }
        if (typeof mermaid === 'undefined') {
            const script = document.createElement('script');
            script.src = 'https://cdn.jsdelivr.net/npm/mermaid@10.9.5/dist/mermaid.min.js';
            script.crossOrigin = 'anonymous';
            script.integrity = 'sha384-enVdc7lTHDGtpROV85t9+VqPC2EyyB0hsRD0MrvQnHUsHmTHIz2D8SPP4EnBkstH';
            script.onload = () => {
                mermaid.initialize({ startOnLoad: false, theme: 'dark' });
                mermaid.run({ nodes: divs });
                divs.forEach(d => d.setAttribute('data-processed', 'true'));
            };
            document.head.appendChild(script);
        } else {
            mermaid.run({ nodes: divs });
            divs.forEach(d => d.setAttribute('data-processed', 'true'));
        }
    }
    requestAnimationFrame(tryInit);
}
"#)]
extern "C" {
    fn initArchitectureMermaid();
}

/// Classify a file path into an architecture category.
fn classify_path(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    // Routes / pages
    if lower.contains("/routes/")
        || lower.contains("/pages/")
        || lower.contains("/app/")
        || lower.starts_with("routes/")
        || lower.starts_with("pages/")
        || lower.starts_with("app/")
    {
        return "Routes";
    }
    // Components
    if lower.contains("/components/") || lower.starts_with("components/") {
        return "Components";
    }
    // Services / API
    if lower.contains("/services/")
        || lower.contains("/api/")
        || lower.starts_with("services/")
        || lower.starts_with("api/")
    {
        return "Services";
    }
    // Utils / helpers
    if lower.contains("/utils/")
        || lower.contains("/helpers/")
        || lower.starts_with("utils/")
        || lower.starts_with("helpers/")
    {
        return "Utils";
    }
    // Tests
    if lower.contains("/tests/")
        || lower.contains("/spec/")
        || lower.contains("/__tests__/")
        || lower.starts_with("tests/")
        || lower.starts_with("spec/")
        || lower.contains("_test.")
        || lower.contains(".test.")
        || lower.contains(".spec.")
    {
        return "Tests";
    }
    // Documentation
    if lower.contains("/docs/") || lower.starts_with("docs/") {
        return "Documentation";
    }
    // Top-level markdown
    if !lower.contains('/') && lower.ends_with(".md") {
        return "Documentation";
    }
    // Config
    if lower == "cargo.toml"
        || lower == "package.json"
        || lower == "tsconfig.json"
        || lower == "dioxus.toml"
        || lower == "tailwind.config.js"
        || lower == "tailwind.config.ts"
        || lower.ends_with(".config.js")
        || lower.ends_with(".config.ts")
        || lower.ends_with(".config.mjs")
        || (lower.ends_with(".toml") && !lower.contains('/'))
        || lower == ".gitignore"
        || lower == ".env"
        || lower == ".env.example"
    {
        return "Config";
    }
    // Assets
    if lower.contains("/public/")
        || lower.contains("/static/")
        || lower.contains("/assets/")
        || lower.starts_with("public/")
        || lower.starts_with("static/")
        || lower.starts_with("assets/")
    {
        return "Assets";
    }
    // Hooks
    if lower.contains("/hooks/") || lower.starts_with("hooks/") {
        return "Hooks";
    }
    // Stores / state
    if lower.contains("/stores/")
        || lower.contains("/store/")
        || lower.starts_with("stores/")
        || lower.starts_with("store/")
    {
        return "Stores";
    }
    "Source"
}

/// Category display info: (label, color for mermaid, tailwind text color class)
fn category_info(cat: &str) -> (&str, &str, &str) {
    match cat {
        "Routes" => ("Routes", "#60a5fa", "text-blue-400"),
        "Components" => ("Components", "#34d399", "text-emerald-400"),
        "Services" => ("Services", "#f472b6", "text-pink-400"),
        "Utils" => ("Utils", "#a78bfa", "text-violet-400"),
        "Hooks" => ("Hooks", "#fb923c", "text-orange-400"),
        "Stores" => ("Stores", "#facc15", "text-yellow-400"),
        "Tests" => ("Tests", "#4ade80", "text-green-400"),
        "Documentation" => ("Docs", "#94a3b8", "text-slate-400"),
        "Config" => ("Config", "#e2e8f0", "text-slate-300"),
        "Assets" => ("Assets", "#22d3ee", "text-cyan-400"),
        _ => ("Source", "#818cf8", "text-indigo-400"),
    }
}

/// Build a Mermaid flowchart string from categorized file counts.
fn build_mermaid(categories: &[(String, usize)], total: usize) -> String {
    let mut lines = Vec::new();
    lines.push("flowchart TD".to_string());
    lines.push(format!("    ROOT[\"Project<br/>{} files\"]", total));

    for (cat, count) in categories {
        let (label, color, _) = category_info(cat);
        let id = cat.replace(' ', "_");
        lines.push(format!("    {}[\"{}<br/>{} files\"]", id, label, count));
        lines.push(format!("    ROOT --> {}", id));
        lines.push(format!(
            "    style {} fill:{},stroke:{},color:#111",
            id, color, color
        ));
    }

    lines.push("    style ROOT fill:#1e293b,stroke:#475569,color:#e2e8f0".to_string());
    lines.join("\n")
}

#[component]
pub fn CodeRepoArchitecture(naddr: String) -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut file_paths = use_signal(Vec::<String>::new);
    let mut repo_signal = use_signal(|| None::<Repository>);
    let mut gen = use_signal(|| 0u32);
    let mut open_categories = use_signal(HashSet::<String>::new);

    use_effect(use_reactive(&naddr, move |naddr| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let current_gen = gen.peek().wrapping_add(1);
        gen.set(current_gen);
        loading.set(true);
        error.set(None);
        repo_signal.set(None);
        file_paths.set(Vec::new());
        spawn(async move {
            let repo = match fetch_repository(&naddr).await {
                Ok(r) => r,
                Err(e) => {
                    if *gen.peek() != current_gen {
                        return;
                    }
                    error.set(Some(format!("Repository not found: {}", e)));
                    loading.set(false);
                    return;
                }
            };
            if *gen.peek() != current_gen {
                return;
            }
            repo_signal.set(Some(repo.clone()));

            // Try REST API first, then fall back to git worker
            let paths = if let Ok(p) = file_fetcher::fetch_all_file_paths(&repo, None).await {
                p
            } else {
                if !git_service::GitService::is_initialized() {
                    if let Err(e) = git_service::GitService::init().await {
                        if *gen.peek() != current_gen {
                            return;
                        }
                        error.set(Some(format!("Failed to initialize git: {}", e)));
                        loading.set(false);
                        return;
                    }
                }
                match git_service::git_service().list_all_files(&repo, None).await {
                    Ok(p) => p,
                    Err(e) => {
                        if *gen.peek() != current_gen {
                            return;
                        }
                        error.set(Some(format!("Failed to list files: {}", e)));
                        loading.set(false);
                        return;
                    }
                }
            };
            if *gen.peek() != current_gen {
                return;
            }
            file_paths.set(paths);
            loading.set(false);
        });
    }));

    let paths = file_paths();
    let total = paths.len();
    let mut cat_groups: std::collections::BTreeMap<String, Vec<&str>> =
        std::collections::BTreeMap::new();
    for p in &paths {
        let cat = classify_path(p);
        cat_groups.entry(cat.to_string()).or_default().push(p);
    }
    let mut sorted_cats: Vec<(String, usize)> = cat_groups
        .iter()
        .map(|(k, v)| (k.clone(), v.len()))
        .collect();
    sorted_cats.sort_by(|a, b| b.1.cmp(&a.1));

    let mermaid_code = if !sorted_cats.is_empty() {
        build_mermaid(&sorted_cats, total)
    } else {
        String::new()
    };

    // Initialize mermaid after DOM update (rAF retry loop handles DOM readiness)
    use_effect(use_reactive(&mermaid_code, move |code| {
        if !code.is_empty() {
            #[cfg(feature = "web")]
            initArchitectureMermaid();
        }
    }));

    let repo_name = repo_signal()
        .map(|r| r.display_name().to_string())
        .unwrap_or_else(|| "Repository".to_string());

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-2",
                    Link {
                        to: Route::CodeRepo { naddr: naddr.clone() },
                        class: "text-blue-400 hover:underline font-medium",
                        "{repo_name}"
                    }
                    span { class: "text-muted-foreground", "/" }
                    span { class: "text-foreground font-medium", "Architecture" }
                }
                div { class: "px-4",
                    RepoTabNav {
                        naddr: naddr.clone(),
                        active_tab: "architecture".to_string(),
                    }
                }
            }

            div { class: "p-4 max-w-6xl mx-auto",
                if loading() {
                    // Skeleton
                    div { class: "space-y-4 animate-pulse",
                        div { class: "h-8 bg-muted rounded w-64" }
                        div { class: "h-64 bg-muted rounded" }
                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-4",
                            for _ in 0..8 {
                                div { class: "h-20 bg-muted rounded" }
                            }
                        }
                    }
                } else if let Some(err) = error() {
                    div { class: "text-center py-12",
                        div { class: "text-red-400 mb-4",
                            svg {
                                class: "w-12 h-12 mx-auto",
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
                                line { x1: "12", y1: "8", x2: "12", y2: "12" }
                                line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                            }
                        }
                        p { class: "text-muted-foreground", "{err}" }
                    }
                } else if sorted_cats.is_empty() {
                    div { class: "text-center py-12 text-muted-foreground",
                        "No files found in repository."
                    }
                } else {
                    // Header
                    h2 { class: "text-xl font-semibold text-foreground mb-1", "Project Architecture" }
                    p { class: "text-muted-foreground text-sm mb-6",
                        "{total} files across {sorted_cats.len()} categories"
                    }

                    // Mermaid diagram
                    div { class: "bg-card border border-border rounded-lg p-6 mb-6 overflow-x-auto",
                        div {
                            class: "mermaid",
                            "{mermaid_code}"
                        }
                    }

                    // Category breakdown cards
                    div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4 mb-6",
                        for (cat , count) in sorted_cats.iter() {
                            {
                                let (label, hex_color, tw_color) = category_info(cat);
                                let pct = if total > 0 { (*count as f64 / total as f64 * 100.0) as u32 } else { 0 };
                                rsx! {
                                    div {
                                        key: "{cat}",
                                        class: "bg-card border border-border rounded-lg p-4",
                                        div { class: "flex items-center justify-between mb-2",
                                            span { class: "text-sm font-medium {tw_color}", "{label}" }
                                            span { class: "text-xs text-muted-foreground", "{pct}%" }
                                        }
                                        div { class: "text-2xl font-bold text-foreground", "{count}" }
                                        div { class: "text-xs text-muted-foreground", "files" }
                                        // Progress bar
                                        div { class: "mt-2 h-1.5 bg-muted rounded-full overflow-hidden",
                                            div {
                                                class: "h-full rounded-full transition-all",
                                                style: "width: {pct}%; background-color: {hex_color};",
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // File listing per category (collapsible)
                    div { class: "space-y-3",
                        h3 { class: "text-lg font-semibold text-foreground", "File Breakdown" }
                        for (cat , _count) in sorted_cats.iter() {
                            {
                                let (label, _, tw_color) = category_info(cat);
                                let cat_files = cat_groups.get(cat).cloned().unwrap_or_default();
                                let is_open = open_categories.read().contains(cat);
                                let cat_key = cat.clone();
                                rsx! {
                                    details {
                                        key: "{cat_key}-details",
                                        class: "bg-card border border-border rounded-lg",
                                        open: is_open,
                                        ontoggle: {
                                            let cat_key = cat_key.clone();
                                            move |_| {
                                                let mut set = open_categories.write();
                                                if set.contains(&cat_key) {
                                                    set.remove(&cat_key);
                                                } else {
                                                    set.insert(cat_key.clone());
                                                }
                                            }
                                        },
                                        summary { class: "px-4 py-3 cursor-pointer select-none hover:bg-accent/50 transition rounded-lg",
                                            span { class: "font-medium {tw_color}", "{label}" }
                                            span { class: "ml-2 text-muted-foreground text-sm", "({cat_files.len()} files)" }
                                        }
                                        if is_open {
                                            div { class: "px-4 pb-3 max-h-64 overflow-y-auto",
                                                for file in cat_files.iter() {
                                                    div {
                                                        key: "{file}",
                                                        class: "py-0.5 text-sm text-muted-foreground font-mono truncate",
                                                        "{file}"
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
