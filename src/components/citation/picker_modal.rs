//! Citation Picker Modal
//! Select citations to insert into wiki pages and publications
use super::card::CitationCardCompact;
use crate::components::icons::{SearchIcon, XIcon};
use crate::stores::auth_store;
use crate::stores::citation_store::{fetch_citations_by_author, CachedCitation, USER_CITATIONS};
use crate::utils::nkbip03::CitationStyle;
use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::ToBech32;
/// Citation selection result
#[derive(Clone, Debug)]
pub struct CitationSelection {
    /// The citation identifier (naddr or nevent)
    pub identifier: String,
    /// Selected citation style
    pub style: CitationStyle,
    /// Generated markup to insert (e.g., "citation::end::naddr1...")
    pub markup: String,
}
/// Tab selection for picker
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum PickerTab {
    #[default]
    MyCitations,
    Search,
}
#[derive(Props, Clone, PartialEq)]
pub struct CitationPickerModalProps {
    /// Signal controlling modal visibility
    pub show: Signal<bool>,
    /// Callback when citation is selected
    pub on_select: EventHandler<CitationSelection>,
}
#[component]
pub fn CitationPickerModal(mut props: CitationPickerModalProps) -> Element {
    let mut active_tab = use_signal(|| PickerTab::MyCitations);
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(Vec::<CachedCitation>::new);
    let mut is_searching = use_signal(|| false);
    let mut search_task: Signal<Option<Task>> = use_signal(|| None);
    let mut selected_citation = use_signal(|| None::<CachedCitation>);
    let mut selected_style = use_signal(|| CitationStyle::End);
    let mut loading = use_signal(|| false);
    let mut load_version = use_signal(|| 0u64);
    let user_pubkey = auth_store::get_pubkey();
    use_effect(use_reactive(
        (&*props.show.read(), &user_pubkey),
        move |(is_shown, pubkey)| {
            if is_shown {
                let version = load_version.with_mut(|v| {
                    *v = v.wrapping_add(1);
                    *v
                });
                if let Some(pk) = pubkey {
                    loading.set(true);
                    let pk_clone = pk.clone();
                    spawn(async move {
                        if let Err(e) = fetch_citations_by_author(&pk_clone, 100).await {
                            crate::utils::log_fetch_error("citations", e);
                        }
                        if *load_version.peek() == version && *props.show.peek() {
                            loading.set(false);
                        }
                    });
                } else {
                    loading.set(false);
                }
                selected_citation.set(None);
                selected_style.set(CitationStyle::End);
                search_query.set(String::new());
                search_results.set(Vec::new());
            } else {
                load_version.with_mut(|v| *v = v.wrapping_add(1));
                // Cleanup on hide - reset all modal state
                if let Some(task) = search_task.take() {
                    task.cancel();
                }
                loading.set(false);
                is_searching.set(false);
                selected_citation.set(None);
                selected_style.set(CitationStyle::End);
                search_query.set(String::new());
                search_results.set(Vec::new());
            }
        },
    ));
    let mut handle_search = move |new_query: String| {
        search_query.set(new_query.clone());
        if let Some(task) = search_task.take() {
            task.cancel();
        }
        if new_query.is_empty() {
            search_results.set(Vec::new());
            is_searching.set(false);
            return;
        }
        is_searching.set(true);
        let query_snapshot = new_query.clone();
        let query_lower = query_snapshot.to_lowercase();
        let new_task = spawn(async move {
            crate::platform::timer::sleep_ms(150).await;
            if search_query.read().as_str() == query_snapshot.as_str() {
                let all_citations = USER_CITATIONS.read().all();
                let filtered: Vec<CachedCitation> = all_citations
                    .into_iter()
                    .filter(|c| {
                        let base = c.citation.base();
                        base.title.to_lowercase().contains(&query_lower)
                            || base.author.to_lowercase().contains(&query_lower)
                            || base.content.to_lowercase().contains(&query_lower)
                    })
                    .take(50)
                    .collect();
                search_results.set(filtered);
                is_searching.set(false);
            }
        });
        search_task.set(Some(new_task));
    };
    let citations_to_display = use_memo(move || {
        if *active_tab.read() == PickerTab::Search && !search_query.read().is_empty() {
            search_results.read().clone()
        } else {
            USER_CITATIONS.read().all()
        }
    });
    let markup_preview = use_memo(move || {
        if let Some(ref citation) = *selected_citation.read() {
            let identifier = citation.naddr.as_ref().cloned().unwrap_or_else(|| {
                citation
                    .event
                    .id
                    .to_bech32()
                    .unwrap_or_else(|_| citation.event.id.to_hex())
            });
            let style = *selected_style.read();
            format!("{}{}", style.markup_prefix(), identifier)
        } else {
            String::new()
        }
    });
    let close_modal = move |_| {
        if let Some(task) = search_task.take() {
            task.cancel();
        }
        is_searching.set(false);
        props.show.set(false);
    };
    let handle_citation_click = move |citation: CachedCitation| {
        selected_citation.set(Some(citation));
    };
    let handle_insert = move |_| {
        if let Some(ref citation) = *selected_citation.read() {
            if let Some(task) = search_task.take() {
                task.cancel();
            }
            is_searching.set(false);
            let identifier = citation.naddr.as_ref().cloned().unwrap_or_else(|| {
                citation
                    .event
                    .id
                    .to_bech32()
                    .unwrap_or_else(|_| citation.event.id.to_hex())
            });
            let style = *selected_style.read();
            let markup = format!("{}{}", style.markup_prefix(), identifier);
            props.on_select.call(CitationSelection {
                identifier,
                style,
                markup,
            });
            props.show.set(false);
        }
    };
    if !*props.show.read() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: close_modal,
            div {
                class: "bg-background border border-border rounded-xl shadow-xl w-full max-w-2xl max-h-[85vh] overflow-hidden flex flex-col",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between px-6 py-4 border-b border-border",
                    h2 { class: "text-lg font-semibold", "Insert Citation" }
                    button {
                        class: "p-2 text-muted-foreground hover:text-foreground rounded-lg hover:bg-accent transition-colors",
                        onclick: close_modal,
                        XIcon { class: "w-5 h-5".to_string() }
                    }
                }
                div { class: "px-6 py-3 border-b border-border space-y-3",
                    div { class: "flex gap-2",
                        button {
                            class: if *active_tab.read() == PickerTab::MyCitations { "px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground" } else { "px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent transition-colors" },
                            onclick: move |_| {
                                active_tab.set(PickerTab::MyCitations);
                                search_query.set(String::new());
                            },
                            "My Citations"
                        }
                        button {
                            class: if *active_tab.read() == PickerTab::Search { "px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground" } else { "px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent transition-colors" },
                            onclick: move |_| active_tab.set(PickerTab::Search),
                            "Search"
                        }
                    }
                    if *active_tab.read() == PickerTab::Search {
                        div { class: "relative",
                            div { class: "absolute inset-y-0 left-3 flex items-center pointer-events-none",
                                SearchIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                            }
                            input {
                                r#type: "text",
                                class: "w-full pl-10 pr-4 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                placeholder: "Search by title, author...",
                                value: "{search_query}",
                                oninput: move |e| handle_search(e.value()),
                            }
                        }
                    }
                }
                div { class: "flex-1 overflow-hidden flex flex-col md:flex-row",
                    div { class: "flex-1 overflow-y-auto p-4 border-b md:border-b-0 md:border-r border-border",
                        if *loading.read() || *is_searching.read() {
                            div { class: "flex items-center justify-center py-8",
                                div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary" }
                            }
                        } else if citations_to_display.read().is_empty() {
                            div { class: "text-center py-8 text-muted-foreground",
                                p { "No citations found" }
                            }
                        } else {
                            div { class: "space-y-2",
                                for citation in citations_to_display.read().iter() {
                                    {
                                        let citation_clone = citation.clone();
                                        let is_selected = selected_citation
                                            .read()
                                            .as_ref()
                                            .map(|s| s.event.id == citation.event.id)
                                            .unwrap_or(false);
                                        let selected_class = if is_selected { "ring-2 ring-primary" } else { "" };
                                        rsx! {
                                            div { key: "{citation.event.id.to_hex()}", class: "rounded-lg {selected_class}",
                                                CitationCardCompact { citation: citation_clone.clone(), on_click: handle_citation_click }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "w-full md:w-72 p-4 bg-muted/30 flex flex-col",
                        if selected_citation.read().is_some() {
                            div { class: "space-y-3 mb-4",
                                h3 { class: "text-sm font-medium", "Citation Style" }
                                div { class: "space-y-1",
                                    {
                                        render_style_option(
                                            CitationStyle::End,
                                            "Endnote",
                                            "Listed at end of document",
                                            selected_style,
                                        )
                                    }
                                    {
                                        render_style_option(
                                            CitationStyle::Foot,
                                            "Footnote",
                                            "At bottom of section",
                                            selected_style,
                                        )
                                    }
                                    {
                                        render_style_option(
                                            CitationStyle::Inline,
                                            "Inline",
                                            "(Author, Year) format",
                                            selected_style,
                                        )
                                    }
                                    {
                                        render_style_option(
                                            CitationStyle::Quote,
                                            "Block Quote",
                                            "Quoted with citation",
                                            selected_style,
                                        )
                                    }
                                }
                            }
                            div { class: "space-y-2 mb-4",
                                h3 { class: "text-sm font-medium", "Markup Preview" }
                                div { class: "p-2 bg-muted rounded text-xs font-mono break-all",
                                    "{markup_preview}"
                                }
                            }
                            button {
                                class: "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors font-medium",
                                onclick: handle_insert,
                                "Insert Citation"
                            }
                        } else {
                            div { class: "flex-1 flex items-center justify-center text-center text-muted-foreground",
                                p { "Select a citation from the list" }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render a style option radio button
fn render_style_option(
    style: CitationStyle,
    label: &str,
    description: &str,
    mut selected: Signal<CitationStyle>,
) -> Element {
    let is_selected = *selected.read() == style;
    rsx! {
        button {
            class: if is_selected { "w-full flex items-center gap-3 p-2 rounded-lg border border-primary bg-primary/10 text-left" } else { "w-full flex items-center gap-3 p-2 rounded-lg border border-transparent hover:bg-accent text-left transition-colors" },
            onclick: move |_| selected.set(style),
            div { class: if is_selected { "w-4 h-4 rounded-full border-2 border-primary flex items-center justify-center" } else { "w-4 h-4 rounded-full border-2 border-muted-foreground" },
                if is_selected {
                    div { class: "w-2 h-2 rounded-full bg-primary" }
                }
            }
            div { class: "flex-1",
                p { class: "text-sm font-medium", "{label}" }
                p { class: "text-xs text-muted-foreground", "{description}" }
            }
        }
    }
}
