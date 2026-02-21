//! Code Notifications Page
//!
//! View code-related notifications (issues, PRs, reviews, mentions, comments).
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::notifications::{
    fetch_code_notifications, CodeNotification, CodeNotificationType,
};
use crate::stores::{auth_store, nostr_client};
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format::truncate_pubkey;
use crate::utils::format_relative_time_or;
use dioxus::prelude::*;
/// Code notifications page component
#[component]
pub fn CodeNotifications() -> Element {
    let mut notifications = use_signal(Vec::<CodeNotification>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut filter = use_signal(|| None::<CodeNotificationType>);
    let mut request_gen = use_signal(|| 0u32);
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        // Track auth pubkey so effect re-runs on account switch
        let auth = auth_store::AUTH_STATE.read();
        let pubkey = auth.pubkey.clone().unwrap_or_default();
        if pubkey.is_empty() {
            return;
        }
        let gen = request_gen.peek().wrapping_add(1);
        request_gen.set(gen);
        spawn(async move {
            loading.set(true);
            error.set(None);
            match fetch_code_notifications(50).await {
                Ok(notifs) => {
                    if *request_gen.peek() != gen { loading.set(false); return; }
                    notifications.set(notifs);
                }
                Err(e) => {
                    if *request_gen.peek() != gen { loading.set(false); return; }
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });
    if !auth_store::AUTH_STATE.read().is_authenticated {
        return rsx! { NotAuthenticatedState {} };
    }

    let current_filter = filter.read().clone();
    let filtered: Vec<CodeNotification> = notifications
        .read()
        .iter()
        .filter(|n| {
            current_filter
                .as_ref()
                .map(|f| &n.notification_type == f)
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeHome {},
                        class: "text-muted-foreground hover:text-foreground",
                        dangerous_inner_html: icons::ARROW_LEFT,
                    }
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
                            path { d: "M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" }
                            path { d: "M13.73 21a2 2 0 0 1-3.46 0" }
                        }
                        "Notifications"
                    }
                }
            }
            div { class: "p-4 space-y-4",
                // Filter chips
                div { class: "flex flex-wrap gap-2",
                    FilterChip {
                        label: "All",
                        active: current_filter.is_none(),
                        onclick: move |_| filter.set(None),
                    }
                    FilterChip {
                        label: "Issues",
                        active: current_filter.as_ref() == Some(&CodeNotificationType::IssueOpened),
                        onclick: move |_| filter.set(Some(CodeNotificationType::IssueOpened)),
                    }
                    FilterChip {
                        label: "PRs",
                        active: current_filter.as_ref() == Some(&CodeNotificationType::PullRequestOpened),
                        onclick: move |_| filter.set(Some(CodeNotificationType::PullRequestOpened)),
                    }
                    FilterChip {
                        label: "Reviews",
                        active: current_filter.as_ref() == Some(&CodeNotificationType::ReviewReceived),
                        onclick: move |_| filter.set(Some(CodeNotificationType::ReviewReceived)),
                    }
                    FilterChip {
                        label: "Mentions",
                        active: current_filter.as_ref() == Some(&CodeNotificationType::Mentioned),
                        onclick: move |_| filter.set(Some(CodeNotificationType::Mentioned)),
                    }
                    FilterChip {
                        label: "Comments",
                        active: current_filter.as_ref() == Some(&CodeNotificationType::CommentAdded),
                        onclick: move |_| filter.set(Some(CodeNotificationType::CommentAdded)),
                    }
                    FilterChip {
                        label: "Status",
                        active: current_filter.as_ref() == Some(&CodeNotificationType::StatusChanged),
                        onclick: move |_| filter.set(Some(CodeNotificationType::StatusChanged)),
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{err}"
                    }
                }
                if *loading.read() {
                    LoadingSkeleton {}
                } else if filtered.is_empty() {
                    EmptyState {}
                } else {
                    div { class: "space-y-2",
                        for notif in filtered.iter() {
                            NotificationCard { key: "{notif.event_id}", notification: notif.clone() }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn FilterChip(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: if active {
                "px-3 py-1 text-sm rounded-full bg-primary text-primary-foreground font-medium"
            } else {
                "px-3 py-1 text-sm rounded-full bg-muted text-muted-foreground hover:bg-accent transition"
            },
            onclick: move |e| onclick.call(e),
            "{label}"
        }
    }
}
#[component]
fn NotificationCard(notification: CodeNotification) -> Element {
    let author_profile = PROFILE_CACHE.read().peek(&notification.author_pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&notification.author_pubkey));
    let (icon_color, icon_bg) = match notification.notification_type {
        CodeNotificationType::IssueOpened => ("text-green-500", "bg-green-500/10"),
        CodeNotificationType::PullRequestOpened => ("text-purple-500", "bg-purple-500/10"),
        CodeNotificationType::ReviewReceived => ("text-blue-500", "bg-blue-500/10"),
        CodeNotificationType::Mentioned => ("text-orange-500", "bg-orange-500/10"),
        CodeNotificationType::StatusChanged => ("text-yellow-500", "bg-yellow-500/10"),
        CodeNotificationType::CommentAdded => ("text-muted-foreground", "bg-muted"),
    };
    let route = match notification.notification_type {
        CodeNotificationType::IssueOpened => Route::CodeIssueDetail {
            note_id: notification.event_id.clone(),
        },
        CodeNotificationType::PullRequestOpened
        | CodeNotificationType::ReviewReceived => Route::CodePullDetail {
            note_id: notification.event_id.clone(),
        },
        _ => Route::CodeHome {},
    };
    rsx! {
        Link {
            to: route,
            class: "block bg-card border border-border rounded-lg p-4 hover:bg-accent/50 transition",
            div { class: "flex items-start gap-3",
                div { class: "w-8 h-8 rounded-full {icon_bg} flex items-center justify-center shrink-0",
                    svg {
                        class: "w-4 h-4 {icon_color}",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        match notification.notification_type {
                            CodeNotificationType::IssueOpened => rsx! {
                                circle { cx: "12", cy: "12", r: "10" }
                                line { x1: "12", y1: "8", x2: "12", y2: "12" }
                                line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                            },
                            CodeNotificationType::PullRequestOpened => rsx! {
                                circle { cx: "18", cy: "18", r: "3" }
                                circle { cx: "6", cy: "6", r: "3" }
                                path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                                line { x1: "6", y1: "9", x2: "6", y2: "21" }
                            },
                            CodeNotificationType::ReviewReceived => rsx! {
                                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                                polyline { points: "14 2 14 8 20 8" }
                                line { x1: "16", y1: "13", x2: "8", y2: "13" }
                                line { x1: "16", y1: "17", x2: "8", y2: "17" }
                            },
                            CodeNotificationType::Mentioned => rsx! {
                                circle { cx: "12", cy: "12", r: "4" }
                                path { d: "M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-3.92 7.94" }
                            },
                            CodeNotificationType::StatusChanged => rsx! {
                                polyline { points: "22 12 18 12 15 21 9 3 6 12 2 12" }
                            },
                            CodeNotificationType::CommentAdded => rsx! {
                                path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                            },
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center justify-between gap-2",
                        span { class: "text-xs font-medium text-muted-foreground",
                            "{notification.notification_type}"
                        }
                        span { class: "text-xs text-muted-foreground shrink-0",
                            {format_relative_time_or(notification.created_at, "Unknown")}
                        }
                    }
                    h3 { class: "text-sm font-medium truncate mt-0.5", "{notification.title}" }
                    if !notification.summary.is_empty() {
                        p { class: "text-xs text-muted-foreground truncate mt-0.5",
                            "{notification.summary}"
                        }
                    }
                    p { class: "text-xs text-muted-foreground mt-1",
                        "by {author_name}"
                    }
                }
            }
        }
    }
}
#[component]
fn EmptyState() -> Element {
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
                    path { d: "M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" }
                    path { d: "M13.73 21a2 2 0 0 1-3.46 0" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Notifications" }
            p { class: "text-muted-foreground text-sm",
                "You're all caught up! Notifications will appear here when someone mentions you, opens issues or PRs on your repositories, or leaves reviews."
            }
        }
    }
}
#[component]
fn NotAuthenticatedState() -> Element {
    rsx! {
        div { class: "min-h-screen flex items-center justify-center p-4",
            div { class: "text-center max-w-md",
                div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                }
                h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                p { class: "text-muted-foreground mb-6",
                    "Connect with your Nostr identity to view your notifications."
                }
                Link { to: Route::CodeHome {}, class: "text-primary hover:underline", "Back to Code" }
            }
        }
    }
}
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-2 animate-pulse",
            for i in 0..5 {
                div {
                    key: "{i}",
                    class: "bg-card border border-border rounded-lg p-4",
                    div { class: "flex items-start gap-3",
                        div { class: "w-8 h-8 rounded-full bg-muted shrink-0" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-3 bg-muted rounded w-20" }
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/3" }
                        }
                    }
                }
            }
        }
    }
}
