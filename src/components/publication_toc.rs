//! Publication Table of Contents Component
//! Tree-based navigation for publication sections (NKBIP-01 Kind 30040/30041)

use dioxus::prelude::*;
use crate::stores::publication_store::{PublicationTree, PublicationNode};
use crate::components::icons::ChevronDownIcon;

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
