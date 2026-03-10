//! Contributors List Component
//!
//! Displays repository contributors with role badges (owner, maintainer)
//! and optional aggregate stats (issue count, PR count).
use crate::stores::profiles::PROFILE_CACHE;
use dioxus::prelude::*;
use std::collections::HashSet;

/// Display a list of repository contributors with role badges and optional stats
#[component]
pub fn ContributorsList(
    owner: String,
    maintainers: Vec<String>,
    #[props(default = None)] issue_count: Option<u32>,
    #[props(default = None)] pr_count: Option<u32>,
) -> Element {
    // Deduplicate maintainers while preserving original order, excluding the owner
    let mut seen = HashSet::new();
    let mut unique_maintainers = Vec::new();
    for m in &maintainers {
        if *m != owner && seen.insert(m.clone()) {
            unique_maintainers.push(m);
        }
    }
    let total_count = 1 + unique_maintainers.len();

    rsx! {
        div { class: "space-y-3",
            // Header with contributor count
            div { class: "flex items-center justify-between",
                h3 { class: "text-sm font-semibold text-foreground", "Contributors" }
                span { class: "text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded-full",
                    "{total_count}"
                }
            }
            // Owner
            ContributorRow {
                pk: owner.clone(),
                role: "Owner".to_string(),
                role_class: "bg-purple-500/20 text-purple-400".to_string(),
            }
            // Maintainers (deduplicated, excluding owner)
            for maintainer in unique_maintainers.iter() {
                ContributorRow {
                    key: "{maintainer}",
                    pk: (*maintainer).clone(),
                    role: "Maintainer".to_string(),
                    role_class: "bg-blue-500/20 text-blue-400".to_string(),
                }
            }
            // Aggregate stats summary
            if issue_count.is_some() || pr_count.is_some() {
                div { class: "pt-2 border-t border-border mt-2",
                    div { class: "flex gap-3 text-xs text-muted-foreground",
                        if let Some(ic) = issue_count {
                            span { class: "flex items-center gap-1",
                                span { class: "w-2 h-2 rounded-full bg-green-500 inline-block" }
                                "{ic} issues"
                            }
                        }
                        if let Some(pc) = pr_count {
                            span { class: "flex items-center gap-1",
                                span { class: "w-2 h-2 rounded-full bg-purple-500 inline-block" }
                                "{pc} PRs"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Renders a contributor avatar + display name + role badge row
#[component]
fn ContributorRow(pk: String, role: String, role_class: String) -> Element {
    let profile = PROFILE_CACHE.read().peek(&pk).cloned();
    let picture = profile.as_ref().and_then(|p| p.picture.clone());
    let display_name = profile
        .as_ref()
        .and_then(|p| {
            p.display_name
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .or(p.name.as_ref().filter(|s| !s.trim().is_empty()))
        })
        .cloned()
        .unwrap_or_else(|| truncate_pk(&pk));
    let initial = display_name.chars().next().unwrap_or('?');
    rsx! {
        div {
            class: "flex items-center gap-3 p-2 rounded-lg hover:bg-accent/50 transition",
            div { class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center overflow-hidden",
                if let Some(pic) = &picture {
                    if pic.starts_with("http://") || pic.starts_with("https://") {
                        img {
                            class: "w-full h-full object-cover",
                            src: "{pic}",
                            alt: "{display_name}",
                        }
                    } else {
                        span { class: "text-xs text-muted-foreground", "{initial}" }
                    }
                } else {
                    span { class: "text-xs text-muted-foreground", "{initial}" }
                }
            }
            span { class: "text-sm font-medium text-foreground", "{display_name}" }
            span { class: "px-2 py-0.5 text-xs rounded-full {role_class}", "{role}" }
        }
    }
}

fn truncate_pk(pk: &str) -> String {
    let char_count = pk.chars().count();
    if char_count > 12 {
        let prefix: String = pk.chars().take(6).collect();
        let suffix: String = pk.chars().skip(char_count - 4).collect();
        format!("{}...{}", prefix, suffix)
    } else {
        pk.to_string()
    }
}
