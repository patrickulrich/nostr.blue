use crate::components::icons::*;
use crate::routes::podcast::podcast_shared_states::{
    PodcastApiAuthRequiredState, PodcastApiInitializingState,
};
use crate::routes::Route;
use crate::services::podcast_index::{self, PodcastFeed};
use crate::stores::nostr_client;
use dioxus::prelude::*;

enum RssArtistState {
    Initializing,
    Loaded(Vec<PodcastFeed>),
    AuthRequired,
    Error(String),
}

#[component]
pub fn MusicRssArtist(artist: String) -> Element {
    let artist_name = artist.clone();
    let artist_data = use_resource(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();
        let query = artist.clone();
        async move {
            if !client_initialized {
                return RssArtistState::Initializing;
            }
            if !has_signer {
                return RssArtistState::AuthRequired;
            }
            let feeds = match podcast_index::search_music(&query, Some(40)).await {
                Ok(feeds) => feeds,
                Err(e) => {
                    return RssArtistState::Error(format!("Failed to search artist: {}", e));
                }
            };
            RssArtistState::Loaded(feeds)
        }
    });

    rsx! {
        div { class: "container mx-auto px-4 py-8 max-w-5xl",
            Link {
                to: Route::MusicHome {},
                class: "inline-flex items-center gap-2 text-muted-foreground hover:text-foreground mb-6 transition-colors",
                ArrowLeftIcon { class: "w-4 h-4".to_string() }
                "Back to Music"
            }
            match &*artist_data.read() {
                Some(RssArtistState::Initializing) => rsx! {
                    PodcastApiInitializingState { item_label: "artist" }
                },
                Some(RssArtistState::AuthRequired) => rsx! {
                    PodcastApiAuthRequiredState { item_label: "artist" }
                },
                Some(RssArtistState::Error(e)) => rsx! {
                    div { class: "bg-muted/30 rounded-lg border border-border p-8 text-center",
                        MusicIcon { class: "w-12 h-12 text-muted-foreground mx-auto mb-4" }
                        h2 { class: "text-2xl font-bold mb-2", "Artist Not Found" }
                        p { class: "text-muted-foreground", "{e}" }
                    }
                },
                Some(RssArtistState::Loaded(feeds)) => {
                    let display_name = feeds
                        .first()
                        .and_then(|f| f.author.clone())
                        .unwrap_or_else(|| artist_name.clone());
                    let artwork = feeds
                        .iter()
                        .filter_map(|f| f.get_image().map(String::from))
                        .next();
                    let filtered: Vec<&PodcastFeed> = feeds
                        .iter()
                        .filter(|f| {
                            f.author
                                .as_ref()
                                .map(|a| a.eq_ignore_ascii_case(&artist_name))
                                .unwrap_or(false)
                        })
                        .collect();
                    let album_count = filtered.len();
                    let album_label = if album_count == 1 { "album" } else { "albums" };
                    let filtered_feeds = filtered;
                    rsx! {
                        div { class: "space-y-6",
                            div { class: "bg-muted/30 rounded-lg border border-border p-6",
                                div { class: "flex flex-col sm:flex-row items-start gap-6",
                                    div { class: "w-48 h-48 bg-muted rounded-lg flex items-center justify-center overflow-hidden shrink-0",
                                        if let Some(ref art_url) = artwork {
                                            img {
                                                src: "{art_url}",
                                                alt: "{display_name}",
                                                class: "w-full h-full object-cover",
                                                referrerpolicy: "no-referrer",
                                            }
                                        } else {
                                            MusicIcon { class: "w-24 h-24 text-muted-foreground" }
                                        }
                                    }
                                    div { class: "flex-1 space-y-4",
                                        div {
                                            div { class: "flex items-center gap-2 mb-2",
                                                span { class: "inline-block px-2 py-1 rounded text-xs font-medium bg-orange-500/20 text-orange-400",
                                                    "RSS Music"
                                                }
                                            }
                                            h1 { class: "text-3xl font-bold", "{display_name}" }
                                            p { class: "text-xl text-muted-foreground mt-2",
                                                "{album_count} {album_label}"
                                            }
                                        }
                                        if let Some(desc) = filtered_feeds.first().and_then(|f| f.description.as_ref()) {
                                            p { class: "text-sm text-muted-foreground line-clamp-3", "{desc}" }
                                        }
                                    }
                                }
                            }
                            div { class: "space-y-1",
                                div { class: "flex items-center gap-2 mb-4 px-3",
                                    MusicIcon { class: "w-5 h-5 text-primary" }
                                    h2 { class: "text-xl font-bold", "Albums" }
                                }
                                if filtered_feeds.is_empty() {
                                    div { class: "text-center py-8",
                                        MusicIcon { class: "w-12 h-12 text-muted-foreground mx-auto mb-4" }
                                        p { class: "text-muted-foreground",
                                            "No albums found for this artist."
                                        }
                                    }
                                } else {
                                    div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4",
                                        for feed in filtered_feeds.iter() {
                                            {
                                                let feed_id = feed.id;
                                                let title = feed.title.clone();
                                                let image = feed.get_image().map(String::from);
                                                let track_count = feed.episode_count.unwrap_or(0);
                                                rsx! {
                                                    Link {
                                                        key: "{feed_id}",
                                                        to: Route::MusicRssAlbum { feed_id },
                                                        class: "group block bg-card border border-border rounded-lg overflow-hidden hover:border-primary/50 transition",
                                                        div { class: "relative aspect-square bg-muted",
                                                            if let Some(ref img) = image {
                                                                img {
                                                                    src: "{img}",
                                                                    alt: "{title}",
                                                                    class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                                                                    loading: "lazy",
                                                                    referrerpolicy: "no-referrer",
                                                                }
                                                            } else {
                                                                div { class: "w-full h-full flex items-center justify-center text-muted-foreground",
                                                                    MusicIcon { class: "w-12 h-12" }
                                                                }
                                                            }
                                                            if feed.value.is_some() {
                                                                div { class: "absolute top-2 left-2 px-1.5 py-0.5 text-[10px] font-semibold bg-amber-500/90 text-white rounded",
                                                                    "V4V"
                                                                }
                                                            }
                                                        }
                                                        div { class: "p-2",
                                                            p { class: "text-sm font-medium line-clamp-2", "{title}" }
                                                            if track_count > 0 {
                                                                p { class: "text-xs text-muted-foreground mt-0.5",
                                                                    "{track_count} tracks"
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
                }
                None => rsx! {
                    div { class: "bg-muted/30 rounded-lg border border-border p-6",
                        div { class: "flex items-center gap-6",
                            div { class: "w-48 h-48 bg-muted rounded-lg animate-pulse" }
                            div { class: "flex-1 space-y-4",
                                div { class: "h-8 bg-muted rounded w-64 animate-pulse" }
                                div { class: "h-4 bg-muted rounded w-48 animate-pulse" }
                                div { class: "h-4 bg-muted rounded w-32 animate-pulse" }
                            }
                        }
                    }
                    div { class: "mt-6 grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4",
                        for i in 0..8 {
                            div { key: "{i}", class: "animate-pulse" ,
                                div { class: "aspect-square bg-muted rounded-lg" }
                                div { class: "mt-2 space-y-1",
                                    div { class: "h-4 bg-muted rounded w-3/4" }
                                    div { class: "h-3 bg-muted rounded w-1/2" }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}
