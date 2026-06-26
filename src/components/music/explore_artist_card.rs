use crate::components::icons::UserIcon;
use crate::routes::Route;
use crate::services::music_explore::ExploreArtist;
use crate::stores::profiles;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ExploreArtistCardProps {
    pub artist: ExploreArtist,
}
/// Vertical (circular avatar) card for the Explore "Artists" row. Handles
/// Wavlake artists, Nostr pubkeys (async profile resolution) and RSS authors.
#[component]
pub fn ExploreArtistCard(props: ExploreArtistCardProps) -> Element {
    // Nostr pubkeys need async profile resolution for name/avatar. Hooks must
    // run unconditionally, so we always set this up and only use it for Nostr.
    let nostr_pubkey = match &props.artist {
        ExploreArtist::Nostr { pubkey } => Some(pubkey.clone()),
        _ => None,
    };
    let mut display_name = use_signal(String::new);
    let mut avatar_url = use_signal(|| None::<String>);
    let mut lookup_gen = use_signal(|| 0u32);
    use_effect(use_reactive((&nostr_pubkey,), move |(pubkey_opt,)| {
        display_name.set(String::new());
        avatar_url.set(None);
        let Some(pubkey) = pubkey_opt else { return };
        let gen = lookup_gen.with_mut(|g| {
            *g = g.wrapping_add(1);
            *g
        });
        spawn(async move {
            if let Ok(profile) = profiles::fetch_profile(pubkey).await {
                if *lookup_gen.peek() == gen {
                    display_name.set(profile.get_display_name());
                    avatar_url.set(profile.picture.clone().filter(|p| !p.is_empty()));
                }
            }
        });
    }));
    match props.artist.clone() {
        ExploreArtist::Wavlake { id, name, art_url } => {
            let artist_id = id.clone();
            rsx! {
                Link {
                    key: "wl-{id}",
                    to: Route::MusicArtist { artist_id },
                    class: "group block text-center",
                    div { class: "aspect-square rounded-full overflow-hidden bg-muted relative mx-auto max-w-[160px]",
                        if !art_url.is_empty() {
                            img {
                                src: "{art_url}",
                                alt: "{name}",
                                class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                                loading: "lazy",
                                referrerpolicy: "no-referrer",
                            }
                        } else {
                            div { class: "w-full h-full bg-gradient-to-br from-purple-500/20 to-pink-500/20 flex items-center justify-center",
                                UserIcon { class: "w-12 h-12 text-muted-foreground/50" }
                            }
                        }
                    }
                    h3 { class: "mt-2 font-medium text-sm truncate group-hover:text-primary transition", "{name}" }
                    p { class: "text-xs text-muted-foreground", "Artist" }
                }
            }
        }
        ExploreArtist::Nostr { pubkey } => {
            let route = Route::MusicArtist { artist_id: pubkey.clone() };
            let name = if display_name.read().is_empty() {
                crate::utils::format::truncate_pubkey(&pubkey)
            } else {
                display_name.read().clone()
            };
            let avatar = (*avatar_url.read()).clone();
            rsx! {
                Link {
                    key: "nostr-{pubkey}",
                    to: route,
                    class: "group block text-center",
                    div { class: "aspect-square rounded-full overflow-hidden bg-muted relative mx-auto max-w-[160px]",
                        if let Some(url) = avatar {
                            img {
                                src: "{url}",
                                alt: "{name}",
                                class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                                loading: "lazy",
                                referrerpolicy: "no-referrer",
                            }
                        } else {
                            div { class: "w-full h-full bg-gradient-to-br from-purple-500/20 to-pink-500/20 flex items-center justify-center",
                                UserIcon { class: "w-12 h-12 text-muted-foreground/50" }
                            }
                        }
                        div { class: "absolute top-2 right-2 px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-purple-500/20 text-purple-400",
                            title: "Nostr",
                            "N"
                        }
                    }
                    h3 { class: "mt-2 font-medium text-sm truncate group-hover:text-primary transition", "{name}" }
                    p { class: "text-xs text-muted-foreground", "Artist" }
                }
            }
        }
        ExploreArtist::Rss { name } => {
            rsx! {
                Link {
                    key: "rss-{name}",
                    to: Route::MusicRssArtist { artist: name.clone() },
                    class: "group block text-center",
                    div { class: "aspect-square rounded-full overflow-hidden bg-muted relative mx-auto max-w-[160px]",
                        div { class: "w-full h-full bg-gradient-to-br from-orange-500/20 to-red-500/20 flex items-center justify-center",
                            UserIcon { class: "w-12 h-12 text-muted-foreground/50" }
                        }
                        div { class: "absolute top-2 right-2 px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-orange-500/20 text-orange-400",
                            title: "Podcasting 2.0",
                            "RSS"
                        }
                    }
                    h3 { class: "mt-2 font-medium text-sm truncate group-hover:text-primary transition", "{name}" }
                    p { class: "text-xs text-muted-foreground", "Artist" }
                }
            }
        }
    }
}
#[component]
pub fn ExploreArtistCardSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse text-center",
            div { class: "aspect-square rounded-full bg-muted mx-auto max-w-[160px]" }
            div { class: "mt-2 h-4 bg-muted rounded w-3/4 mx-auto" }
            div { class: "mt-1 h-3 bg-muted rounded w-1/4 mx-auto" }
        }
    }
}
