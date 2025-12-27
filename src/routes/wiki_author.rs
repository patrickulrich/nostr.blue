//! Wiki Author Route
//! Display wiki articles by a specific author (NIP-54)

use dioxus::prelude::*;
use crate::components::{WikiCardSkeleton, WikiGrid};
use crate::components::icons::{BookOpenIcon, ArrowLeftIcon, UserIcon};
use crate::stores::{wiki_store, nostr_client, profiles};
use crate::stores::wiki_store::CachedWikiPage;
use crate::routes::Route;
use crate::utils::truncate_pubkey;

/// Wiki author page - shows all wiki articles by a specific author
#[component]
pub fn WikiAuthor(pubkey: String) -> Element {
    let nav = use_navigator();
    let mut loading = use_signal(|| true);
    let mut pages = use_signal(Vec::<CachedWikiPage>::new);
    let mut error = use_signal(|| None::<String>);

    // Get author profile
    let author_profile = profiles::get_profile(&pubkey);
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));
    let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());

    // Fetch wiki pages by author
    let pubkey_clone = pubkey.clone();
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let pk = pubkey_clone.clone();
        spawn(async move {
            loading.set(true);
            match wiki_store::fetch_wiki_pages_by_author(&pk, 50).await {
                Ok(result) => {
                    pages.set(result);
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
        nav.push(Route::WikiHome {});
    };

    rsx! {
        div {
            class: "max-w-6xl mx-auto px-4 py-6",

            // Back button
            div {
                class: "mb-6",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Back to Wiki"
                }
            }

            // Author header
            div {
                class: "flex items-center gap-4 mb-8",
                // Author avatar
                Link {
                    to: Route::Profile { pubkey: pubkey.clone() },
                    if let Some(ref picture) = author_picture {
                        img {
                            class: "w-16 h-16 rounded-full object-cover",
                            src: "{picture}",
                            alt: "{author_name}",
                        }
                    } else {
                        div {
                            class: "w-16 h-16 rounded-full bg-gradient-to-br from-primary/50 to-accent/50 flex items-center justify-center text-2xl font-medium text-primary-foreground",
                            "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                }
                div {
                    h1 {
                        class: "text-2xl font-bold text-foreground",
                        "Articles by {author_name}"
                    }
                    div {
                        class: "flex items-center gap-4 mt-1 text-sm text-muted-foreground",
                        span {
                            class: "flex items-center gap-1",
                            BookOpenIcon { class: "w-4 h-4" }
                            "{pages.read().len()} articles"
                        }
                        Link {
                            to: Route::Profile { pubkey: pubkey.clone() },
                            class: "flex items-center gap-1 hover:text-foreground transition-colors",
                            UserIcon { class: "w-4 h-4" }
                            "View profile"
                        }
                    }
                }
            }

            // Error state
            if let Some(ref e) = *error.read() {
                div {
                    class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "Error loading articles: {e}"
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && pages.read().is_empty()) {
                // Loading skeleton
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    for _ in 0..4 {
                        WikiCardSkeleton {}
                    }
                }
            } else if pages.read().is_empty() {
                // Empty state
                div {
                    class: "flex flex-col items-center justify-center py-16 text-center",
                    BookOpenIcon { class: "w-16 h-16 text-muted-foreground mb-4" }
                    h2 {
                        class: "text-xl font-semibold text-foreground mb-2",
                        "No Articles Yet"
                    }
                    p {
                        class: "text-muted-foreground mb-6 max-w-md",
                        "{author_name} hasn't published any wiki articles yet."
                    }
                }
            } else {
                WikiGrid {
                    pages: pages.read().clone(),
                    loading: *loading.read(),
                }
            }
        }
    }
}
