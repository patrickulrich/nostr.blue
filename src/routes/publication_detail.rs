//! Publication Detail Route
//! View NKBIP-01 publication with TOC (Kind 30040/30041)
use crate::components::icons::{
    AlertTriangleIcon, ArrowLeftIcon, BookOpenIcon, BookmarkIcon, CheckIcon, CopyIcon,
    Link2Icon, RefreshIcon, ShareIcon,
};
use crate::components::{
    CitationMetadata, PublicationProgress, PublicationSectionContent,
    PublicationSectionSkeleton, PublicationTocDynamic, PublicationTocHorizontal,
    PublicationTocSkeleton, SectionMetadata, SectionNavigation, SectionOutline,
    ShareModal,
};
use crate::routes::Route;
use crate::stores::publication_store::{self, PublicationSection, PublicationTree};
use crate::stores::{auth_store, nostr_client};
use crate::utils::clipboard::copy_formatted_content;
use crate::utils::nkbip08::extract_book_wikilinks;
use dioxus::prelude::*;
use lru::LruCache;
use std::num::NonZeroUsize;
/// Maximum number of dynamically loaded sections to cache
const DYNAMIC_SECTIONS_CACHE_SIZE: usize = 100;
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
    let mut show_share_modal = use_signal(|| false);
    let mut current_toc_parent = use_signal(|| None::<PublicationSection>);
    let mut suppress_auto_parent = use_signal(|| false);
    let auth = auth_store::AUTH_STATE.read();
    let _is_logged_in = auth.pubkey.is_some();
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
                    if !t.root.section_addresses.is_empty() {
                        selected_section
                            .set(Some(t.root.section_addresses[0].address.clone()));
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
    let mut handle_section_select = move |address: String| {
        let section_opt = tree
            .read()
            .as_ref()
            .and_then(|t| t.sections.get(&address).cloned());
        if let Some(section) = section_opt {
            if section.is_index && !section.child_addresses.is_empty() {
                current_toc_parent.set(Some(section));
            }
        }
        selected_section.set(Some(address));
    };
    let mut dynamic_sections = use_signal(|| {
        LruCache::<
            String,
            publication_store::PublicationSection,
        >::new(NonZeroUsize::new(DYNAMIC_SECTIONS_CACHE_SIZE).unwrap())
    });
    use_drop(move || {
        dynamic_sections.write().clear();
    });
    let mut section_load_errors = use_signal(
        std::collections::HashMap::<String, String>::new,
    );
    let mut section_loading = use_signal(std::collections::HashSet::<String>::new);
    let current_section = use_memo(move || {
        let sel = selected_section.read().clone();
        sel.and_then(|addr| {
            if let Some(section) = tree
                .read()
                .as_ref()
                .and_then(|t| t.sections.get(&addr).cloned())
            {
                return Some(section);
            }
            dynamic_sections.read().peek(&addr).cloned()
        })
    });
    let mut retry_section_load = move |addr: String| {
        section_load_errors.write().remove(&addr);
        dynamic_sections.write().pop(&addr);
    };
    use_effect(move || {
        let sel = selected_section.read().clone();
        if let Some(addr) = sel {
            let in_tree = tree
                .read()
                .as_ref()
                .map(|t| t.sections.contains_key(&addr))
                .unwrap_or(false);
            let in_dynamic = dynamic_sections.read().contains(&addr);
            let is_loading = section_loading.read().contains(&addr);
            if !in_tree && !in_dynamic && !is_loading {
                let addr_for_log = addr.clone();
                let addr_for_check = addr.clone();
                let addr_for_error = addr.clone();
                let addr_for_loading = addr.clone();
                section_loading.write().insert(addr_for_loading.clone());
                section_load_errors.write().remove(&addr_for_error);
                spawn(async move {
                    let parts: Vec<&str> = addr.split(':').collect();
                    if parts.len() >= 3 {
                        let kind_result = parts[0].parse::<u16>();
                        let pubkey_result = nostr_sdk::prelude::PublicKey::from_hex(
                            parts[1],
                        );
                        match (kind_result, pubkey_result) {
                            (Ok(kind), Ok(pubkey)) => {
                                let d_tag = parts[2..].join(":");
                                let filter = nostr_sdk::prelude::Filter::new()
                                    .kind(nostr_sdk::prelude::Kind::Custom(kind))
                                    .author(pubkey)
                                    .identifier(&d_tag);
                                let start = instant::Instant::now();
                                match nostr_client::fetch_events_aggregated(
                                        filter,
                                        std::time::Duration::from_secs(10),
                                    )
                                    .await
                                {
                                    Ok(events) => {
                                        let current_sel = selected_section.read().clone();
                                        if current_sel != Some(addr_for_check.clone()) {
                                            log::debug!(
                                                "Dropping stale section fetch result for addr={} (selection changed)",
                                                addr_for_log
                                            );
                                            section_loading.write().remove(&addr_for_loading);
                                            return;
                                        }
                                        if dynamic_sections.read().contains(&addr_for_check) {
                                            section_loading.write().remove(&addr_for_loading);
                                            return;
                                        }
                                        if let Some(event) = events.first() {
                                            if let Some(section) = publication_store::parse_publication_section(
                                                event,
                                            ) {
                                                dynamic_sections.write().put(addr.clone(), section);
                                                section_load_errors.write().remove(&addr_for_error);
                                            } else {
                                                log::warn!(
                                                    "Failed to parse publication section for addr={}",
                                                    addr_for_log
                                                );
                                                section_load_errors
                                                    .write()
                                                    .insert(
                                                        addr_for_error.clone(),
                                                        "Failed to parse section content".to_string(),
                                                    );
                                            }
                                        } else {
                                            log::warn!(
                                                "No events found for section addr={} (took {:?})",
                                                addr_for_log, start.elapsed()
                                            );
                                            section_load_errors
                                                .write()
                                                .insert(
                                                    addr_for_error.clone(),
                                                    "Section not found".to_string(),
                                                );
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "Failed to fetch section addr={}: {} (took {:?})",
                                            addr_for_log, e, start.elapsed()
                                        );
                                        section_load_errors
                                            .write()
                                            .insert(
                                                addr_for_error.clone(),
                                                format!("Network error: {}", e),
                                            );
                                    }
                                }
                            }
                            (Err(e), _) => {
                                log::warn!(
                                    "Failed to parse kind from addr={}: {}", addr_for_log, e
                                );
                                section_load_errors
                                    .write()
                                    .insert(
                                        addr_for_error.clone(),
                                        format!("Invalid section address: {}", e),
                                    );
                            }
                            (_, Err(e)) => {
                                log::warn!(
                                    "Failed to parse pubkey from addr={}: {}", addr_for_log, e
                                );
                                section_load_errors
                                    .write()
                                    .insert(
                                        addr_for_error.clone(),
                                        format!("Invalid author key: {}", e),
                                    );
                            }
                        }
                    } else {
                        log::warn!(
                            "Invalid address format (expected kind:pubkey:d-tag): {}",
                            addr_for_log
                        );
                        section_load_errors
                            .write()
                            .insert(
                                addr_for_error.clone(),
                                "Invalid address format".to_string(),
                            );
                    }
                    section_loading.write().remove(&addr_for_loading);
                });
            }
        }
    });
    use_effect(move || {
        if *suppress_auto_parent.read() {
            suppress_auto_parent.set(false);
            return;
        }
        if let Some(ref section) = *current_section.read() {
            let is_root = tree
                .read()
                .as_ref()
                .map(|t| t.root.a_tag == section.a_tag)
                .unwrap_or(false);
            if is_root {
                return;
            }
            let in_tree = tree
                .read()
                .as_ref()
                .map(|t| t.sections.contains_key(&section.a_tag))
                .unwrap_or(false);
            if !in_tree && section.is_index && !section.child_addresses.is_empty() {
                let current_parent = current_toc_parent.read().clone();
                let should_update = current_parent
                    .as_ref()
                    .map(|p| p.a_tag != section.a_tag)
                    .unwrap_or(true);
                if should_update {
                    current_toc_parent.set(Some(section.clone()));
                }
            }
        }
    });
    let handle_toc_back = move |_| {
        let current_parent = current_toc_parent.read().clone();
        if let Some(parent) = current_parent {
            if let Some(ref t) = *tree.read() {
                let parent_a_tag_lower = parent.a_tag.to_lowercase();
                let is_direct_child_of_root = t
                    .root
                    .section_addresses
                    .iter()
                    .any(|s| s.address.to_lowercase() == parent_a_tag_lower);
                if is_direct_child_of_root {
                    suppress_auto_parent.set(true);
                    current_toc_parent.set(None);
                } else if let Some(node) = t.nodes.get(&parent.a_tag) {
                    if let Some(ref parent_addr) = node.parent {
                        if *parent_addr == t.root.a_tag {
                            current_toc_parent.set(None);
                            if !t.root.section_addresses.is_empty() {
                                selected_section
                                    .set(Some(t.root.section_addresses[0].address.clone()));
                            }
                        } else {
                            selected_section.set(Some(parent_addr.clone()));
                        }
                    } else {
                        current_toc_parent.set(None);
                        if !t.root.section_addresses.is_empty() {
                            selected_section
                                .set(Some(t.root.section_addresses[0].address.clone()));
                        }
                    }
                } else {
                    current_toc_parent.set(None);
                    if !t.root.section_addresses.is_empty() {
                        selected_section
                            .set(Some(t.root.section_addresses[0].address.clone()));
                    }
                }
            }
        }
    };
    let nav_sections = use_memo(move || {
        let sel = selected_section.read().clone();
        let tree_opt = tree.read().clone();
        if let (Some(addr), Some(ref t)) = (sel, tree_opt) {
            let addresses: Vec<_> = t
                .root
                .section_addresses
                .iter()
                .map(|s| s.address.clone())
                .collect();
            if let Some(current_idx) = addresses.iter().position(|a| a == &addr) {
                let prev = if current_idx > 0 {
                    let prev_addr = &addresses[current_idx - 1];
                    t.sections
                        .get(prev_addr)
                        .map(|s| (prev_addr.clone(), s.title.clone()))
                } else {
                    None
                };
                let next = if current_idx < addresses.len() - 1 {
                    let next_addr = &addresses[current_idx + 1];
                    t.sections
                        .get(next_addr)
                        .map(|s| (next_addr.clone(), s.title.clone()))
                } else {
                    None
                };
                return (prev, next);
            }
        }
        (None, None)
    });
    rsx! {
        div { class: "h-[calc(100vh-4rem)] flex flex-col",
            div { class: "shrink-0 flex items-center justify-between px-4 py-3 border-b border-border bg-background",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Publications"
                }
                if let Some(ref pub_tree) = *tree.read() {
                    h1 { class: "text-lg font-semibold text-foreground truncate max-w-md hidden md:block",
                        "{pub_tree.root.title}"
                    }
                }
                div { class: "flex items-center gap-2",
                    {
                        let count = *citation_count.read();
                        let suffix = if count > 1 { "s" } else { "" };
                        if count > 0 {
                            rsx! {
                                span { class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs bg-purple-500/20 text-purple-600 dark:text-purple-400 rounded-full whitespace-nowrap",
                                    "📚 {count} citation{suffix}"
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                    {
                        let section_content = current_section.read().as_ref().map(|s| s.content.clone());
                        let has_content = section_content.is_some();
                        rsx! {
                            button {
                                class: "flex items-center gap-1.5 px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition-colors disabled:opacity-50",
                                disabled: !has_content,
                                title: "Copy section content",
                                onclick: move |_| {
                                    if let Some(content) = current_section.read().as_ref().map(|s| s.content.clone())
                                    {
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
                        onclick: move |_| show_share_modal.set(true),
                        ShareIcon { class: "w-5 h-5" }
                    }
                }
            }
            if let Some(ref e) = *error.read() {
                div { class: "flex-1 flex items-center justify-center",
                    div { class: "text-center",
                        p { class: "text-destructive mb-4", "Error loading publication: {e}" }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                            onclick: go_back,
                            "Back to Publications"
                        }
                    }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read()
                || (*loading.read() && tree.read().is_none())
            {
                div { class: "flex-1 flex",
                    aside { class: "hidden lg:block w-64 shrink-0 border-r border-border overflow-y-auto scrollbar-hide",
                        PublicationTocSkeleton {}
                    }
                    main { class: "flex-1 overflow-y-auto scrollbar-hide p-6",
                        PublicationSectionSkeleton {}
                    }
                }
            } else if let Some(ref pub_tree) = *tree.read() {
                div { class: "flex-1 flex overflow-hidden",
                    aside { class: "hidden lg:block w-64 shrink-0 border-r border-border overflow-y-auto scrollbar-hide",
                        PublicationTocDynamic {
                            tree: pub_tree.clone(),
                            selected: selected_section.read().clone(),
                            current_parent: current_toc_parent.read().clone(),
                            on_select: EventHandler::new(handle_section_select),
                            on_back: EventHandler::new(handle_toc_back),
                        }
                    }
                    div { class: "flex-1 flex overflow-hidden",
                        main {
                            id: "publication-content",
                            class: "flex-1 overflow-y-auto scrollbar-hide",
                            div { class: "sticky top-0 z-10 bg-background px-6 py-2 border-b border-border",
                                PublicationProgress {
                                    tree: pub_tree.clone(),
                                    current_section: selected_section.read().clone(),
                                }
                                div { class: "lg:hidden mt-2",
                                    PublicationTocHorizontal {
                                        tree: pub_tree.clone(),
                                        selected: selected_section.read().clone(),
                                        on_select: EventHandler::new(handle_section_select),
                                    }
                                }
                            }
                            div { class: "max-w-3xl mx-auto px-6 py-8",
                                {
                                    let sel_addr = selected_section.read().clone();
                                    let is_section_loading = sel_addr
                                        .as_ref()
                                        .map(|a| section_loading.read().contains(a))
                                        .unwrap_or(false);
                                    let section_error = sel_addr
                                        .as_ref()
                                        .and_then(|a| section_load_errors.read().get(a).cloned());
                                    if is_section_loading {
                                        rsx! {
                                            div { class: "flex flex-col items-center justify-center py-16",
                                                div { class: "animate-spin w-8 h-8 border-2 border-primary border-t-transparent rounded-full mb-4" }
                                                p { class: "text-muted-foreground", "Loading section..." }
                                            }
                                        }
                                    } else if let Some(error_msg) = section_error {
                                        let retry_addr = sel_addr.clone().unwrap_or_default();
                                        rsx! {
                                            div { class: "flex flex-col items-center justify-center py-16",
                                                div { class: "bg-destructive/10 border border-destructive/20 rounded-lg p-6 max-w-md text-center",
                                                    div { class: "flex items-center justify-center gap-2 text-destructive mb-3",
                                                        AlertTriangleIcon { class: "w-5 h-5" }
                                                        span { class: "font-medium", "Failed to load section" }
                                                    }
                                                    p { class: "text-sm text-muted-foreground mb-4", "{error_msg}" }
                                                    button {
                                                        class: "inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors",
                                                        onclick: move |_| retry_section_load(retry_addr.clone()),
                                                        RefreshIcon { class: "w-4 h-4" }
                                                        "Retry"
                                                    }
                                                }
                                            }
                                        }
                                    } else if let Some(ref section) = *current_section.read() {
                                        rsx! {
                                            div { class: "mb-6",
                                                SectionMetadata { section: section.clone() }
                                            }
                                            PublicationSectionContent {
                                                section: section.clone(),
                                                on_citations_loaded: move |metadata: CitationMetadata| {
                                                    citation_count.set(metadata.count);
                                                },
                                                on_child_select: move |child_address: String| {
                                                    handle_section_select(child_address);
                                                },
                                            }
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
                                        }
                                    } else {
                                        rsx! {
                                            div { class: "text-center py-16",
                                                h2 { class: "text-2xl font-bold text-foreground mb-4", "{pub_tree.root.title}" }
                                                if let Some(ref author) = pub_tree.root.author {
                                                    p { class: "text-lg text-muted-foreground mb-4", "by {author}" }
                                                }
                                                if let Some(ref summary) = pub_tree.root.summary {
                                                    p { class: "text-muted-foreground max-w-md mx-auto mb-6", "{summary}" }
                                                }
                                                p { class: "text-sm text-muted-foreground", "{pub_tree.root.section_addresses.len()} sections" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(ref section) = *current_section.read() {
                            {
                                let book_refs = extract_book_wikilinks(&section.content);
                                rsx! {
                                    aside { class: "hidden xl:block w-56 shrink-0 border-l border-border overflow-y-auto scrollbar-hide p-4",
                                        div { class: "sticky top-4 space-y-6",
                                            SectionOutline {
                                                content: section.content.clone(),
                                                on_heading_click: move |id: String| {
                                                    let js = format!(
                                                        "document.getElementById('{}')?.scrollIntoView({{ behavior: 'smooth', block: 'start' }})",
                                                        id,
                                                    );
                                                    let _ = document::eval(&js);
                                                },
                                            }
                                            if !book_refs.is_empty() {
                                                div { class: "pt-4 border-t border-border",
                                                    h3 { class: "text-xs font-medium text-muted-foreground uppercase tracking-wider mb-3 flex items-center gap-2",
                                                        BookOpenIcon { class: "w-3 h-3" }
                                                        "Referenced Books"
                                                    }
                                                    ul { class: "space-y-2",
                                                        for book_ref in book_refs.iter() {
                                                            li { key: "{book_ref.raw}",
                                                                Link {
                                                                    class: "text-sm text-muted-foreground hover:text-foreground transition-colors flex items-start gap-2",
                                                                    to: Route::PublicationSearch {
                                                                        query: book_ref.to_query_string(),
                                                                    },
                                                                    Link2Icon { class: "w-3 h-3 mt-1 shrink-0" }
                                                                    span { "{book_ref.display_text()}" }
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
                div { class: "flex-1 flex items-center justify-center",
                    div { class: "text-center",
                        BookOpenIcon { class: "w-16 h-16 text-muted-foreground mx-auto mb-4" }
                        h2 { class: "text-xl font-semibold text-foreground mb-2",
                            "Publication Not Found"
                        }
                        p { class: "text-muted-foreground mb-6",
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
            if *show_share_modal.read() {
                if let Some(ref pub_tree) = *tree.read() {
                    ShareModal {
                        event: pub_tree.root.event.clone(),
                        on_close: move |_| show_share_modal.set(false),
                    }
                }
            }
        }
    }
}
