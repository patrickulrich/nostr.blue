//! Publication Table of Contents Component
//! Tree-based navigation for publication sections (NKBIP-01 Kind 30040/30041)

use dioxus::prelude::*;
use std::time::Duration;
use crate::stores::publication_store::{PublicationTree, PublicationNode, PublicationSection, sections_by_addresses_filter, parse_publication_section, SectionReference};
use crate::stores::nostr_client;
use crate::components::icons::{ChevronDownIcon, ArrowLeftIcon};

/// Dynamic Table of Contents that updates based on current navigation level
/// Uses tree's parent relationships instead of manual breadcrumb tracking
#[component]
pub fn PublicationTocDynamic(
    /// The root publication tree
    tree: PublicationTree,
    /// Currently selected section address
    selected: Option<String>,
    /// Current parent section (for showing its children)
    current_parent: Option<PublicationSection>,
    /// Callback when a section is selected
    on_select: EventHandler<String>,
    /// Callback when navigating back up
    on_back: EventHandler<()>,
) -> Element {
    // Determine what items to show in the TOC
    let items_to_show: Vec<String> = if let Some(ref parent) = current_parent {
        // Show children of current parent
        parent.child_addresses.iter().map(|a| a.address.clone()).collect()
    } else {
        // Show root level items
        tree.root.section_addresses.iter().map(|a| a.address.clone()).collect()
    };

    let current_title = current_parent.as_ref()
        .map(|p| p.title.clone())
        .unwrap_or_else(|| tree.root.title.clone());

    let item_count = items_to_show.len();

    // Check if we have a parent to go back to (using tree structure)
    let has_parent = current_parent.is_some();

    rsx! {
        nav {
            class: "publication-toc",

            // Breadcrumb navigation
            if has_parent {
                div {
                    class: "px-3 py-2 border-b border-border",
                    button {
                        class: "flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors w-full",
                        onclick: move |_| on_back.call(()),
                        ArrowLeftIcon { class: "w-4 h-4" }
                        span { class: "truncate", "Back" }
                    }
                }
            }

            // Current level header
            div {
                class: "px-3 py-2 border-b border-border",
                h2 {
                    class: "font-semibold text-sm text-foreground truncate",
                    "{current_title}"
                }
                p {
                    class: "text-xs text-muted-foreground mt-0.5",
                    "{item_count} items"
                }
            }

            // Items at current level - key forces re-creation when addresses change
            div {
                class: "py-2 overflow-y-auto max-h-[calc(100vh-14rem)]",
                DynamicTocList {
                    key: "{current_title}",
                    addresses: items_to_show.clone(),
                    tree: tree.clone(),
                    selected: selected.clone(),
                    on_select: on_select,
                }
            }
        }
    }
}

/// List component for dynamic TOC items
#[component]
fn DynamicTocList(
    addresses: Vec<String>,
    tree: PublicationTree,
    selected: Option<String>,
    on_select: EventHandler<String>,
) -> Element {
    // Fetch sections for the addresses - use memo to track address changes
    let mut sections = use_signal(Vec::<PublicationSection>::new);
    let mut loading = use_signal(|| true);

    // Create a key from addresses to detect changes
    let addresses_key = addresses.join(",");
    let mut last_key = use_signal(String::new);

    // Check if addresses changed and fetch if needed
    if addresses_key != *last_key.read() {
        last_key.set(addresses_key.clone());
        let addrs = addresses.clone();

        spawn(async move {
            loading.set(true);

            // Build section references for fetching
            let refs: Vec<SectionReference> = addrs.iter()
                .map(|a| SectionReference {
                    address: a.clone(),
                    relay_hint: None,
                    event_id: None,
                })
                .collect();

            let mut found_sections: Vec<PublicationSection> = Vec::new();

            let filters = sections_by_addresses_filter(&refs);
            for filter in filters {
                if let Ok(events) = nostr_client::fetch_events_aggregated(
                    filter,
                    Duration::from_secs(10),
                ).await {
                    for event in events {
                        if let Some(section) = parse_publication_section(&event) {
                            found_sections.push(section);
                        }
                    }
                }
            }

            // Sort by the order in addresses
            let address_order: std::collections::HashMap<_, _> = addrs.iter()
                .enumerate()
                .map(|(i, a)| (a.clone(), i))
                .collect();
            found_sections.sort_by_key(|s| address_order.get(&s.a_tag).copied().unwrap_or(usize::MAX));

            sections.set(found_sections);
            loading.set(false);
        });
    }

    if *loading.read() {
        rsx! {
            div {
                class: "px-2 space-y-1",
                for _ in 0..5 {
                    div {
                        class: "h-8 bg-muted/50 rounded animate-pulse"
                    }
                }
            }
        }
    } else {
        rsx! {
            ul {
                class: "space-y-0.5 px-1",
                for section in sections.read().iter() {
                    DynamicTocItem {
                        key: "{section.a_tag}",
                        section: section.clone(),
                        is_selected: selected.as_ref() == Some(&section.a_tag),
                        on_select: on_select,
                    }
                }
            }
        }
    }
}

