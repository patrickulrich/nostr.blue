//! Publication Card Component
//! Display card for publications (NKBIP-01 Kind 30040)
use crate::components::content_menu::{ContentMenu, ContentMenuType};
use crate::components::icons::{BookOpenIcon, FileVideoIcon};
use crate::routes::Route;
use crate::stores::profiles;
use crate::stores::publication_store::{PublicationIndex, PublicationType};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
/// Publication card for feed display
#[component]
pub fn PublicationCard(publication: PublicationIndex) -> Element {
    let nav = use_navigator();
    let author_hex = publication.event.pubkey.to_hex();
    let author_profile = profiles::get_profile(&author_hex);
    let author_name = author_profile
        .as_ref()
        .and_then(crate::stores::profiles::display_name_or_name)
        .or_else(|| publication.author.clone())
        .unwrap_or_else(|| truncate_pubkey(&author_hex));
    let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());
    let naddr = publication.naddr.clone();
    let card_click = move |_| {
        nav.push(Route::PublicationDetail {
            naddr: naddr.clone(),
        });
    };
    let section_count = publication.section_addresses.len();
    let author_hex_menu = author_hex.clone();
    let naddr_menu = publication.naddr.clone();
    let event_id = publication.event.id.to_hex();
    rsx! {
        div {
            class: "relative bg-card border border-border rounded-lg overflow-hidden hover:border-primary/50 transition-colors cursor-pointer",
            onclick: card_click,
            div { class: "absolute top-2 right-2 z-10",
                ContentMenu {
                    content_type: ContentMenuType::Publication,
                    author_pubkey: author_hex_menu,
                    naddr: naddr_menu,
                    event_id: Some(event_id),
                }
            }
            if let Some(ref image) = publication.cover_image {
                div { class: "aspect-[2/1] overflow-hidden bg-muted",
                    img {
                        class: "w-full h-full object-cover",
                        src: "{image}",
                        alt: "{publication.title}",
                    }
                }
            } else {
                div { class: "aspect-[2/1] bg-gradient-to-br from-primary/20 to-accent/20 flex items-center justify-center",
                    BookOpenIcon { class: "w-12 h-12 text-muted-foreground" }
                }
            }
            div { class: "p-4 space-y-3",
                if publication.pub_type != PublicationType::default() {
                    span { class: "inline-flex items-center px-2 py-0.5 text-xs font-medium rounded-full bg-primary/10 text-primary",
                        {publication.pub_type.as_str()}
                    }
                }
                h3 { class: "font-semibold text-foreground line-clamp-2", "{publication.title}" }
                if let Some(ref summary) = publication.summary {
                    p { class: "text-sm text-muted-foreground line-clamp-2", "{summary}" }
                }
                div { class: "flex items-center gap-2 pt-2",
                    if let Some(ref picture) = author_picture {
                        img {
                            class: "w-6 h-6 rounded-full object-cover",
                            src: "{picture}",
                            alt: "{author_name}",
                        }
                    } else {
                        div { class: "w-6 h-6 rounded-full bg-gradient-to-br from-primary/50 to-accent/50 flex items-center justify-center text-xs font-medium text-primary-foreground",
                            "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                    span { class: "text-sm text-muted-foreground", "{author_name}" }
                }
                div { class: "flex items-center gap-1 text-xs text-muted-foreground",
                    FileVideoIcon { class: "w-3.5 h-3.5" }
                    span { "{section_count} sections" }
                }
            }
        }
    }
}
/// Compact publication card for lists
#[component]
pub fn PublicationCardCompact(publication: PublicationIndex) -> Element {
    let nav = use_navigator();
    let naddr = publication.naddr.clone();
    let card_click = move |_| {
        nav.push(Route::PublicationDetail {
            naddr: naddr.clone(),
        });
    };
    let section_count = publication.section_addresses.len();
    rsx! {
        div {
            class: "flex gap-3 p-3 bg-card border border-border rounded-lg hover:border-primary/50 transition-colors cursor-pointer",
            onclick: card_click,
            div { class: "shrink-0 w-16 h-20 rounded overflow-hidden bg-muted",
                if let Some(ref image) = publication.cover_image {
                    img {
                        class: "w-full h-full object-cover",
                        src: "{image}",
                        alt: "{publication.title}",
                    }
                } else {
                    div { class: "w-full h-full bg-gradient-to-br from-primary/20 to-accent/20 flex items-center justify-center",
                        BookOpenIcon { class: "w-6 h-6 text-muted-foreground" }
                    }
                }
            }
            div { class: "flex-1 min-w-0",
                h4 { class: "font-medium text-foreground line-clamp-1", "{publication.title}" }
                if let Some(ref author) = publication.author {
                    p { class: "text-sm text-muted-foreground", "by {author}" }
                }
                div { class: "flex items-center gap-1 text-xs text-muted-foreground mt-1",
                    FileVideoIcon { class: "w-3 h-3" }
                    span { "{section_count} sections" }
                }
            }
        }
    }
}
/// Skeleton loader for publication card
#[component]
pub fn PublicationCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden animate-pulse",
            div { class: "aspect-[2/1] bg-muted" }
            div { class: "p-4 space-y-3",
                div { class: "h-4 bg-muted rounded w-16" }
                div { class: "h-5 bg-muted rounded w-3/4" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-2/3" }
                div { class: "flex items-center gap-2 pt-2",
                    div { class: "w-6 h-6 bg-muted rounded-full" }
                    div { class: "h-3 bg-muted rounded w-20" }
                }
            }
        }
    }
}
/// Publication feed grid
#[component]
pub fn PublicationGrid(
    publications: Vec<PublicationIndex>,
    #[props(default = false)] loading: bool,
) -> Element {
    rsx! {
        div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
            for publication in publications.iter() {
                PublicationCard {
                    key: "{publication.event.id.to_hex()}",
                    publication: publication.clone(),
                }
            }
            if loading {
                for _ in 0..3 {
                    PublicationCardSkeleton {}
                }
            }
        }
    }
}
