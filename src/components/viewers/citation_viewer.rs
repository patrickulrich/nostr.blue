//! Citations Home Page
//!
//! Discovery/management page for NKBIP-03 citations.
//! Shows user's citations grouped by type with search and filtering.
use std::time::Duration;

use crate::components::citation::card::{CitationCard, CitationCardSkeleton};
use crate::components::citation::editor_modal::CitationEditorModal;
use crate::components::icons::{PenSquareIcon, SearchIcon};
use crate::components::ClientInitializing;
use crate::stores::citation_store::{
    cache_citation, fetch_citations_by_author, get_cached_citation, parse_citation_event,
    search_citations, CachedCitation, CitationGroup, USER_CITATIONS,
};
use crate::stores::nostr_client::fetching::{fetch_event_targeted, parse_event_id};
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
/// Tab selection for the Citations page
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CitationsTab {
    All,
    Internal,
    External,
    Hardcopy,
    Prompt,
}
impl CitationsTab {
    fn label(&self) -> &'static str {
        match self {
            CitationsTab::All => "All",
            CitationsTab::Internal => "Nostr",
            CitationsTab::External => "Web",
            CitationsTab::Hardcopy => "Books",
            CitationsTab::Prompt => "AI",
        }
    }
    fn emoji(&self) -> &'static str {
        match self {
            CitationsTab::All => "📚",
            CitationsTab::Internal => "📌",
            CitationsTab::External => "🌐",
            CitationsTab::Hardcopy => "📖",
            CitationsTab::Prompt => "🤖",
        }
    }
    fn count(&self, group: &CitationGroup) -> usize {
        match self {
            CitationsTab::All => group.total_count(),
            CitationsTab::Internal => group.internal.len(),
            CitationsTab::External => group.external.len(),
            CitationsTab::Hardcopy => group.hardcopy.len(),
            CitationsTab::Prompt => group.prompt.len(),
        }
    }
    fn citations(&self, group: &CitationGroup) -> Vec<CachedCitation> {
        match self {
            CitationsTab::All => group.all(),
            CitationsTab::Internal => group.internal.clone(),
            CitationsTab::External => group.external.clone(),
            CitationsTab::Hardcopy => group.hardcopy.clone(),
            CitationsTab::Prompt => group.prompt.clone(),
        }
    }
}
/// Citations home page
#[component]
pub fn CitationsHome() -> Element {
    let mut active_tab = use_signal(|| CitationsTab::All);
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(Vec::<CachedCitation>::new);
    let mut is_searching = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut show_editor_modal = use_signal(|| false);
    let is_authenticated = auth_store::is_authenticated();
    let user_pubkey = auth_store::get_pubkey();
    use_effect(use_reactive(
        (&user_pubkey, &*nostr_client::CLIENT_INITIALIZED.read()),
        move |(pubkey, client_initialized)| {
            if !client_initialized {
                return;
            }
            if let Some(pk) = pubkey {
                loading.set(true);
                error.set(None);
                let pk_clone = pk.clone();
                spawn(async move {
                    match fetch_citations_by_author(&pk_clone, 200).await {
                        Ok(_) => {
                            loading.set(false);
                        }
                        Err(e) => {
                            log::warn!("Failed to fetch citations: {}", e);
                            error.set(Some(e));
                            loading.set(false);
                        }
                    }
                });
            } else {
                loading.set(false);
            }
        },
    ));
    let mut handle_search = move |query: String| {
        search_query.set(query.clone());
        if query.is_empty() {
            search_results.set(Vec::new());
            is_searching.set(false);
            return;
        }
        is_searching.set(true);
        if let Some(pk) = user_pubkey.clone() {
            let query_clone = query.clone();
            spawn(async move {
                match search_citations(&query_clone, &pk, 50).await {
                    Ok(results) => {
                        search_results.set(results);
                    }
                    Err(e) => {
                        log::warn!("Search failed: {}", e);
                    }
                }
                is_searching.set(false);
            });
        }
    };
    let citations_to_display = use_memo(move || {
        if !search_query.read().is_empty() {
            search_results.read().clone()
        } else {
            let group = USER_CITATIONS.read();
            active_tab.read().citations(&group)
        }
    });
    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! {
            div { class: "flex items-center justify-center h-full", ClientInitializing {} }
        };
    }
    rsx! {
        div { class: "flex flex-col h-full",
            header { class: "sticky top-0 z-10 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60 border-b border-border",
                div { class: "px-4 py-4",
                    div { class: "flex items-center justify-between mb-4",
                        h1 { class: "text-2xl font-bold", "Citations" }
                        if is_authenticated {
                            button {
                                class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                                onclick: move |_| show_editor_modal.set(true),
                                PenSquareIcon { class: "w-4 h-4".to_string() }
                                "New Citation"
                            }
                        }
                    }
                    div { class: "relative mb-4",
                        div { class: "absolute inset-y-0 left-3 flex items-center pointer-events-none",
                            SearchIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                        }
                        input {
                            r#type: "text",
                            class: "w-full pl-10 pr-4 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                            placeholder: "Search citations by title, author...",
                            value: "{search_query}",
                            oninput: move |e| handle_search(e.value()),
                        }
                    }
                    div { class: "flex gap-1 overflow-x-auto pb-1",
                        {
                            [
                                CitationsTab::All,
                                CitationsTab::Internal,
                                CitationsTab::External,
                                CitationsTab::Hardcopy,
                                CitationsTab::Prompt,
                            ]
                                .iter()
                                .map(|tab| {
                                    let is_active = *active_tab.read() == *tab;
                                    let count = tab.count(&USER_CITATIONS.read());
                                    let tab_clone = *tab;
                                    rsx! {
                                        button {
                                            key: "{tab.label()}",
                                            class: if is_active { "flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground" } else { "flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground transition-colors" },
                                            onclick: move |_| {
                                                active_tab.set(tab_clone);
                                                search_query.set(String::new());
                                            },
                                            span { "{tab.emoji()}" }
                                            span { "{tab.label()}" }
                                            if count > 0 {
                                                span { class: "ml-1 px-1.5 py-0.5 text-xs rounded-full bg-muted text-muted-foreground",
                                                    "{count}"
                                                }
                                            }
                                        }
                                    }
                                })
                        }
                    }
                }
            }
            main { class: "flex-1 overflow-y-auto p-4",
                if !is_authenticated {
                    div { class: "flex flex-col items-center justify-center py-20 text-center",
                        div { class: "text-6xl mb-4", "📚" }
                        h2 { class: "text-xl font-semibold mb-2", "Sign in to manage citations" }
                        p { class: "text-muted-foreground max-w-md",
                            "Create and manage your citation library for wiki pages, publications, and research."
                        }
                    }
                } else if *loading.read() {
                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                        for _ in 0..6 {
                            CitationCardSkeleton {}
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "flex flex-col items-center justify-center py-20 text-center",
                        div { class: "text-6xl mb-4", "⚠️" }
                        h2 { class: "text-xl font-semibold mb-2", "Failed to load citations" }
                        p { class: "text-muted-foreground", "{err}" }
                    }
                } else if citations_to_display.read().is_empty() {
                    div { class: "flex flex-col items-center justify-center py-20 text-center",
                        div { class: "text-6xl mb-4",
                            if !search_query.read().is_empty() {
                                "🔍"
                            } else {
                                match *active_tab.read() {
                                    CitationsTab::All => "📚",
                                    CitationsTab::Internal => "📌",
                                    CitationsTab::External => "🌐",
                                    CitationsTab::Hardcopy => "📖",
                                    CitationsTab::Prompt => "🤖",
                                }
                            }
                        }
                        h2 { class: "text-xl font-semibold mb-2",
                            if !search_query.read().is_empty() {
                                "No citations found"
                            } else {
                                "No citations yet"
                            }
                        }
                        p { class: "text-muted-foreground max-w-md mb-4",
                            if !search_query.read().is_empty() {
                                "Try a different search term."
                            } else {
                                "Create your first citation to reference in wiki pages and publications."
                            }
                        }
                        if search_query.read().is_empty() {
                            button {
                                class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                                onclick: move |_| show_editor_modal.set(true),
                                PenSquareIcon { class: "w-4 h-4".to_string() }
                                "Create Citation"
                            }
                        }
                    }
                } else {
                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                        for citation in citations_to_display.read().iter() {
                            CitationCard {
                                key: "{citation.event.id.to_hex()}",
                                citation: citation.clone(),
                                on_click: move |c: CachedCitation| {
                                    log::info!("Clicked citation: {}", c.citation.short_display());
                                },
                            }
                        }
                    }
                }
            }
            CitationEditorModal { show: show_editor_modal }
        }
    }
}
/// Citation detail page.
///
/// Renders a single NKBIP-03 citation (regular kinds 30-33), which are
/// referenced via `nevent`. Fetches the event by its id, parses it, and renders
/// the full citation card plus the complete cited content.
#[component]
pub fn CitationViewer(naddr: String) -> Element {
    let mut citation = use_signal(|| None::<CachedCitation>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut request_id = use_signal(|| 0u32);

    let navigator = use_navigator();

    let client_init = *nostr_client::CLIENT_INITIALIZED.read();
    use_effect(use_reactive(
        (&naddr, &client_init),
        move |(id, client_initialized)| {
            if !client_initialized {
                return;
            }
            let current_id = request_id.peek().wrapping_add(1);
            request_id.set(current_id);
            loading.set(true);
            error.set(None);
            citation.set(None);
            spawn(async move {
                let parsed = match parse_event_id(&id) {
                    Some(p) => p,
                    None => {
                        if *request_id.peek() != current_id {
                            return;
                        }
                        error.set(Some("Invalid citation identifier".to_string()));
                        loading.set(false);
                        return;
                    }
                };
                let event_id_hex = parsed.event_id.to_hex();

                // Cache-first.
                if let Some(cached) = get_cached_citation(&event_id_hex) {
                    if *request_id.peek() != current_id {
                        return;
                    }
                    citation.set(Some(cached));
                    loading.set(false);
                    return;
                }

                match fetch_event_targeted(parsed, Duration::from_secs(12)).await {
                    Ok(Some(event)) => {
                        let kind = event.kind.as_u16();
                        if !matches!(kind, 30..=33) {
                            if *request_id.peek() != current_id {
                                return;
                            }
                            error.set(Some(format!("Not a citation (kind {})", kind)));
                            loading.set(false);
                            return;
                        }
                        if let Some(c) = parse_citation_event(&event) {
                            cache_citation(c.clone());
                            if *request_id.peek() != current_id {
                                return;
                            }
                            citation.set(Some(c));
                        } else if *request_id.peek() == current_id {
                            error.set(Some("Failed to parse citation".to_string()));
                        }
                        if *request_id.peek() == current_id {
                            loading.set(false);
                        }
                    }
                    Ok(None) => {
                        if *request_id.peek() != current_id {
                            return;
                        }
                        error.set(Some("Citation not found".to_string()));
                        loading.set(false);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch citation: {}", e);
                        if *request_id.peek() != current_id {
                            return;
                        }
                        error.set(Some(e));
                        loading.set(false);
                    }
                }
            });
        },
    ));

    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! { ClientInitializing {} };
    }

    let loading_val = *loading.read();
    let error_val = error.read().clone();
    let citation_data = citation.read().clone();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center gap-4 px-4 py-3",
                    button {
                        class: "p-2 hover:bg-accent rounded-lg transition",
                        onclick: move |_| navigator.go_back(),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                    }
                    h1 { class: "text-xl font-bold", "Citation" }
                }
            }

            if loading_val {
                div { class: "p-4 max-w-3xl mx-auto",
                    CitationCardSkeleton {}
                }
            } else if let Some(err) = error_val {
                div { class: "p-4",
                    div { class: "rounded-lg border border-border p-6 text-center",
                        p { class: "text-muted-foreground", "{err}" }
                    }
                }
            } else if let Some(c) = citation_data.as_ref() {
                div { class: "p-4 space-y-4 max-w-3xl mx-auto",
                    CitationCard { citation: c.clone() }

                    // Full cited content (the card only previews the first 120 chars).
                    {
                        let full_content = c.citation.base().content.clone();
                        rsx! {
                            div { class: "rounded-lg border border-border p-4",
                                h2 { class: "text-sm font-semibold uppercase tracking-wide text-muted-foreground mb-2",
                                    "Cited Content"
                                }
                                if full_content.is_empty() {
                                    p { class: "text-sm text-muted-foreground italic", "No content" }
                                } else {
                                    p { class: "text-sm whitespace-pre-wrap", "{full_content}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