/// Single item in the dynamic TOC
#[component]
fn DynamicTocItem(
    section: PublicationSection,
    is_selected: bool,
    on_select: EventHandler<String>,
) -> Element {
    let a_tag = section.a_tag.clone();
    let is_index = section.is_index;
    let child_count = section.child_addresses.len();

    rsx! {
        li {
            button {
                class: format!(
                    "w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-left transition-colors {}",
                    if is_selected { "bg-primary/10 text-primary" } else { "hover:bg-accent text-foreground" }
                ),
                onclick: move |_| on_select.call(a_tag.clone()),

                // Title
                span {
                    class: "flex-1 text-sm truncate",
                    "{section.title}"
                }

                // Indicator for nested indexes
                if is_index && child_count > 0 {
                    span {
                        class: "text-xs text-muted-foreground",
                        "{child_count}"
                    }
                    ChevronDownIcon { class: "w-3.5 h-3.5 -rotate-90 text-muted-foreground" }
                }
            }
        }
    }
}

/// Table of Contents component for publications
#[component]
pub fn PublicationToc(
    /// The publication tree structure
    tree: PublicationTree,
    /// Currently selected section address (a-tag format)
    #[props(default = None)]
    selected: Option<String>,
    /// Callback when a section is selected
    on_select: EventHandler<String>,
) -> Element {
    rsx! {
        nav {
            class: "publication-toc",
            // Publication header
            div {
                class: "px-3 py-2 border-b border-border",
                h2 {
                    class: "font-semibold text-sm text-foreground truncate",
                    "{tree.root.title}"
                }
                if !tree.root.section_addresses.is_empty() {
                    p {
                        class: "text-xs text-muted-foreground mt-0.5",
                        "{tree.root.section_addresses.len()} sections"
                    }
                }
            }

            // Section tree
            div {
                class: "py-2 overflow-y-auto max-h-[calc(100vh-12rem)]",
                {
                    // Get root children from section_addresses
                    let root_children: Vec<String> = tree.root.section_addresses.iter()
                        .map(|s| s.address.clone())
                        .collect();
                    rsx! {
                        TocNodeList {
                            nodes: root_children,
                            tree: tree.clone(),
                            selected: selected.clone(),
                            on_select: on_select,
                            depth: 0,
                        }
                    }
                }
            }
        }
    }
}

/// Recursive node list component
#[component]
fn TocNodeList(
    nodes: Vec<String>,
    tree: PublicationTree,
    selected: Option<String>,
    on_select: EventHandler<String>,
    depth: usize,
) -> Element {
    rsx! {
        ul {
            class: "space-y-0.5",
            style: "padding-left: {depth * 12}px",
            for address in nodes.iter() {
                if let Some(node) = tree.nodes.get(address) {
                    TocNode {
                        key: "{address}",
                        address: address.clone(),
                        node: node.clone(),
                        tree: tree.clone(),
                        selected: selected.clone(),
                        on_select: on_select,
                        depth: depth,
                    }
                }
            }
        }
    }
}

