//! Contributors List Component
//!
//! Displays repository contributors with role badges (owner, maintainer)
//! and optional aggregate stats (issue count, PR count).
use crate::stores::profiles::PROFILE_CACHE;
use dioxus::prelude::*;

/// Display a list of repository contributors with role badges and optional stats
#[component]
pub fn ContributorsList(
    owner: String,
    maintainers: Vec<String>,
    #[props(default = None)]
    issue_count: Option<u32>,
    #[props(default = None)]
    pr_count: Option<u32>,
) -> Element {
    // Total contributor count: 1 (owner) + maintainers who aren't the owner
    let unique_maintainers: Vec<&String> = maintainers
        .iter()
        .filter(|m| **m != owner)
        .collect();
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
            {
                let profile = PROFILE_CACHE.read().peek(&owner).cloned();
                let picture = profile.as_ref().and_then(|p| p.picture.clone());
                let display_name = profile
                    .as_ref()
                    .and_then(|p| p.display_name.as_ref().or(p.name.as_ref()))
                    .cloned()
                    .unwrap_or_else(|| truncate_pk(&owner));
                let initial = display_name.chars().next().unwrap_or('?');
                rsx! {
                    div {
                        class: "flex items-center gap-3 p-2 rounded-lg hover:bg-accent/50 transition",
                        div { class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center overflow-hidden",
                            if let Some(pic) = &picture {
                                img {
                                    class: "w-full h-full object-cover",
                                    src: "{pic}",
                                    alt: "{display_name}",
                                }
                            } else {
                                span { class: "text-xs text-muted-foreground", "{initial}" }
                            }
                        }
                        span { class: "text-sm font-medium text-foreground", "{display_name}" }
                        span { class: "px-2 py-0.5 text-xs rounded-full bg-purple-500/20 text-purple-400", "Owner" }
                    }
                }
            }
            // Maintainers (excluding owner to avoid duplication)
            for maintainer in maintainers.iter().filter(|m| **m != owner) {
                {
                    let pk = maintainer.clone();
                    let profile = PROFILE_CACHE.read().peek(&pk).cloned();
                    let picture = profile.as_ref().and_then(|p| p.picture.clone());
                    let display_name = profile
                        .as_ref()
                        .and_then(|p| p.display_name.as_ref().or(p.name.as_ref()))
                        .cloned()
                        .unwrap_or_else(|| truncate_pk(&pk));
                    let initial = display_name.chars().next().unwrap_or('?');
                    rsx! {
                        div {
                            key: "{pk}",
                            class: "flex items-center gap-3 p-2 rounded-lg hover:bg-accent/50 transition",
                            div { class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center overflow-hidden",
                                if let Some(pic) = &picture {
                                    img {
                                        class: "w-full h-full object-cover",
                                        src: "{pic}",
                                        alt: "{display_name}",
                                    }
                                } else {
                                    span { class: "text-xs text-muted-foreground", "{initial}" }
                                }
                            }
                            span { class: "text-sm font-medium text-foreground", "{display_name}" }
                            span { class: "px-2 py-0.5 text-xs rounded-full bg-blue-500/20 text-blue-400", "Maintainer" }
                        }
                    }
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

#[allow(dead_code)]
fn truncate_pk(pk: &str) -> String {
    if pk.len() > 12 {
        format!("{}...{}", &pk[..6], &pk[pk.len() - 4..])
    } else {
        pk.to_string()
    }
}
