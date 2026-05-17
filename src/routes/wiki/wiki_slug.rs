use crate::components::icons::{ArrowLeftIcon, BookOpenIcon, UserIcon};
use crate::components::{WikiCardSkeleton, WikiGrid};
use crate::routes::Route;
use crate::stores::wiki_store::CachedWikiPage;
use crate::stores::{nostr_client, profiles, wiki_store};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr::{FromBech32, ToBech32};

#[component]
pub fn WikiSlug(slug: String) -> Element {
    if slug.starts_with("npub1") {
        rsx! { WikiSlugAuthor { npub: slug } }
    } else {
        rsx! { WikiSlugTopic { topic: slug } }
    }
}

#[component]
fn WikiSlugAuthor(npub: String) -> Element {
    let nav = use_navigator();
    let mut loading = use_signal(|| true);
    let mut pages = use_signal(Vec::<CachedWikiPage>::new);
    let mut error = use_signal(|| None::<String>);
    let mut pubkey_hex = use_signal(|| None::<String>);
    if let Ok(pk) = nostr::PublicKey::from_bech32(&npub) {
        pubkey_hex.set(Some(pk.to_hex()));
    }
    let pk_hex = pubkey_hex.read().clone();
    let author_profile = pk_hex.as_ref().and_then(|p| profiles::get_profile(p));
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&npub));
    let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());
    let pk_hex_effect = pk_hex.clone();
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let pk = pk_hex_effect.clone();
        if pk.is_none() {
            error.set(Some("Invalid npub".to_string()));
            loading.set(false);
            return;
        }
        let hex = pk.unwrap();
        spawn(async move {
            loading.set(true);
            match wiki_store::fetch_wiki_pages_by_author(&hex, 50).await {
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
                        pubkey: crate::utils::nip19_urls::profile_route_id(&npub),
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
                            "{pages.read().len()} articles"
                        }
                        Link {
                            to: Route::Profile {
                                pubkey: crate::utils::nip19_urls::profile_route_id(&npub),
                            },
                            class: "flex items-center gap-1 hover:text-foreground transition-colors",
                            UserIcon { class: "w-4 h-4" }
                            "View profile"
                        }
                    }
                }
            }
            if let Some(ref e) = *error.read() {
                div { class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "Error loading articles: {e}"
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read()
                || (*loading.read() && pages.read().is_empty())
            {
                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    for _ in 0..4 {
                        WikiCardSkeleton {}
                    }
                }
            } else if pages.read().is_empty() {
                div { class: "flex flex-col items-center justify-center py-16 text-center",
                    BookOpenIcon { class: "w-16 h-16 text-muted-foreground mb-4" }
                    h2 { class: "text-xl font-semibold text-foreground mb-2", "No Articles Yet" }
                    p { class: "text-muted-foreground mb-6 max-w-md",
                        "{author_name} hasn't published any wiki articles yet."
                    }
                }
            } else {
                WikiGrid { pages: pages.read().clone(), loading: *loading.read() }
            }
        }
    }
}

#[component]
fn WikiSlugTopic(topic: String) -> Element {
    let nav = use_navigator();
    let mut loading = use_signal(|| true);
    let mut pages = use_signal(Vec::<CachedWikiPage>::new);
    let mut error = use_signal(|| None::<String>);
    let identifier = topic.clone();
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let id = identifier.clone();
        spawn(async move {
            loading.set(true);
            match wiki_store::fetch_wiki_pages_by_topic(&id).await {
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
        div { class: "max-w-4xl mx-auto px-4 py-6",
            div { class: "mb-6",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Back to Wiki"
                }
            }
            h1 { class: "text-2xl font-bold text-foreground mb-2",
                "{topic}"
            }
            p { class: "text-muted-foreground mb-6",
                if pages.read().len() > 1 {
                    "{pages.read().len()} authors have written about this topic."
                } else if pages.read().len() == 1 {
                    "1 author has written about this topic."
                } else {
                    "No one has written about this topic yet."
                }
            }
            if let Some(ref e) = *error.read() {
                div { class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "Error loading pages: {e}"
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read() || *loading.read() {
                div { class: "space-y-4",
                    for _ in 0..3 {
                        WikiCardSkeleton {}
                    }
                }
            } else if pages.read().is_empty() {
                div { class: "flex flex-col items-center justify-center py-16 text-center",
                    BookOpenIcon { class: "w-16 h-16 text-muted-foreground mb-4" }
                    h2 { class: "text-xl font-semibold text-foreground mb-2", "No Pages Found" }
                    p { class: "text-muted-foreground mb-6 max-w-md",
                        "No wiki articles found for \"{topic}\"."
                    }
                    Link {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                        to: Route::WikiNew {},
                        "Create \"{topic}\""
                    }
                }
            } else {
                div { class: "space-y-4",
                    for page in pages.read().iter() {
                        {
                            let page = page.clone();
                            let author_hex = page.event.pubkey.to_hex();
                            let author_profile = profiles::get_profile(&author_hex);
                            let author_name = author_profile
                                .as_ref()
                                .and_then(|p| p.display_name.clone().or(p.name.clone()))
                                .unwrap_or_else(|| truncate_pubkey(&author_hex));
                            let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());
                            let page_npub = page.event.pubkey.to_bech32().unwrap_or_else(|_| author_hex.clone());
                            let time_ago = crate::utils::time::format_relative_time_ex(page.event.created_at, true, true);
                            let page_identifier = page.article.identifier.clone();
                            rsx! {
                                Link {
                                    to: Route::WikiDetail {
                                        npub: page_npub,
                                        identifier: page_identifier,
                                    },
                                    class: "block p-4 bg-card border border-border rounded-lg hover:border-primary/50 transition-colors",
                                    div { class: "flex items-start gap-4",
                                        if let Some(ref picture) = author_picture {
                                            img {
                                                class: "w-10 h-10 rounded-full object-cover shrink-0",
                                                src: "{picture}",
                                                alt: "{author_name}",
                                            }
                                        } else {
                                            div { class: "w-10 h-10 rounded-full bg-gradient-to-br from-primary/50 to-accent/50 flex items-center justify-center text-sm font-medium text-primary-foreground shrink-0",
                                                "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                                            }
                                        }
                                        div { class: "flex-1 min-w-0",
                                            h3 { class: "font-semibold text-lg text-foreground",
                                                "{page.article.title}"
                                            }
                                            if let Some(ref summary) = page.article.summary {
                                                p { class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                                                    "{summary}"
                                                }
                                            }
                                            div { class: "flex items-center gap-2 mt-2 text-sm text-muted-foreground",
                                                span { "{author_name}" }
                                                span { "•" }
                                                span { "{time_ago}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