/// Single TOC node component
#[component]
fn TocNode(
    address: String,
    node: PublicationNode,
    tree: PublicationTree,
    selected: Option<String>,
    on_select: EventHandler<String>,
    depth: usize,
) -> Element {
    let mut is_expanded = use_signal(|| depth < 2); // Auto-expand first 2 levels
    let is_selected = selected.as_ref() == Some(&address);
    let has_children = !node.children.is_empty();

    // Get section title from tree
    let title = tree.sections.get(&address)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| "Untitled".to_string());

    let addr_for_click = address.clone();

    rsx! {
        li {
            div {
                class: format!(
                    "flex items-center gap-1 px-2 py-1.5 rounded-md cursor-pointer transition-colors {}",
                    if is_selected { "bg-primary/10 text-primary" } else { "hover:bg-accent text-foreground" }
                ),

                // Expand/collapse button for nodes with children
                if has_children {
                    button {
                        class: "p-0.5 rounded hover:bg-accent transition-colors",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            let current = *is_expanded.read();
                            is_expanded.set(!current);
                        },
                        if *is_expanded.read() {
                            ChevronDownIcon { class: "w-3.5 h-3.5" }
                        } else {
                            // Rotate chevron to point right when collapsed
                            ChevronDownIcon { class: "w-3.5 h-3.5 -rotate-90" }
                        }
                    }
                } else {
                    // Spacing placeholder
                    div { class: "w-4" }
                }

                // Section title
                button {
                    class: "flex-1 text-left text-sm truncate",
                    onclick: move |_| on_select.call(addr_for_click.clone()),
                    "{title}"
                }

                // Loading indicator if content not yet loaded
                if !node.resolved {
                    div {
                        class: "w-3 h-3 border-2 border-muted-foreground/30 border-t-muted-foreground rounded-full animate-spin"
                    }
                }
            }

            // Children (if expanded)
            if has_children && *is_expanded.read() {
                TocNodeList {
                    nodes: node.children.clone(),
                    tree: tree.clone(),
                    selected: selected.clone(),
                    on_select: on_select,
                    depth: depth + 1,
                }
            }
        }
    }
}

/// Compact horizontal TOC for mobile/narrow views
#[component]
pub fn PublicationTocHorizontal(
    tree: PublicationTree,
    selected: Option<String>,
    on_select: EventHandler<String>,
) -> Element {
    // Flatten to just top-level sections for horizontal view
    let sections: Vec<_> = tree.root.section_addresses.iter()
        .filter_map(|s| {
            tree.sections.get(&s.address).map(|sec| (s.address.clone(), sec.clone()))
        })
        .collect();

    rsx! {
        div {
            class: "flex gap-2 overflow-x-auto pb-2 scrollbar-thin",
            for (addr, section) in sections.iter() {
                button {
                    key: "{addr}",
                    class: format!(
                        "px-3 py-1.5 text-sm rounded-full whitespace-nowrap transition-colors {}",
                        if selected.as_ref() == Some(addr) {
                            "bg-primary text-primary-foreground"
                        } else {
                            "bg-accent hover:bg-accent/80"
                        }
                    ),
                    onclick: {
                        let addr = addr.clone();
                        move |_| on_select.call(addr.clone())
                    },
                    "{section.title}"
                }
            }
        }
    }
}

/// Progress indicator showing reading position
#[component]
pub fn PublicationProgress(
    tree: PublicationTree,
    current_section: Option<String>,
) -> Element {
    let total = tree.sections.len();

    // Build all section addresses (root + nested children)
    let all_sections: Vec<String> = tree.root.section_addresses.iter()
        .map(|s| s.address.clone())
        .chain(tree.nodes.values().flat_map(|n| n.children.iter().cloned()))
        .collect();

    let current_index = current_section
        .and_then(|addr| all_sections.iter().position(|a| *a == addr))
        .unwrap_or(0);

    let progress = if total > 0 {
        ((current_index + 1) as f64 / total as f64 * 100.0).round() as u32
    } else {
        0
    };

    rsx! {
        div {
            class: "flex items-center gap-2 text-xs text-muted-foreground",
            div {
                class: "flex-1 h-1 bg-muted rounded-full overflow-hidden",
                div {
                    class: "h-full bg-primary transition-all duration-300",
                    style: "width: {progress}%",
                }
            }
            span { "{progress}%" }
        }
    }
}

/// Skeleton loader for TOC
#[component]
pub fn PublicationTocSkeleton() -> Element {
    rsx! {
        nav {
            class: "publication-toc animate-pulse",
            div {
                class: "px-3 py-2 border-b border-border",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/3 mt-1" }
            }
            div {
                class: "py-2 space-y-2 px-2",
                for _ in 0..8 {
                    div {
                        class: "flex items-center gap-2 py-1",
                        div { class: "w-4 h-4 bg-muted rounded" }
                        div { class: "h-3 bg-muted rounded flex-1" }
                    }
                }
            }
        }
    }
}
