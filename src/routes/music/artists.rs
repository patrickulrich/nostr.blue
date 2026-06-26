use crate::components::{ExploreArtistCard, ExploreArtistCardSkeleton};
use crate::routes::Route;
use crate::services::music_explore::{self, ExploreArtist};
use dioxus::prelude::*;

const PAGE_SIZE: usize = 48;

/// Full Artists browse page (Wavlake + Nostr + Podcasting 2.0).
#[component]
pub fn MusicArtists() -> Element {
    let mut artists = use_signal(Vec::<ExploreArtist>::new);
    let mut loading = use_signal(|| true);
    use_effect(move || {
        spawn(async move {
            artists.set(music_explore::fetch_explore_artists(PAGE_SIZE).await);
            loading.set(false);
        });
    });
    rsx! {
        div { class: "max-w-5xl mx-auto p-4 space-y-6",
            Link {
                to: Route::MusicHome {},
                class: "inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground transition",
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "w-4 h-4",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path { d: "M15 19l-7-7 7-7" }
                }
                "Back to Music"
            }
            h1 { class: "text-2xl font-bold", "All Artists" }
            if *loading.read() {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for i in 0..12 {
                        ExploreArtistCardSkeleton { key: "{i}" }
                    }
                }
            } else if artists.read().is_empty() {
                div { class: "text-center py-16 text-muted-foreground",
                    "No artists found right now. Check back later."
                }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for artist in artists.read().iter() {
                        ExploreArtistCard {
                            key: "{key_of(artist)}",
                            artist: artist.clone(),
                        }
                    }
                }
            }
        }
    }
}

fn key_of(artist: &ExploreArtist) -> String {
    match artist {
        ExploreArtist::Wavlake { id, .. } => format!("wl-{id}"),
        ExploreArtist::Nostr { pubkey } => format!("nostr-{pubkey}"),
        ExploreArtist::Rss { name } => format!("rss-{name}"),
    }
}
