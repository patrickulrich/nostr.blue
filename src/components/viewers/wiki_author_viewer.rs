use crate::components::icons::{ArrowLeftIcon, BookOpenIcon, UserIcon};
use crate::components::{ApiInitializingState, WikiCardSkeleton, WikiGrid};
use crate::hooks::use_nostr_resource_public;
use crate::hooks::NostrResourceState;
use crate::routes::Route;
use crate::stores::{profiles, wiki_store};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

#[component]
pub fn WikiAuthorViewer(pubkey: String) -> Element {
    let nav = use_navigator();
    let author_profile = profiles::get_profile(&pubkey);
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));
    let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());
    let pk = pubkey.clone();
    let pages = use_nostr_resource_public(move || {
        let pk = pk.clone();
        async move { wiki_store::fetch_wiki_pages_by_author(&pk, 50).await }
    });
    let pages_state = pages.state();
    let go_back = move |_| {
        nav.push(Route::WikiHome {});
    };
    rsx! {
        div { class: "max-w-6xl mx-auto px-4 py-6",
            div { class: "mb-6",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Back to Wiki"
                }
            }
            div { class: "flex items-center gap-4 mb-8",
                Link {
                    to: Route::Profile {
                        pubkey: crate::utils::nip19_urls::profile_route_id(&pubkey),
                    },
                    if let Some(ref picture) = author_picture {
                        img {
                            class: "w-16 h-16 rounded-full object-cover",
                            src: "{picture}",
                            alt: "{author_name}",
                        }
                    } else {
                        div { class: "w-16 h-16 rounded-full bg-gradient-to-br from-primary/50 to-accent/50 flex items-center justify-center text-2xl font-medium text-primary-foreground",
                            "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                }
                div {
                    h1 { class: "text-2xl font-bold text-foreground", "Articles by {author_name}" }
                    div { class: "flex items-center gap-4 mt-1 text-sm text-muted-foreground",
                        span { class: "flex items-center gap-1",
                            BookOpenIcon { class: "w-4 h-4" }
                            match &*pages_state.read() {
                                NostrResourceState::Loaded(_data) => "{_data.len()} articles",
                                _ => "0 articles",
                            }
                        }
                        Link {
                            to: Route::Profile {
                                pubkey: crate::utils::nip19_urls::profile_route_id(&pubkey),
                            },
                            class: "flex items-center gap-1 hover:text-foreground transition-colors",
                            UserIcon { class: "w-4 h-4" }
                            "View profile"
                        }
                    }
                }
            }
            match &*pages_state.read() {
                NostrResourceState::Initializing => rsx! {
                    ApiInitializingState { item_label: "articles" }
                },
                NostrResourceState::Loading => rsx! {
                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        for _ in 0..4 {
                            WikiCardSkeleton {}
                        }
                    }
                },
                NostrResourceState::Error(e) => rsx! {
                    div { class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                        "Error loading articles: {e}"
                    }
                },
                NostrResourceState::Loaded(data) => {
                    if data.is_empty() {
                        rsx! {
                            div { class: "flex flex-col items-center justify-center py-16 text-center",
                                BookOpenIcon { class: "w-16 h-16 text-muted-foreground mb-4" }
                                h2 { class: "text-xl font-semibold text-foreground mb-2", "No Articles Yet" }
                                p { class: "text-muted-foreground mb-6 max-w-md",
                                    "{author_name} hasn't published any wiki articles yet."
                                }
                            }
                        }
                    } else {
                        rsx! {
                            WikiGrid { pages: data.clone(), loading: false }
                        }
                    }
                }
                NostrResourceState::AuthRequired => rsx! {},
            }
        }
    }
}
