//! Repository Discussions Page
//!
//! View discussions for a repository.
//! Follows patterns from code_repo_issues.rs.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::discussions::fetch_repo_discussions;
use crate::stores::nostr_client;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format_relative_time_or;
use crate::utils::nip34::Discussion;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
/// Category filter for discussions
#[derive(Clone, Copy, PartialEq)]
enum CategoryFilter {
    All,
    General,
    Ideas,
    QAndA,
    ShowAndTell,
}
impl CategoryFilter {
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::General => "General",
            Self::Ideas => "Ideas",
            Self::QAndA => "Q&A",
            Self::ShowAndTell => "Show & Tell",
        }
    }
    fn matches(&self, discussion: &Discussion) -> bool {
        match self {
            Self::All => true,
            Self::General => discussion.category.as_deref() == Some("general"),
            Self::Ideas => discussion.category.as_deref() == Some("ideas"),
            Self::QAndA => discussion.category.as_deref() == Some("q-a"),
            Self::ShowAndTell => discussion.category.as_deref() == Some("show-and-tell"),
        }
    }
}
/// Repository discussions page component
#[component]
pub fn CodeRepoDiscussions(naddr: String) -> Element {
    let mut discussions = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut active_filter = use_signal(|| CategoryFilter::All);
    use_effect(use_reactive(&naddr, move |n| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            error.set(None);
            loading.set(true);
            match fetch_repo_discussions(&n).await {
                Ok(fetched) => {
                    discussions.set(fetched);
                    error.set(None);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    }));
    let all_discussions = discussions.read();
    let filtered: Vec<_> = all_discussions
        .iter()
        .filter(|d| active_filter.read().matches(d))
        .cloned()
        .collect();
    let filters = [
        CategoryFilter::All,
        CategoryFilter::General,
        CategoryFilter::Ideas,
        CategoryFilter::QAndA,
        CategoryFilter::ShowAndTell,
    ];
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeRepo {
                                naddr: naddr.clone(),
                            },
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
                        div {
                            h1 { class: "text-xl font-bold flex items-center gap-2",
                                svg {
                                    class: "w-5 h-5",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                                }
                                "Discussions"
                            }
                            p { class: "text-sm text-muted-foreground",
                                if !*loading.read() {
                                    "{filtered.len()} of {all_discussions.len()} discussions"
                                }
                            }
                        }
                    }
                    Link {
                        to: Route::CodeDiscussionNew {
                            naddr: naddr.clone(),
                        },
                        class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition flex items-center gap-1",
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
                            line { x1: "12", y1: "5", x2: "12", y2: "19" }
                            line { x1: "5", y1: "12", x2: "19", y2: "12" }
                        }
                        "New Discussion"
                    }
                }
                if !*loading.read() && !all_discussions.is_empty() {
                    div { class: "px-4 pb-3 flex gap-2 flex-wrap",
                        for filter in filters.iter() {
                            button {
                                key: "{filter.label()}",
                                class: if *active_filter.read() == *filter {
                                    "px-3 py-1.5 text-sm rounded-lg bg-accent text-foreground font-medium"
                                } else {
                                    "px-3 py-1.5 text-sm rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent/50 transition"
                                },
                                onclick: {
                                    let f = *filter;
                                    move |_| active_filter.set(f)
                                },
                                "{filter.label()}"
                            }
                        }
                    }
                }
            }
            div { class: "p-4",
                if let Some(err) = error.read().as_ref() {
                    div { class: "text-center text-red-400 py-4", "{err}" }
                } else if *loading.read() {
                    LoadingSkeleton {}
                } else if filtered.is_empty() {
                    if all_discussions.is_empty() {
                        EmptyDiscussions {}
                    } else {
                        div { class: "text-center py-12",
                            p { class: "text-muted-foreground text-sm", "No discussions matching this filter" }
                        }
                    }
                } else {
                    div { class: "border border-border rounded-lg divide-y divide-border",
                        for discussion in filtered.iter() {
                            DiscussionRow { key: "{discussion.event_id}", discussion: discussion.clone() }
                        }
                    }
                }
            }
        }
    }
}
fn discussion_preview_title(discussion: &Discussion) -> String {
    discussion
        .subject
        .clone()
        .unwrap_or_else(|| {
            let preview: String = discussion.content.chars().take(80).collect();
            if discussion.content.len() > 80 {
                format!("{}...", preview)
            } else {
                preview
            }
        })
}

fn category_display_label(category: &str) -> &'static str {
    match category {
        "general" => "General",
        "ideas" => "Ideas",
        "q-a" => "Q&A",
        "show-and-tell" => "Show & Tell",
        _ => "Other",
    }
}

#[component]
fn DiscussionRow(discussion: Discussion) -> Element {
    let author_profile = PROFILE_CACHE.read().peek(&discussion.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&discussion.pubkey));
    let title = discussion_preview_title(&discussion);
    let category_label = discussion.category.as_deref().map(category_display_label);
    rsx! {
        Link {
            to: Route::CodeDiscussionDetail {
                note_id: discussion.event_id.clone(),
            },
            class: "block p-4 hover:bg-accent/50 transition",
            div { class: "flex items-start justify-between gap-3",
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2 mb-1",
                        svg {
                            class: "w-4 h-4 text-green-500 shrink-0",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                        }
                        span { class: "font-medium text-foreground truncate", "{title}" }
                    }
                    div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                        if let Some(cat) = category_label {
                            span { class: "px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-500 border border-blue-500/20",
                                "{cat}"
                            }
                        }
                        span { "{author_name}" }
                        span { "opened " {format_relative_time_or(discussion.created_at, "Unknown")} }
                    }
                }
                if discussion.comment_count > 0 {
                    div { class: "flex items-center gap-1 text-xs text-muted-foreground shrink-0",
                        svg {
                            class: "w-3.5 h-3.5",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                        }
                        "{discussion.comment_count}"
                    }
                }
            }
        }
    }
}
#[component]
fn EmptyDiscussions() -> Element {
    rsx! {
        div { class: "text-center py-12",
            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-muted-foreground",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Discussions" }
            p { class: "text-muted-foreground text-sm", "Start a discussion to share ideas or ask questions." }
        }
    }
}
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-2 animate-pulse",
            for i in 0..3 {
                div { key: "{i}", class: "p-4 border border-border rounded-lg",
                    div { class: "h-4 bg-muted rounded w-2/3 mb-3" }
                    div { class: "h-3 bg-muted rounded w-1/3" }
                }
            }
        }
    }
}
