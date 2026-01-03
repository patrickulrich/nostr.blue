//! Publication Detail Route
//! View NKBIP-01 publication with TOC (Kind 30040/30041)

use dioxus::prelude::*;
use crate::components::{
    PublicationToc, PublicationTocSkeleton, PublicationProgress, PublicationTocHorizontal,
    PublicationSectionContent, PublicationSectionSkeleton,
    SectionMetadata, SectionNavigation, SectionOutline, CitationMetadata,
};
use crate::components::icons::{ArrowLeftIcon, ShareIcon, BookmarkIcon, BookOpenIcon, Link2Icon, CopyIcon, CheckIcon};
use crate::utils::clipboard::copy_formatted_content;
use crate::stores::publication_store::{self, PublicationTree};
use crate::stores::{auth_store, nostr_client};
use crate::routes::Route;
use crate::utils::nkbip08::extract_book_wikilinks;

/// Publication detail view with TOC navigation
#[component]
pub fn PublicationDetail(naddr: String) -> Element {
    let nav = use_navigator();
    let mut loading = use_signal(|| true);
    let mut tree = use_signal(|| None::<PublicationTree>);
    let mut selected_section = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut copied = use_signal(|| false);
    let mut citation_count = use_signal(|| 0usize);

    let auth = auth_store::AUTH_STATE.read();
    let _is_logged_in = auth.pubkey.is_some();

    // Fetch publication tree
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let addr = naddr.clone();
        spawn(async move {
            loading.set(true);
            match publication_store::fetch_publication_tree(&addr).await {
                Ok(t) => {
                    // Select first section by default
                    if !t.root.section_addresses.is_empty() {
                        selected_section.set(Some(t.root.section_addresses[0].address.clone()));
                    }
                    tree.set(Some(t));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    let go_back = move |_| {
        nav.push(Route::PublicationsHome {});
    };

    let handle_section_select = move |address: String| {
        selected_section.set(Some(address));
    };

    // Get current section content
    let current_section = use_memo(move || {
        let sel = selected_section.read().clone();
        sel.and_then(|addr| {
            tree.read().as_ref().and_then(|t| t.sections.get(&addr).cloned())
        })
    });

    // Compute prev/next sections for navigation
    let nav_sections = use_memo(move || {
        let sel = selected_section.read().clone();
        let tree_opt = tree.read().clone();

        if let (Some(addr), Some(ref t)) = (sel, tree_opt) {
            let addresses: Vec<_> = t.root.section_addresses.iter()
                .map(|s| s.address.clone())
                .collect();

            if let Some(current_idx) = addresses.iter().position(|a| a == &addr) {
                let prev = if current_idx > 0 {
                    let prev_addr = &addresses[current_idx - 1];
                    t.sections.get(prev_addr).map(|s| (prev_addr.clone(), s.title.clone()))
                } else {
                    None
                };

                let next = if current_idx < addresses.len() - 1 {
                    let next_addr = &addresses[current_idx + 1];
                    t.sections.get(next_addr).map(|s| (next_addr.clone(), s.title.clone()))
                } else {
                    None
                };

                return (prev, next);
            }
        }
        (None, None)
    });

    rsx! {
        div {
            class: "h-[calc(100vh-4rem)] flex flex-col",

            // Top navigation bar
            div {
                class: "flex-shrink-0 flex items-center justify-between px-4 py-3 border-b border-border bg-background",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Publications"
                }

                // Publication title (if loaded)
                if let Some(ref pub_tree) = *tree.read() {
                    h1 {
                        class: "text-lg font-semibold text-foreground truncate max-w-md hidden md:block",
                        "{pub_tree.root.title}"
                    }
                }

                div {
                    class: "flex items-center gap-2",

                    // Citation count badge
                    {
                        let count = *citation_count.read();
                        let suffix = if count > 1 { "s" } else { "" };
                        if count > 0 {
                            rsx! {
                                span {
                                    class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs bg-purple-500/20 text-purple-600 dark:text-purple-400 rounded-full whitespace-nowrap",
                                    "📚 {count} citation{suffix}"
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }

                    // Copy button - copies current section content
                    {
                        let section_content = current_section.read().as_ref().map(|s| s.content.clone());
                        let has_content = section_content.is_some();
                        rsx! {
                            button {
                                class: "flex items-center gap-1.5 px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition-colors disabled:opacity-50",
                                disabled: !has_content,
                                title: "Copy section content",
                                onclick: move |_| {
                                    if let Some(content) = current_section.read().as_ref().map(|s| s.content.clone()) {
                                        spawn(async move {
                                            match copy_formatted_content(&content).await {
                                                Ok(_) => {
                                                    copied.set(true);
                                                    spawn(async move {
                                                        gloo_timers::future::TimeoutFuture::new(2000).await;
                                                        copied.set(false);
                                                    });
                                                }
                                                Err(e) => log::error!("Failed to copy: {:?}", e),
                                            }
                                        });
                                    }
                                },
                                if *copied.read() {
                                    CheckIcon { class: "w-4 h-4 text-green-500" }
                                    "Copied!"
                                } else {
                                    CopyIcon { class: "w-4 h-4" }
                                    "Copy"
                                }
                            }
                        }
                    }

                    button {
                        class: "p-2 rounded-lg hover:bg-accent transition-colors",
                        title: "Bookmark",
                        BookmarkIcon { class: "w-5 h-5" }
                    }
                    button {
                        class: "p-2 rounded-lg hover:bg-accent transition-colors",
                        title: "Share",
                        ShareIcon { class: "w-5 h-5" }
                    }
                }
            }

            // Error state
            if let Some(ref e) = *error.read() {
                div {
                    class: "flex-1 flex items-center justify-center",
                    div {
                        class: "text-center",
                        p { class: "text-destructive mb-4", "Error loading publication: {e}" }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                            onclick: go_back,
                            "Back to Publications"
                        }
                    }
                }
            }

            // Main content area
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && tree.read().is_none()) {
                div {
                    class: "flex-1 flex",
                    // TOC skeleton
                    aside {
                        class: "hidden lg:block w-64 flex-shrink-0 border-r border-border overflow-y-auto",
                        PublicationTocSkeleton {}
                    }
                    // Content skeleton
                    main {
                        class: "flex-1 overflow-y-auto p-6",
                        PublicationSectionSkeleton {}
                    }
                }
            } else if let Some(ref pub_tree) = *tree.read() {
                div {
                    class: "flex-1 flex overflow-hidden",

                    // TOC sidebar (desktop)
                    aside {
                        class: "hidden lg:block w-64 flex-shrink-0 border-r border-border overflow-y-auto",
                        PublicationToc {
                            tree: pub_tree.clone(),
                            selected: selected_section.read().clone(),
                            on_select: EventHandler::new(handle_section_select),
                        }
                    }

                    // Main content area with optional right sidebar
                    div {
                        class: "flex-1 flex overflow-hidden",

                        // Center content
                        main {
                            id: "publication-content",
                            class: "flex-1 overflow-y-auto",

                            // Progress indicator
                            div {
                                class: "sticky top-0 z-10 bg-background px-6 py-2 border-b border-border",
                                PublicationProgress {
                                    tree: pub_tree.clone(),
                                    current_section: selected_section.read().clone(),
                                }

                                // Horizontal TOC for mobile (visible only when vertical TOC is hidden)
                                div {
                                    class: "lg:hidden mt-2",
                                    PublicationTocHorizontal {
                                        tree: pub_tree.clone(),
                                        selected: selected_section.read().clone(),
                                        on_select: EventHandler::new(handle_section_select),
                                    }
                                }
                            }

                            // Section content
                            div {
                                class: "max-w-3xl mx-auto px-6 py-8",
                                if let Some(ref section) = *current_section.read() {
                                    // Section metadata (reading time, word count)
                                    div {
                                        class: "mb-6",
                                        SectionMetadata {
                                            section: section.clone(),
                                        }
                                    }

                                    // Section content
                                    PublicationSectionContent {
                                        section: section.clone(),
                                        on_citations_loaded: move |metadata: CitationMetadata| {
                                            citation_count.set(metadata.count);
                                        },
                                    }

                                    // Section navigation (prev/next)
                                    {
                                        let (prev, next) = nav_sections.read().clone();
                                        rsx! {
                                            SectionNavigation {
                                                prev_section: prev,
                                                next_section: next,
                                                on_navigate: handle_section_select,
                                            }
                                        }
                                    }
                                } else {
                                    // Show publication info when no section selected
                                    div {
                                        class: "text-center py-16",
                                        h2 {
                                            class: "text-2xl font-bold text-foreground mb-4",
                                            "{pub_tree.root.title}"
                                        }
                                        if let Some(ref author) = pub_tree.root.author {
                                            p {
                                                class: "text-lg text-muted-foreground mb-4",
                                                "by {author}"
                                            }
                                        }
                                        if let Some(ref summary) = pub_tree.root.summary {
                                            p {
                                                class: "text-muted-foreground max-w-md mx-auto mb-6",
                                                "{summary}"
                                            }
                                        }
                                        p {
                                            class: "text-sm text-muted-foreground",
                                            "{pub_tree.root.section_addresses.len()} sections"
                                        }
                                    }
                                }
                            }
                        }

                        // Right sidebar - "On this page" outline (hidden on smaller screens)
                        if let Some(ref section) = *current_section.read() {
                            {
                                // Extract book references from section content
                                let book_refs = extract_book_wikilinks(&section.content);

                                rsx! {
                                    aside {
                                        class: "hidden xl:block w-56 flex-shrink-0 border-l border-border overflow-y-auto p-4",
                                        div {
                                            class: "sticky top-4 space-y-6",

                                            // Section outline
                                            SectionOutline {
                                                content: section.content.clone(),
                                                on_heading_click: move |id: String| {
                                                    // Scroll to the heading using JavaScript
                                                    let js = format!(
                                                        "document.getElementById('{}')?.scrollIntoView({{ behavior: 'smooth', block: 'start' }})",
                                                        id
                                                    );
                                                    let _ = document::eval(&js);
                                                },
                                            }

                                            // Referenced Books section (if any book references found)
                                            if !book_refs.is_empty() {
                                                div {
                                                    class: "pt-4 border-t border-border",
                                                    h3 {
                                                        class: "text-xs font-medium text-muted-foreground uppercase tracking-wider mb-3 flex items-center gap-2",
                                                        BookOpenIcon { class: "w-3 h-3" }
                                                        "Referenced Books"
                                                    }
                                                    ul {
                                                        class: "space-y-2",
                                                        for book_ref in book_refs.iter() {
                                                            li {
                                                                key: "{book_ref.raw}",
                                                                Link {
                                                                    class: "text-sm text-muted-foreground hover:text-foreground transition-colors flex items-start gap-2",
                                                                    to: Route::PublicationSearch { query: book_ref.to_query_string() },
                                                                    Link2Icon { class: "w-3 h-3 mt-1 flex-shrink-0" }
                                                                    span {
                                                                        "{book_ref.display_text()}"
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
            } else {
                // Not found
                div {
                    class: "flex-1 flex items-center justify-center",
                    div {
                        class: "text-center",
                        BookOpenIcon { class: "w-16 h-16 text-muted-foreground mx-auto mb-4" }
                        h2 {
                            class: "text-xl font-semibold text-foreground mb-2",
                            "Publication Not Found"
                        }
                        p {
                            class: "text-muted-foreground mb-6",
                            "This publication doesn't exist or couldn't be loaded."
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                            onclick: go_back,
                            "Back to Publications"
                        }
                    }
                }
            }
        }
    }
}
