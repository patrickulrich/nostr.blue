//! Starred Repositories Page
//!
//! Shows the user's starred repositories loaded from their Kind 7 (Reaction) events.
use crate::components::{icons, CodeRepoCard};
use crate::routes::Route;
use crate::services::git_hosting::stars::load_user_stars;
use crate::stores::{auth_store, code_store, nostr_client};
use crate::stores::nostr_client::fetch_events_aggregated;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::time::Duration;

/// Starred repositories page component
#[component]
pub fn CodeStars() -> Element {
    // All hooks must be called before any early returns to maintain stable call-order indices.
    let mut repos = use_signal(Vec::<Repository>::new);
    let mut loading = use_signal(|| true);
    let mut search_query = use_signal(String::new);
    let mut star_load_error = use_signal(|| None::<String>);
    let mut refresh_counter = use_signal(|| 0u32);
    let mut request_id = use_signal(|| 0u32);

    use_effect(move || {
        let _counter = *refresh_counter.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        if !is_authenticated {
            loading.set(false);
            return;
        }
        let current_id = request_id.peek().wrapping_add(1);
        request_id.set(current_id);
        spawn(async move {
            loading.set(true);
            star_load_error.set(None);
            // Load stars from relays into STARRED_REPOS
            if let Err(e) = load_user_stars().await {
                log::warn!("Failed to load user stars: {}", e);
                if *request_id.peek() != current_id { return; }
                star_load_error.set(Some(format!("Failed to load stars: {}", e)));
                loading.set(false);
                return;
            }
            if *request_id.peek() != current_id { return; }
            // Hydrate CODE_REPOS_CACHE for starred repos
            let starred_coords: Vec<_> = code_store::STARRED_REPOS.peek().iter().cloned().collect();
            if !starred_coords.is_empty() {
                let authors: std::collections::HashSet<PublicKey> = starred_coords.iter()
                    .filter_map(|c| {
                        let parts: Vec<&str> = c.split(':').collect();
                        if parts.len() >= 3 && parts[0] == "30617" {
                            PublicKey::from_hex(parts[1]).ok()
                        } else { None }
                    })
                    .collect();
                if !authors.is_empty() {
                    let filter = Filter::new()
                        .kind(Kind::GitRepoAnnouncement)
                        .authors(authors);
                    match fetch_events_aggregated(filter, Duration::from_secs(15)).await {
                        Ok(events) => {
                            if *request_id.peek() != current_id { return; }
                            code_store::cache_repo_events(&events);
                        }
                        Err(e) => {
                            log::error!("Failed to fetch starred repo events: {}", e);
                            if *request_id.peek() != current_id { return; }
                            star_load_error.set(Some(format!("Failed to load repository details: {}", e)));
                            loading.set(false);
                            return;
                        }
                    }
                }
            }
            if *request_id.peek() != current_id { return; }
            // Get Repository objects from cache
            let starred = code_store::get_starred_repos();
            repos.set(starred);
            loading.set(false);
        });
    });

    let filtered = use_memo(move || {
        let query = search_query.read().to_lowercase();
        repos
            .read()
            .iter()
            .filter(|r| {
                if query.is_empty() {
                    return true;
                }
                let name = r.name.as_ref().unwrap_or(&r.id).to_lowercase();
                let desc = r.description.as_deref().unwrap_or_default().to_lowercase();
                name.contains(&query) || desc.contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    let auth = auth_store::AUTH_STATE.read();
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState {}
        };
    }

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
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
                                polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
                            }
                            "Starred Repositories"
                        }
                    }
                }
                div { class: "px-4 pb-3",
                    input {
                        class: "w-full px-3 py-1.5 bg-muted rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary",
                        r#type: "text",
                        placeholder: "Filter repositories...",
                        value: "{search_query}",
                        oninput: move |e| search_query.set(e.value()),
                    }
                }
            }
            div { class: "p-4",
                if let Some(err) = star_load_error.read().as_ref() {
                    div { class: "text-center py-8",
                        p { class: "text-red-400 mb-3", "{err}" }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition text-sm",
                            onclick: move |_| refresh_counter += 1,
                            "Retry"
                        }
                    }
                } else if *loading.read() {
                    LoadingState {}
                } else if filtered().is_empty() {
                    EmptyState { has_search: !search_query.read().is_empty() }
                } else {
                    div { class: "space-y-3",
                        for repo in filtered().iter() {
                            CodeRepoCard { key: "{repo.event_id}", repo: repo.clone() }
                        }
                    }
                }
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
                    "Connect with your Nostr identity to view your starred repositories."
                }
                Link {
                    to: Route::CodeHome {},
                    class: "text-primary hover:underline",
                    "← Back to Code"
                }
            }
        }
    }
}

#[component]
fn EmptyState(has_search: bool) -> Element {
    rsx! {
        div { class: "text-center py-16",
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
                    polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
                }
            }
            h3 { class: "font-semibold text-xl mb-2",
                if has_search {
                    "No matching repositories"
                } else {
                    "No Starred Repositories Yet"
                }
            }
            p { class: "text-muted-foreground max-w-md mx-auto mb-6",
                if has_search {
                    "Try adjusting your search filter."
                } else {
                    "Star repositories to keep track of interesting projects. Stars are stored as Nostr Kind 7 (Reaction) events."
                }
            }
            if !has_search {
                Link {
                    to: Route::CodeExplore {},
                    class: "inline-flex items-center gap-2 px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition font-medium",
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
                        circle { cx: "12", cy: "12", r: "10" }
                        polygon { points: "16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76" }
                    }
                    "Explore Repositories"
                }
            }
        }
    }
}

#[component]
fn LoadingState() -> Element {
    rsx! {
        div { class: "space-y-3",
            for i in 0..5 {
                div {
                    key: "{i}",
                    class: "bg-card border border-border rounded-lg p-4 animate-pulse",
                    div { class: "flex items-start gap-3",
                        div { class: "w-10 h-10 rounded-lg bg-muted" }
                        div { class: "flex-1",
                            div { class: "h-4 bg-muted rounded w-1/3 mb-2" }
                            div { class: "h-3 bg-muted rounded w-1/4" }
                        }
                    }
                    div { class: "h-3 bg-muted rounded w-2/3 mt-3" }
                    div { class: "flex gap-4 mt-3",
                        div { class: "h-3 bg-muted rounded w-12" }
                        div { class: "h-3 bg-muted rounded w-12" }
                    }
                }
            }
        }
    }
}
