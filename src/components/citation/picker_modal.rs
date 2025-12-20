//! Citation Picker Modal
//! Select citations to insert into wiki pages and publications

use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::ToBech32;
use crate::stores::citation_store::{
    CachedCitation, USER_CITATIONS,
    fetch_citations_by_author, search_citations,
};
use crate::stores::auth_store;
use crate::utils::nkbip03::CitationStyle;
use super::card::CitationCardCompact;
use crate::components::icons::{XIcon, SearchIcon};

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
    // Task for debounced search - cancels previous search when new input arrives
    let mut search_task: Signal<Option<Task>> = use_signal(|| None);

    // Selected citation
    let mut selected_citation = use_signal(|| None::<CachedCitation>);
    let mut selected_style = use_signal(|| CitationStyle::End);

    // Loading state
    let mut loading = use_signal(|| false);

    let user_pubkey = auth_store::get_pubkey();

    // Load user's citations when modal opens
    use_effect(use_reactive(
        (&*props.show.read(), &user_pubkey),
        move |(is_shown, pubkey)| {
            if is_shown {
                if let Some(pk) = pubkey {
                    loading.set(true);
                    let pk_clone = pk.clone();
                    spawn(async move {
                        if let Err(e) = fetch_citations_by_author(&pk_clone, 100).await {
                            crate::utils::log_fetch_error("citations", e);
                        }
                        loading.set(false);
                    });
                }
                // Reset selection when opening
                selected_citation.set(None);
                selected_style.set(CitationStyle::End);
                search_query.set(String::new());
                search_results.set(Vec::new());
            }
        },
    ));

    // Handle search with debouncing and task cancellation
    let mut handle_search = move |new_query: String| {
        search_query.set(new_query.clone());

        // Cancel any pending search task
        if let Some(task) = search_task.take() {
            task.cancel();
        }

        if new_query.is_empty() {
            search_results.set(Vec::new());
            is_searching.set(false);
            return;
        }

        if let Some(pk) = user_pubkey.clone() {
            is_searching.set(true);

            // Capture query for stale result verification
            let query_snapshot = new_query.clone();

            // Start new debounced search task
            let new_task = spawn(async move {
                // Debounce: wait 300ms before searching
                #[cfg(target_family = "wasm")]
                {
                    gloo_timers::future::TimeoutFuture::new(300).await;
                }
                #[cfg(not(target_family = "wasm"))]
                {
                    use std::time::Duration;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }

                match search_citations(&query_snapshot, &pk, 50).await {
                    Ok(results) => {
                        // Only apply results if query hasn't changed
                        if search_query.read().as_str() == query_snapshot.as_str() {
                            search_results.set(results);
                            is_searching.set(false);
                        }
                    }
                    Err(e) => {
                        if search_query.read().as_str() == query_snapshot.as_str() {
                            log::warn!("Citation search failed: {}", e);
                            is_searching.set(false);
                        }
                    }
                }
            });

            search_task.set(Some(new_task));
        }
    };

    // Get citations to display based on tab
    let citations_to_display = use_memo(move || {
        if *active_tab.read() == PickerTab::Search && !search_query.read().is_empty() {
            search_results.read().clone()
        } else {
            USER_CITATIONS.read().all()
        }
    });

    // Generate markup preview
    let markup_preview = use_memo(move || {
        if let Some(ref citation) = *selected_citation.read() {
            // EventId.to_bech32() returns Result<String, Infallible> - unwrap is safe
            let identifier = citation.naddr.as_ref()
                .cloned()
                .unwrap_or_else(|| citation.event.id.to_bech32().unwrap());
            let style = *selected_style.read();
            format!("{}{}", style.markup_prefix(), identifier)
        } else {
            String::new()
        }
    });

    // Close modal
    let close_modal = move |_| {
        props.show.set(false);
    };

    // Handle citation selection
    let handle_citation_click = move |citation: CachedCitation| {
        selected_citation.set(Some(citation));
    };

    // Handle insert
    let handle_insert = move |_| {
        if let Some(ref citation) = *selected_citation.read() {
            // EventId.to_bech32() returns Result<String, Infallible> - unwrap is safe
            let identifier = citation.naddr.as_ref()
                .cloned()
                .unwrap_or_else(|| citation.event.id.to_bech32().unwrap());
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
        // Backdrop
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: close_modal,

            // Modal content
            div {
                class: "bg-background border border-border rounded-xl shadow-xl w-full max-w-2xl max-h-[85vh] overflow-hidden flex flex-col",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between px-6 py-4 border-b border-border",

                    h2 {
                        class: "text-lg font-semibold",
                        "Insert Citation"
                    }

                    button {
                        class: "p-2 text-muted-foreground hover:text-foreground rounded-lg hover:bg-accent transition-colors",
                        onclick: close_modal,
                        XIcon { class: "w-5 h-5".to_string() }
                    }
                }

                // Tab bar and search
                div {
                    class: "px-6 py-3 border-b border-border space-y-3",

                    // Tabs
                    div {
                        class: "flex gap-2",

                        button {
                            class: if *active_tab.read() == PickerTab::MyCitations {
                                "px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground"
                            } else {
                                "px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent transition-colors"
                            },
                            onclick: move |_| {
                                active_tab.set(PickerTab::MyCitations);
                                search_query.set(String::new());
                            },
                            "My Citations"
                        }

                        button {
                            class: if *active_tab.read() == PickerTab::Search {
                                "px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground"
                            } else {
                                "px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent transition-colors"
                            },
                            onclick: move |_| active_tab.set(PickerTab::Search),
                            "Search"
                        }
                    }

                    // Search input (shown when Search tab is active)
                    if *active_tab.read() == PickerTab::Search {
                        div {
                            class: "relative",
                            div {
                                class: "absolute inset-y-0 left-3 flex items-center pointer-events-none",
                                SearchIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                            }
                            input {
                                r#type: "text",
                                class: "w-full pl-10 pr-4 py-2 bg-muted/50 border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50",
                                placeholder: "Search by title, author...",
                                value: "{search_query}",
                                oninput: move |e| handle_search(e.value()),
                            }
                        }
                    }
                }

                // Content area - two columns on larger screens
                div {
                    class: "flex-1 overflow-hidden flex flex-col md:flex-row",

                    // Citation list
                    div {
                        class: "flex-1 overflow-y-auto p-4 border-b md:border-b-0 md:border-r border-border",

                        if *loading.read() || *is_searching.read() {
                            div {
                                class: "flex items-center justify-center py-8",
                                div {
                                    class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary"
                                }
                            }
                        } else if citations_to_display.read().is_empty() {
                            div {
                                class: "text-center py-8 text-muted-foreground",
                                p { "No citations found" }
                            }
                        } else {
                            div {
                                class: "space-y-2",
                                for citation in citations_to_display.read().iter() {
                                    {
                                        let citation_clone = citation.clone();
                                        let is_selected = selected_citation.read()
                                            .as_ref()
                                            .map(|s| s.event.id == citation.event.id)
                                            .unwrap_or(false);
                                        let selected_class = if is_selected { "ring-2 ring-primary" } else { "" };

                                        rsx! {
                                            div {
                                                key: "{citation.event.id.to_hex()}",
                                                class: "rounded-lg {selected_class}",
                                                CitationCardCompact {
                                                    citation: citation_clone.clone(),
                                                    on_click: handle_citation_click,
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Selection panel
                    div {
                        class: "w-full md:w-72 p-4 bg-muted/30 flex flex-col",

                        if selected_citation.read().is_some() {
                            // Style selector
                            div {
                                class: "space-y-3 mb-4",

                                h3 {
                                    class: "text-sm font-medium",
                                    "Citation Style"
                                }

                                div {
                                    class: "space-y-1",

                                    // Endnote
                                    {render_style_option(CitationStyle::End, "Endnote", "Listed at end of document", selected_style)}

                                    // Footnote
                                    {render_style_option(CitationStyle::Foot, "Footnote", "At bottom of section", selected_style)}

                                    // Inline
                                    {render_style_option(CitationStyle::Inline, "Inline", "(Author, Year) format", selected_style)}

                                    // Quote
                                    {render_style_option(CitationStyle::Quote, "Block Quote", "Quoted with citation", selected_style)}
                                }
                            }

                            // Markup preview
                            div {
                                class: "space-y-2 mb-4",

                                h3 {
                                    class: "text-sm font-medium",
                                    "Markup Preview"
                                }

                                div {
                                    class: "p-2 bg-muted rounded text-xs font-mono break-all",
                                    "{markup_preview}"
                                }
                            }

                            // Insert button
                            button {
                                class: "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors font-medium",
                                onclick: handle_insert,
                                "Insert Citation"
                            }
                        } else {
                            div {
                                class: "flex-1 flex items-center justify-center text-center text-muted-foreground",
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
            class: if is_selected {
                "w-full flex items-center gap-3 p-2 rounded-lg border border-primary bg-primary/10 text-left"
            } else {
                "w-full flex items-center gap-3 p-2 rounded-lg border border-transparent hover:bg-accent text-left transition-colors"
            },
            onclick: move |_| selected.set(style),

            div {
                class: if is_selected {
                    "w-4 h-4 rounded-full border-2 border-primary flex items-center justify-center"
                } else {
                    "w-4 h-4 rounded-full border-2 border-muted-foreground"
                },
                if is_selected {
                    div { class: "w-2 h-2 rounded-full bg-primary" }
                }
            }

            div {
                class: "flex-1",
                p {
                    class: "text-sm font-medium",
                    "{label}"
                }
                p {
                    class: "text-xs text-muted-foreground",
                    "{description}"
                }
            }
        }
    }
}
