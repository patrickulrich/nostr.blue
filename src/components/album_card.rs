use dioxus::prelude::*;
use crate::services::wavlake::WavlakeSearchResult;
use crate::components::icons::DiscIcon;

#[derive(Props, Clone, PartialEq)]
pub struct AlbumCardProps {
    pub result: WavlakeSearchResult,
}

#[component]
pub fn AlbumCard(props: AlbumCardProps) -> Element {
    let result = &props.result;

    rsx! {
        Link {
            to: crate::routes::Route::MusicAlbum { album_id: result.id.clone() },
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group",

            // Album art (square)
            div {
                class: "relative flex-shrink-0",
                if let Some(ref art_url) = result.album_art_url {
                    img {
                        src: "{art_url}",
                        alt: "{result.name}",
                        class: "w-14 h-14 rounded object-cover",
                        loading: "lazy"
                    }
                } else {
                    div {
                        class: "w-14 h-14 rounded bg-muted flex items-center justify-center",
                        DiscIcon { class: "w-6 h-6 text-muted-foreground".to_string() }
                    }
                }
            }

            // Album info
            div {
                class: "flex-1 min-w-0",
                div {
                    class: "font-medium text-sm truncate",
                    "{result.name}"
                }
                if let Some(ref artist) = result.artist {
                    div {
                        class: "text-xs text-muted-foreground truncate",
                        "{artist}"
                    }
                } else {
                    div {
                        class: "text-xs text-muted-foreground",
                        "Album"
                    }
                }
            }
        }
    }
}

#[component]
pub fn AlbumCardSkeleton() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 rounded-lg animate-pulse",
            div { class: "w-14 h-14 bg-muted rounded flex-shrink-0" }
            div {
                class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
        }
    }
}
