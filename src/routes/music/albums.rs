use crate::components::{ExploreAlbumCard, ExploreAlbumCardSkeleton};
use crate::routes::Route;
use crate::services::music_explore::{self, ExploreAlbum};
use dioxus::prelude::*;

const PAGE_SIZE: usize = 48;

/// Full Albums browse page (Wavlake + Podcasting 2.0 feeds).
#[component]
pub fn MusicAlbums() -> Element {
    let mut albums = use_signal(Vec::<ExploreAlbum>::new);
    let mut loading = use_signal(|| true);
    use_effect(move || {
        spawn(async move {
            albums.set(music_explore::fetch_explore_albums(PAGE_SIZE).await);
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
            h1 { class: "text-2xl font-bold", "All Albums" }
            if *loading.read() {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for i in 0..12 {
                        ExploreAlbumCardSkeleton { key: "{i}" }
                    }
                }
            } else if albums.read().is_empty() {
                div { class: "text-center py-16 text-muted-foreground",
                    "No albums found right now. Check back later."
                }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for album in albums.read().iter() {
                        ExploreAlbumCard {
                            key: "{key_of(album)}",
                            album: album.clone(),
                        }
                    }
                }
            }
        }
    }
}

fn key_of(album: &ExploreAlbum) -> String {
    match album {
        ExploreAlbum::Wavlake { id, .. } => format!("wl-{id}"),
        ExploreAlbum::Rss { feed_id, .. } => format!("rss-{feed_id}"),
    }
}
