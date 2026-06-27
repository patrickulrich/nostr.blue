use crate::components::icons::DiscIcon;
use crate::routes::Route;
use crate::services::music_explore::ExploreAlbum;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ExploreAlbumCardProps {
    pub album: ExploreAlbum,
}
/// Vertical cover card for the Explore "Albums" row. Handles both Wavlake
/// albums (-> MusicAlbum) and RSS music feeds (-> MusicRssAlbum).
#[component]
pub fn ExploreAlbumCard(props: ExploreAlbumCardProps) -> Element {
    match props.album.clone() {
        ExploreAlbum::Wavlake { id, title, art_url, artist } => {
            let album_id = id.clone();
            rsx! {
                Link {
                    key: "wl-{id}",
                    to: Route::MusicAlbum { album_id },
                    class: "group block",
                    div { class: "aspect-square rounded-lg overflow-hidden bg-muted relative",
                        if !art_url.is_empty() {
                            img {
                                src: "{art_url}",
                                alt: "{title}",
                                class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                                loading: "lazy",
                                referrerpolicy: "no-referrer",
                            }
                        } else {
                            div { class: "w-full h-full bg-gradient-to-br from-orange-500/20 to-red-500/20 flex items-center justify-center",
                                DiscIcon { class: "w-12 h-12 text-muted-foreground/50".to_string() }
                            }
                        }
                        div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition flex items-center justify-center",
                            div { class: "w-12 h-12 bg-primary rounded-full flex items-center justify-center shadow-lg",
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-6 h-6 text-primary-foreground ml-1",
                                    fill: "currentColor",
                                    view_box: "0 0 24 24",
                                    path { d: "M8 5v14l11-7z" }
                                }
                            }
                        }
                    }
                    div { class: "mt-2",
                        h3 { class: "font-medium text-sm truncate group-hover:text-primary transition", "{title}" }
                        p { class: "text-xs text-muted-foreground truncate", "{artist}" }
                    }
                }
            }
        }
        ExploreAlbum::Rss { feed_id, title, art_url, author } => {
            rsx! {
                Link {
                    key: "rss-{feed_id}",
                    to: Route::MusicRssAlbum { feed_id },
                    class: "group block",
                    div { class: "aspect-square rounded-lg overflow-hidden bg-muted relative",
                        if let Some(ref url) = art_url {
                            img {
                                src: "{url}",
                                alt: "{title}",
                                class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                                loading: "lazy",
                                referrerpolicy: "no-referrer",
                            }
                        } else {
                            div { class: "w-full h-full bg-gradient-to-br from-orange-500/20 to-red-500/20 flex items-center justify-center",
                                DiscIcon { class: "w-12 h-12 text-muted-foreground/50".to_string() }
                            }
                        }
                        div { class: "absolute top-2 right-2 px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-orange-500/20 text-orange-400",
                            title: "Podcasting 2.0",
                            "RSS"
                        }
                        div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition flex items-center justify-center",
                            div { class: "w-12 h-12 bg-primary rounded-full flex items-center justify-center shadow-lg",
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-6 h-6 text-primary-foreground ml-1",
                                    fill: "currentColor",
                                    view_box: "0 0 24 24",
                                    path { d: "M8 5v14l11-7z" }
                                }
                            }
                        }
                    }
                    div { class: "mt-2",
                        h3 { class: "font-medium text-sm truncate group-hover:text-primary transition", "{title}" }
                        p { class: "text-xs text-muted-foreground truncate",
                            {author.clone().unwrap_or_else(|| "Podcast feed".to_string())}
                        }
                    }
                }
            }
        }
    }
}
#[component]
pub fn ExploreAlbumCardSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse",
            div { class: "aspect-square rounded-lg bg-muted" }
            div { class: "mt-2 space-y-1",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
        }
    }
}
