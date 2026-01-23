//! Repository Tab Navigation Component
//!
//! Displays responsive tab navigation for repository pages: Overview, Code, Issues, PRs, Commits, etc.
//! Desktop: horizontal tab bar. Mobile: overflow dropdown for tabs that don't fit.
//! Styled to match gittr's tab-navigation.tsx pattern.

use dioxus::prelude::*;
use crate::routes::Route;

/// Tab configuration
#[derive(Clone, PartialEq)]
struct TabConfig {
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    route: Route,
    count: Option<u32>,
}

/// Repository tab navigation with responsive overflow
#[component]
pub fn RepoTabNav(
    naddr: String,
    active_tab: String,
    #[props(default = None)] issue_count: Option<u32>,
    #[props(default = None)] pr_count: Option<u32>,
) -> Element {
    let mut show_overflow = use_signal(|| false);

    // Define all tabs
    let tabs = [
        TabConfig {
            id: "overview",
            label: "Overview",
            icon: r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/></svg>"#,
            route: Route::CodeRepo { naddr: naddr.clone() },
            count: None,
        },
        TabConfig {
            id: "code",
            label: "Code",
            icon: r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>"#,
            route: Route::CodeRepoTree {
                naddr: naddr.clone(),
                git_ref: "HEAD".to_string(),
                path: "".to_string(),
            },
            count: None,
        },
        TabConfig {
            id: "issues",
            label: "Issues",
            icon: r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="1"/></svg>"#,
            route: Route::CodeRepoIssues { naddr: naddr.clone() },
            count: issue_count,
        },
        TabConfig {
            id: "pulls",
            label: "Pull Requests",
            icon: r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M13 6h3a2 2 0 0 1 2 2v7"/><line x1="6" y1="9" x2="6" y2="21"/></svg>"#,
            route: Route::CodeRepoPulls { naddr: naddr.clone() },
            count: pr_count,
        },
        TabConfig {
            id: "commits",
            label: "Commits",
            icon: r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><line x1="1.05" y1="12" x2="7" y2="12"/><line x1="17.01" y1="12" x2="22.96" y2="12"/></svg>"#,
            route: Route::CodeRepoCommits { naddr: naddr.clone() },
            count: None,
        },
    ];

    // On desktop, show all tabs; on mobile, show overflow menu for PRs/Commits
    let overflow_tabs = &tabs[3.min(tabs.len())..]; // PRs, Commits, etc.

    rsx! {
        div {
            class: "flex items-center gap-1 border-b border-border",

            // Primary tabs (overflow tabs hidden on mobile, shown in dropdown instead)
            div {
                class: "flex items-center",

                for (index, tab) in tabs.iter().enumerate() {
                    // Hide overflow tabs (index >= 3) on mobile - they appear in dropdown menu
                    div {
                        key: "{tab.id}",
                        class: if index >= 3 { "hidden md:block" } else { "" },
                        TabItem {
                            config: tab.clone(),
                            is_active: active_tab == tab.id,
                        }
                    }
                }
            }

            // Mobile overflow menu (hidden on desktop via CSS in parent)
            if !overflow_tabs.is_empty() {
                div {
                    class: "md:hidden relative ml-auto",

                    button {
                        class: "flex items-center gap-1 px-3 py-2 text-sm text-muted-foreground hover:text-foreground",
                        onclick: move |_| {
                            let current = *show_overflow.read();
                            show_overflow.set(!current);
                        },

                        svg {
                            class: "w-4 h-4",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "12", cy: "12", r: "1" }
                            circle { cx: "19", cy: "12", r: "1" }
                            circle { cx: "5", cy: "12", r: "1" }
                        }
                    }

                    // Overflow dropdown
                    if *show_overflow.read() {
                        div {
                            class: "absolute right-0 top-full mt-1 w-48 bg-background border border-border rounded-lg shadow-lg z-50",
                            onclick: move |_| show_overflow.set(false),

                            for tab in overflow_tabs.iter() {
                                OverflowTabItem {
                                    key: "{tab.id}",
                                    config: tab.clone(),
                                    is_active: active_tab == tab.id,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Individual tab item
#[component]
fn TabItem(config: TabConfig, is_active: bool) -> Element {
    let base_class = if is_active {
        "flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium border-b-2 border-primary text-foreground -mb-px"
    } else {
        "flex items-center gap-1.5 px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground hover:bg-accent/50 transition -mb-px border-b-2 border-transparent"
    };

    rsx! {
        Link {
            to: config.route.clone(),
            class: "{base_class}",

            span {
                class: "w-4 h-4 shrink-0",
                dangerous_inner_html: "{config.icon}"
            }

            span {
                class: "hidden sm:inline",
                "{config.label}"
            }

            // Count badge
            if let Some(count) = config.count {
                if count > 0 {
                    span {
                        class: "ml-1 px-1.5 py-0.5 text-xs rounded-full bg-muted text-muted-foreground",
                        "{count}"
                    }
                }
            }
        }
    }
}

/// Overflow menu tab item
#[component]
fn OverflowTabItem(config: TabConfig, is_active: bool) -> Element {
    let base_class = if is_active {
        "flex items-center gap-2 w-full px-3 py-2 text-sm font-medium text-primary bg-primary/10"
    } else {
        "flex items-center gap-2 w-full px-3 py-2 text-sm text-foreground hover:bg-accent transition"
    };

    rsx! {
        Link {
            to: config.route.clone(),
            class: "{base_class}",

            span {
                class: "w-4 h-4 shrink-0",
                dangerous_inner_html: "{config.icon}"
            }

            span { "{config.label}" }

            // Count badge
            if let Some(count) = config.count {
                if count > 0 {
                    span {
                        class: "ml-auto px-1.5 py-0.5 text-xs rounded-full bg-muted text-muted-foreground",
                        "{count}"
                    }
                }
            }
        }
    }
}

/// Compact tab navigation for nested pages
#[component]
pub fn RepoTabNavCompact(
    naddr: String,
    active_tab: String,
) -> Element {
    let tabs = [
        ("overview", "Overview", Route::CodeRepo { naddr: naddr.clone() }),
        ("code", "Code", Route::CodeRepoTree {
            naddr: naddr.clone(),
            git_ref: "HEAD".to_string(),
            path: "".to_string(),
        }),
        ("issues", "Issues", Route::CodeRepoIssues { naddr: naddr.clone() }),
        ("pulls", "PRs", Route::CodeRepoPulls { naddr: naddr.clone() }),
        ("commits", "Commits", Route::CodeRepoCommits { naddr: naddr.clone() }),
    ];

    rsx! {
        div {
            class: "flex items-center gap-1 text-sm",

            for (id, label, route) in tabs.iter() {
                Link {
                    key: "{id}",
                    to: route.clone(),
                    class: if active_tab == *id {
                        "px-2 py-1 rounded text-primary font-medium"
                    } else {
                        "px-2 py-1 rounded text-muted-foreground hover:text-foreground hover:bg-accent/50 transition"
                    },
                    "{label}"
                }
            }
        }
    }
}
