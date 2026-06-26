use crate::routes::Route;
use crate::stores::nostr_music::NostrPlaylist;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PlaylistCardProps {
    pub playlist: NostrPlaylist,
}
/// Vertical cover card for a Nostr music playlist (kind 34139).
#[component]
pub fn PlaylistCard(props: PlaylistCardProps) -> Element {
    let playlist = &props.playlist;
    let track_count = playlist.track_refs.len();
    rsx! {
        Link {
            to: Route::MusicPlaylistDetail {
                naddr: playlist.naddr.clone().unwrap_or_else(|| playlist.coordinate.clone()),
            },
            class: "group block",
            div { class: "aspect-square rounded-lg overflow-hidden bg-muted relative",
                if let Some(ref image) = playlist.image {
                    img {
                        src: "{image}",
                        alt: "{playlist.title}",
                        class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                    }
                } else if let Some(ref gradient) = playlist.gradient {
                    div {
                        class: "w-full h-full",
                        style: "background: linear-gradient(135deg, {gradient})",
                    }
                } else {
                    div { class: "w-full h-full bg-gradient-to-br from-purple-500/30 to-blue-500/30 flex items-center justify-center",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-12 h-12 text-muted-foreground/50",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3",
                            }
                        }
                    }
                }
                div { class: "absolute bottom-2 right-2 px-2 py-1 bg-black/70 rounded text-xs text-white font-medium",
                    "{track_count} tracks"
                }
                div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition flex items-center justify-center",
                    div { class: "w-12 h-12 bg-primary rounded-full flex items-center justify-center",
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
                h3 { class: "font-medium text-sm truncate group-hover:text-primary transition",
                    "{playlist.title}"
                }
                if let Some(ref desc) = playlist.description {
                    p { class: "text-xs text-muted-foreground truncate mt-0.5", "{desc}" }
                }
            }
        }
    }
}
#[component]
pub fn PlaylistCardSkeleton() -> Element {
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
