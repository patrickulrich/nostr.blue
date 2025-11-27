use dioxus::prelude::*;
use crate::services::wavlake::WavlakeSearchResult;
use crate::components::icons::UserIcon;

#[derive(Props, Clone, PartialEq)]
pub struct ArtistCardProps {
    pub result: WavlakeSearchResult,
}

#[component]
pub fn ArtistCard(props: ArtistCardProps) -> Element {
    let result = &props.result;

    rsx! {
        Link {
            to: crate::routes::Route::MusicArtist { artist_id: result.id.clone() },
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group",

            // Artist image (circular)
            div {
                class: "relative flex-shrink-0",
                if let Some(ref art_url) = result.artist_art_url.as_ref().filter(|u| !u.is_empty()) {
                    img {
                        src: "{art_url}",
                        alt: "{result.name}",
                        class: "w-14 h-14 rounded-full object-cover",
                        loading: "lazy"
                    }
                } else {
                    div {
                        class: "w-14 h-14 rounded-full bg-muted flex items-center justify-center",
                        UserIcon { class: "w-6 h-6 text-muted-foreground".to_string() }
                    }
                }
            }

            // Artist info
            div {
                class: "flex-1 min-w-0",
                div {
                    class: "font-medium text-sm truncate",
                    "{result.name}"
                }
                div {
                    class: "text-xs text-muted-foreground",
                    "Artist"
                }
            }
        }
    }
}

#[component]
pub fn ArtistCardSkeleton() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 rounded-lg animate-pulse",
            div { class: "w-14 h-14 bg-muted rounded-full flex-shrink-0" }
            div {
                class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/4" }
            }
        }
    }
}
