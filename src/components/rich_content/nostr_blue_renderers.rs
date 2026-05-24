use crate::components::article_card::ArticleCard;
use crate::components::board::card::PinBoardCardCompact;
use crate::components::code::repo_card::CodeRepoCardCompact;
use crate::components::icons;
use crate::components::live::stream_card::LiveStreamCard;
use crate::components::music::radio_card::RadioCard;
use crate::components::podcast::episode_card::{DisplayEpisode, PodcastEpisodeCard};
use crate::components::podcast::show_card::{PodcastShow, PodcastShowCard};
use crate::components::publication::card::PublicationCardCompact;
use crate::components::recipe::card::RecipeCard;
use crate::components::wiki::card::WikiCardCompact;
use crate::components::{EventCardCompact, PhotoCard, VideoCard, VoiceMessageCard};
use crate::hooks::{use_fetch_event_by_coordinate_with_message, use_fetch_event_by_id};
use crate::components::viewers::music_track_viewer::fetch_track;
use crate::routes::Route;
use crate::services::{podcast_index, wavlake};
use crate::stores::calendar_store::UnifiedEvent;
use crate::stores::music_player::{self, MusicTrack};
use crate::stores::nostr_music::parse_playlist_event;
use crate::stores::pin_boards_store::parse_pinboard_event;
use crate::stores::profiles;
use crate::stores::publication_store::parse_publication_index;
use crate::utils::nip34::Repository;
use crate::utils::nip52::parse_calendar_event;
use crate::utils::nip54::parse_wiki_article;
use crate::utils::nip58::parse_badge_definition;
use crate::utils::nip99::parse_product;
use crate::utils::podcast::{parse_podcast_episode, parse_podcast_metadata};
use crate::utils::radio::RadioStation;
use crate::utils::recipe::extract_metadata as extract_recipe_metadata;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::{Event, Kind, PublicKey, ToBech32};

use super::minicards::{render_channel_minicard, render_playlist_minicard};

/// Generic loading skeleton for nostr.blue content cards
pub(super) fn nostr_blue_loading_skeleton() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-4 bg-muted border border-border rounded-lg animate-pulse",
            div { class: "w-12 h-12 bg-muted-foreground/30 rounded-lg shrink-0" }
            div { class: "flex-1 min-w-0",
                div { class: "h-4 bg-muted-foreground/30 rounded w-3/4 mb-2" }
                div { class: "h-3 bg-muted-foreground/20 rounded w-1/2" }
            }
        }
    }
}

/// Generic error display for nostr.blue content
pub(super) fn nostr_blue_error(message: &str) -> Element {
    rsx! {
        div { class: "p-4 bg-destructive/10 text-destructive rounded-lg text-sm",
            "{message}"
        }
    }
}

/// Renders a nostr.blue livestream link as a LiveStreamCard
#[component]
pub(super) fn NostrBlueLiveStreamRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Livestream not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                LiveStreamCard { event: ev.clone() }
            }
        }
    }
}

/// Renders a nostr.blue video link
#[component]
pub(super) fn NostrBlueVideoRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id, &[21, 22], "Video not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                VideoCard { event: ev.clone() }
            }
        }
    }
}

/// Renders a nostr.blue photo link
#[component]
pub(super) fn NostrBluePhotoRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id, &[20], "Photo not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                PhotoCard { event: ev.clone() }
            }
        }
    }
}

/// Renders a nostr.blue voice message link
#[component]
pub(super) fn NostrBlueVoiceRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id, &[1040], "Voice message not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                VoiceMessageCard { event: ev.clone() }
            }
        }
    }
}

/// Renders a nostr.blue podcast show link
#[component]
pub(super) fn NostrBluePodcastShowRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Podcast not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_podcast_show_card(ev, &id_for_link)}
            }
        }
    }
}

fn render_podcast_show_card(event: &Event, naddr: &str) -> Element {
    match parse_podcast_metadata(event) {
        Ok(metadata) => {
            let show = PodcastShow::from_nostr_metadata(&metadata);
            rsx! {
                PodcastShowCard { show, compact: true }
            }
        }
        Err(_) => {
            rsx! {
                Link {
                    to: Route::PodcastNostrDetail {
                        naddr: naddr.to_string(),
                    },
                    class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                    icons::MusicIcon { class: "w-4 h-4" }
                    "View Podcast"
                }
            }
        }
    }
}

/// Renders a nostr.blue podcast episode link
#[component]
pub(super) fn NostrBluePodcastEpisodeRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Episode not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_podcast_episode_card(ev, &id_for_link)}
            }
        }
    }
}

fn render_podcast_episode_card(event: &Event, naddr: &str) -> Element {
    match parse_podcast_episode(event) {
        Ok(episode) => {
            let display_episode =
                DisplayEpisode::from_nostr_episode(&episode, "Podcast Episode", None);
            rsx! {
                PodcastEpisodeCard { episode: display_episode, show_description: false }
            }
        }
        Err(_) => {
            rsx! {
                Link {
                    to: Route::PodcastNostrEpisodeDetail {
                        naddr: naddr.to_string(),
                    },
                    class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                    icons::MusicIcon { class: "w-4 h-4" }
                    "View Episode"
                }
            }
        }
    }
}

/// Renders a nostr.blue RSS podcast episode link with playback
#[component]
pub(super) fn NostrBlueRssPodcastEpisodeRenderer(
    podcast_id: String,
    episode_id: String,
) -> Element {
    let podcast_id_for_link = podcast_id.clone();
    let episode_id_for_link = episode_id.clone();
    let resource: Resource<Result<DisplayEpisode, String>> = use_resource(move || {
        let podcast_id = podcast_id.clone();
        let episode_id = episode_id.clone();
        async move {
            let decoded_episode_id = urlencoding::decode(&episode_id)
                .map(|s| s.into_owned())
                .unwrap_or(episode_id);
            let feed_id = podcast_id
                .parse::<u64>()
                .map_err(|_| "Invalid podcast ID format".to_string())?;
            let create_minimal_feed =
                |ep: &podcast_index::Episode, feed_id: u64| -> podcast_index::PodcastFeed {
                    podcast_index::PodcastFeed {
                        id: feed_id,
                        title: ep.feed_title.clone().unwrap_or_default(),
                        url: ep.feed_url.clone().unwrap_or_default(),
                        original_url: None,
                        link: None,
                        description: None,
                        author: None,
                        owner_name: None,
                        image: ep.feed_image.clone(),
                        artwork: None,
                        language: None,
                        itunes_id: None,
                        podcast_guid: ep.podcast_guid.clone(),
                        categories: None,
                        episode_count: None,
                        trending_score: None,
                        value: None,
                    }
                };
            if let Ok(ep_id) = decoded_episode_id.parse::<u64>() {
                if let Ok(ep) = podcast_index::get_episode_by_id(ep_id).await {
                    let feed = podcast_index::get_podcast_by_id(feed_id)
                        .await
                        .unwrap_or_else(|e| {
                            log::warn!("Feed fetch failed but episode found: {}", e);
                            create_minimal_feed(&ep, feed_id)
                        });
                    return Ok(DisplayEpisode::from_podcast_index_episode(&ep, &feed));
                }
                log::debug!("Direct episode fetch failed, falling back to search");
            } else if let Ok((ep, feed_opt)) =
                podcast_index::get_episode_by_guid(&decoded_episode_id, None).await
            {
                if ep.feed_id == Some(feed_id) {
                    let feed = match feed_opt {
                        Some(f) => f,
                        None => podcast_index::get_podcast_by_id(feed_id)
                            .await
                            .unwrap_or_else(|_| create_minimal_feed(&ep, feed_id)),
                    };
                    return Ok(DisplayEpisode::from_podcast_index_episode(&ep, &feed));
                } else {
                    log::debug!(
                        "GUID lookup returned episode from different feed, falling back to search"
                    );
                }
            } else {
                log::debug!("GUID-based episode fetch failed, falling back to search");
            }
            let feed = podcast_index::get_podcast_by_id(feed_id)
                .await
                .map_err(|e| format!("Failed to fetch podcast: {}", e))?;
            const MAX_PAGES: u32 = 5;
            const PAGE_SIZE: u32 = 100;
            for page in 0..MAX_PAGES {
                let fetch_count = PAGE_SIZE * (page + 1);
                let episodes =
                    podcast_index::get_episodes_by_feed_id(feed_id, Some(fetch_count), None)
                        .await
                        .map_err(|e| format!("Failed to fetch episodes: {}", e))?;
                let start_idx = if page == 0 {
                    0
                } else {
                    (PAGE_SIZE * page) as usize
                };
                let episodes_to_check = if start_idx < episodes.len() {
                    &episodes[start_idx..]
                } else {
                    break;
                };
                if let Some(ep) = episodes_to_check
                    .iter()
                    .find(|e| e.id.to_string() == decoded_episode_id)
                {
                    return Ok(DisplayEpisode::from_podcast_index_episode(ep, &feed));
                }
                if episodes.len() < fetch_count as usize {
                    break;
                }
            }
            Err(format!(
                "Episode not found (searched {} episodes)",
                MAX_PAGES * PAGE_SIZE,
            ))
        }
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            match resource.read_unchecked().as_ref() {
                None => nostr_blue_loading_skeleton(),
                Some(Err(err)) => rsx! {
                    div { class: "p-3 border border-border rounded-lg bg-card",
                        p { class: "text-sm text-muted-foreground mb-2", "{err}" }
                        Link {
                            to: Route::PodcastRssEpisodeDetail {
                                podcast_id: podcast_id_for_link.clone(),
                                episode_id: episode_id_for_link.clone(),
                            },
                            class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                            icons::MusicIcon { class: "w-4 h-4" }
                            "View Episode"
                        }
                    }
                },
                Some(Ok(display)) => rsx! {
                    PodcastEpisodeCard { episode: display.clone(), show_description: false }
                },
            }
        }
    }
}

/// Renders a nostr.blue RSS podcast show link
#[component]
pub(super) fn NostrBlueRssPodcastShowRenderer(podcast_id: String) -> Element {
    let podcast_id_for_link = podcast_id.clone();
    let resource: Resource<Result<PodcastShow, String>> = use_resource(move || {
        let podcast_id = podcast_id.clone();
        async move {
            let feed_id = podcast_id
                .parse::<u64>()
                .map_err(|_| "Invalid podcast ID format".to_string())?;
            let feed = podcast_index::get_podcast_by_id(feed_id)
                .await
                .map_err(|e| e.to_string())?;
            let value = feed.value.as_ref().and_then(|v| {
                let model = v.model.as_ref()?;
                Some(crate::utils::podcast::ValueBlock {
                    value_type: model
                        .model_type
                        .clone()
                        .unwrap_or_else(|| "lightning".to_string()),
                    method: model
                        .method
                        .clone()
                        .unwrap_or_else(|| "keysend".to_string()),
                    suggested: model.suggested.as_ref().and_then(|s| s.parse().ok()),
                    recipients: v
                        .destinations
                        .iter()
                        .filter_map(|d| {
                            Some(crate::utils::podcast::ValueRecipient {
                                name: d.name.clone(),
                                custom_key: None,
                                custom_value: None,
                                recipient_type: d
                                    .dest_type
                                    .clone()
                                    .unwrap_or_else(|| "node".to_string()),
                                address: d.address.clone()?,
                                split: d.split.unwrap_or(0),
                                fee: None,
                            })
                        })
                        .collect(),
                })
            });
            Ok(PodcastShow {
                id: feed.id.to_string(),
                title: feed.title.clone(),
                author: feed.author.clone().or(feed.owner_name.clone()),
                description: feed.description.clone(),
                image: feed.get_image().map(|s| s.to_string()),
                episode_count: feed.episode_count.map(|c| c as usize),
                source: crate::utils::podcast::PodcastSource::Rss {
                    feed_url: feed.url.clone(),
                    guid: feed.podcast_guid.clone().unwrap_or_default(),
                    podcast_id: Some(feed.id),
                },
                value,
                categories: feed
                    .categories
                    .as_ref()
                    .map(|c| c.values().cloned().collect())
                    .unwrap_or_default(),
                explicit: false,
            })
        }
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            match resource.read_unchecked().as_ref() {
                None => nostr_blue_loading_skeleton(),
                Some(Err(_)) => rsx! {
                    Link {
                        to: Route::PodcastRssFeedDetail {
                            podcast_id: podcast_id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        icons::MusicIcon { class: "w-4 h-4" }
                        "View Podcast"
                    }
                },
                Some(Ok(show)) => rsx! {
                    PodcastShowCard { show: show.clone(), compact: true }
                },
            }
        }
    }
}

/// Renders a nostr.blue music playlist link
#[component]
pub(super) fn NostrBlueMusicPlaylistRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Playlist not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Ok(playlist) = parse_playlist_event(ev) {
                    {render_playlist_minicard(&playlist, &id_for_link)}
                } else {
                    Link {
                        to: Route::MusicPlaylistDetail {
                            naddr: id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        icons::MusicIcon { class: "w-4 h-4" }
                        "View Playlist"
                    }
                }
            }
        }
    }
}

/// Renders a nostr.blue radio station link
#[component]
pub(super) fn NostrBlueRadioStationRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Radio station not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_radio_station_card(ev, &id_for_link)}
            }
        }
    }
}

fn render_radio_station_card(event: &Event, naddr: &str) -> Element {
    match RadioStation::from_event(event) {
        Ok(station) => {
            rsx! {
                RadioCard { station }
            }
        }
        Err(_) => {
            rsx! {
                Link {
                    to: Route::RadioStation {
                        naddr: naddr.to_string(),
                    },
                    class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                    icons::RssIcon { class: "w-4 h-4" }
                    "View Radio Station"
                }
            }
        }
    }
}

/// Renders a nostr.blue article link
#[component]
pub(super) fn NostrBlueArticleRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Article not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                ArticleCard { event: ev.clone() }
            }
        }
    }
}

/// Renders a nostr.blue recipe link
#[component]
pub(super) fn NostrBlueRecipeRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Recipe not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_recipe_from_event(ev, &id_for_link)}
            }
        }
    }
}

fn render_recipe_from_event(event: &Event, naddr: &str) -> Element {
    use crate::stores::recipe_store::CachedRecipe;
    let metadata = extract_recipe_metadata(event);
    let identifier = metadata
        .identifier
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            log::debug!(
                "Recipe event {} has empty identifier, using event ID as fallback",
                event.id
            );
            event.id.to_hex()
        });
    let a_tag = format!("30023:{}:{}", event.pubkey.to_hex(), identifier);
    let cached = CachedRecipe {
        event: event.clone(),
        metadata,
        parsed: None,
        naddr: naddr.to_string(),
        a_tag,
    };
    rsx! {
        RecipeCard { recipe: cached }
    }
}

/// Renders a nostr.blue note link
#[component]
pub(super) fn NostrBlueNoteRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id, &[1, 6, 16], "Note not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_note_minicard(ev, &ev.id.to_hex())}
            }
        }
    }
}

fn render_note_minicard(event: &Event, note_id: &str) -> Element {
    let content_preview = {
        let char_count = event.content.chars().count();
        if char_count > 200 {
            let truncated: String = event.content.chars().take(200).collect();
            format!("{}...", truncated)
        } else {
            event.content.clone()
        }
    };
    let author_pk = event.pubkey.to_hex();
    rsx! {
        Link {
            to: Route::AddressViewer {
                address: crate::utils::nip19_urls::note_route_id_with_kind(note_id, Some(&author_pk), Some(event.kind)),
            },
            class: "block p-3 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
            div { class: "text-sm text-foreground line-clamp-3 whitespace-pre-wrap",
                "{content_preview}"
            }
        }
    }
}

/// Renders a nostr.blue profile link
#[component]
pub(super) fn NostrBlueProfileRenderer(id: String) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut loading = use_signal(|| true);
    let mut valid_pubkey_hex = use_signal(|| None::<String>);
    use_effect(move || {
        let id_clone = id.clone();
        spawn(async move {
            if let Ok(pubkey) = PublicKey::parse(&id_clone) {
                let hex = pubkey.to_hex();
                valid_pubkey_hex.set(Some(hex.clone()));
                if let Ok(fetched) = profiles::fetch_profile(hex).await {
                    profile.set(Some(fetched));
                }
            }
            loading.set(false);
        });
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if *loading.read() {
                {nostr_blue_loading_skeleton()}
            } else {
                {
                    render_profile_minicard(
                        profile.read().as_ref(),
                        valid_pubkey_hex.read().as_deref(),
                    )
                }
            }
        }
    }
}

fn render_profile_minicard(
    profile: Option<&profiles::Profile>,
    valid_pubkey: Option<&str>,
) -> Element {
    let display_name = profile
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| "Unknown".to_string());
    let picture = profile.and_then(|p| p.picture.clone());
    let about = profile.and_then(|p| p.about.clone());
    let avatar_initial = display_name.chars().next().unwrap_or('?');
    if let Some(pubkey) = valid_pubkey {
        rsx! {
            Link {
                to: Route::AddressViewer {
                    address: crate::utils::nip19_urls::profile_route_id(pubkey),
                },
                class: "flex items-center gap-3 p-3 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
                if let Some(ref pic) = picture.as_ref().filter(|u| is_valid_http_url(u)) {
                    img {
                        src: "{pic}",
                        class: "w-12 h-12 rounded-full object-cover shrink-0",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-full bg-muted flex items-center justify-center shrink-0 text-lg font-medium",
                        "{avatar_initial}"
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-foreground truncate", "{display_name}" }
                    if let Some(ref bio) = about {
                        div { class: "text-sm text-muted-foreground line-clamp-1", "{bio}" }
                    }
                }
            }
        }
    } else {
        rsx! {
            div { class: "flex items-center gap-3 p-3 border border-border rounded-lg bg-muted/50",
                if let Some(ref pic) = picture.as_ref().filter(|u| is_valid_http_url(u)) {
                    img {
                        src: "{pic}",
                        class: "w-12 h-12 rounded-full object-cover shrink-0",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-full bg-muted flex items-center justify-center shrink-0 text-lg font-medium",
                        "{avatar_initial}"
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-foreground truncate", "{display_name}" }
                    if let Some(ref bio) = about {
                        div { class: "text-sm text-muted-foreground line-clamp-1", "{bio}" }
                    }
                }
            }
        }
    }
}

/// Renders a nostr.blue calendar event link
#[component]
pub(super) fn NostrBlueCalendarEventRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Event not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Ok(cal_event) = parse_calendar_event(ev) {
                    EventCardCompact { event: UnifiedEvent::Calendar(cal_event), from: None }
                } else {
                    Link {
                        to: Route::CalendarEventDetail {
                            naddr: id_for_link.clone(),
                            from: None,
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        "View Event"
                    }
                }
            }
        }
    }
}

/// Renders a nostr.blue wiki link - dispatches to appropriate renderer based on id type
#[component]
pub(super) fn NostrBlueWikiRenderer(id: String) -> Element {
    if id.starts_with("naddr1") {
        rsx! {
            NostrBlueWikiNaddrRenderer { id }
        }
    } else {
        rsx! {
            NostrBlueWikiTopicRenderer { id }
        }
    }
}

/// Renders wiki topic links (simple d-tag identifier) - no hooks needed
#[component]
fn NostrBlueWikiTopicRenderer(id: String) -> Element {
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::WikiSlug {
                    slug: id.clone(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                "Wiki: {id}"
            }
        }
    }
}

/// Renders wiki naddr links with event fetching
#[component]
fn NostrBlueWikiNaddrRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Wiki page not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_wiki_card(ev, &id_for_link)}
            }
        }
    }
}

fn render_wiki_card(event: &Event, identifier: &str) -> Element {
    use crate::stores::wiki_store::CachedWikiPage;
    if let Ok(article) = parse_wiki_article(event) {
        let coord =
            Coordinate::new(Kind::from(30818), event.pubkey).identifier(&article.identifier);
        let naddr = coord.to_bech32().unwrap_or_else(|_| identifier.to_string());
        let a_tag = format!("30818:{}:{}", event.pubkey.to_hex(), article.identifier);
        let cached = CachedWikiPage {
            event: event.clone(),
            article: article.clone(),
            naddr,
            a_tag,
            forward_links: article.forward_links.clone(),
            backward_links: vec![],
            mime_type: None,
        };
        rsx! {
            WikiCardCompact { page: cached }
        }
    } else {
        let npub = event.pubkey.to_bech32().unwrap_or_else(|_| identifier.to_string());
        rsx! {
            Link {
                to: Route::WikiDetail {
                    npub,
                    identifier: identifier.to_string(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                "View Wiki Page"
            }
        }
    }
}

/// Renders a nostr.blue publication link
#[component]
pub(super) fn NostrBluePublicationRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Publication not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Some(pub_index) = parse_publication_index(ev) {
                    PublicationCardCompact { publication: pub_index }
                } else {
                    Link {
                        to: Route::PublicationDetail {
                            naddr: id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        "View Publication"
                    }
                }
            }
        }
    }
}

/// Renders a nostr.blue pinboard link
#[component]
pub(super) fn NostrBluePinboardRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Pinboard not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Some(pinboard) = parse_pinboard_event(ev, None) {
                    PinBoardCardCompact { board: pinboard }
                } else {
                    Link {
                        to: Route::PinBoardDetail {
                            naddr: id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        "View Pinboard"
                    }
                }
            }
        }
    }
}

/// Renders a nostr.blue badge link
#[component]
pub(super) fn NostrBlueBadgeRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Badge not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_badge_card(ev, &id_for_link)}
            }
        }
    }
}

fn render_badge_card(event: &Event, naddr: &str) -> Element {
    if let Ok(badge) = parse_badge_definition(event) {
        let name = badge.name.clone().unwrap_or_else(|| "Badge".to_string());
        let desc = badge.description.clone();
        let image = badge.image.clone();
        let thumb = badge.thumb.clone();
        rsx! {
            Link {
                to: Route::BadgeDetail {
                    naddr: naddr.to_string(),
                },
                class: "flex items-center gap-3 p-3 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
                if let Some(img_url) = image {
                    img {
                        src: "{img_url}",
                        class: "w-12 h-12 rounded-lg object-cover shrink-0",
                    }
                } else if let Some(thumb_url) = thumb {
                    img {
                        src: "{thumb_url}",
                        class: "w-12 h-12 rounded-lg object-cover shrink-0",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-lg bg-muted flex items-center justify-center shrink-0",
                        "🏆"
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-foreground truncate", "{name}" }
                    if let Some(description) = desc {
                        div { class: "text-sm text-muted-foreground line-clamp-1", "{description}" }
                    }
                }
            }
        }
    } else {
        rsx! {
            Link {
                to: Route::BadgeDetail {
                    naddr: naddr.to_string(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                "View Badge"
            }
        }
    }
}

/// Renders a nostr.blue product link
#[component]
pub(super) fn NostrBlueProductRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Product not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_product_card(ev, &id_for_link)}
            }
        }
    }
}

fn render_product_card(event: &Event, naddr: &str) -> Element {
    if let Ok(product) = parse_product(event) {
        let title = product.title.clone();
        let image_url = product.images.first().map(|img| img.url.clone());
        let price_display = format!("{} {}", product.price.amount, product.price.currency,);
        rsx! {
            Link {
                to: Route::ShopProductDetail {
                    naddr: naddr.to_string(),
                },
                class: "flex items-center gap-3 p-3 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
                if let Some(img_url) = image_url {
                    img {
                        src: "{img_url}",
                        class: "w-16 h-16 rounded-lg object-cover shrink-0",
                    }
                } else {
                    div { class: "w-16 h-16 rounded-lg bg-muted flex items-center justify-center shrink-0",
                        "🛍️"
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-foreground truncate", "{title}" }
                    div { class: "text-sm font-medium text-green-600 dark:text-green-400",
                        "{price_display}"
                    }
                }
            }
        }
    } else {
        rsx! {
            Link {
                to: Route::ShopProductDetail {
                    naddr: naddr.to_string(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                "View Product"
            }
        }
    }
}

/// Renders a nostr.blue code repo link
#[component]
pub(super) fn NostrBlueCodeRepoRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Repository not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Some(repo) = Repository::from_event(ev) {
                    CodeRepoCardCompact { repo }
                } else {
                    Link {
                        to: Route::AddressViewer {
                            address: id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        "View Repository"
                    }
                }
            }
        }
    }
}

/// Renders a nostr.blue community link
#[component]
pub(super) fn NostrBlueCommunityRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let parts: Vec<&str> = id.splitn(3, ':').collect();
    let is_valid = parts.len() == 3
        && parts[0].parse::<u32>().is_ok()
        && PublicKey::from_hex(parts[1]).is_ok()
        && !parts[2].is_empty();
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if is_valid {
                Link {
                    to: Route::CommunityPage {
                        a_tag: id_for_link.clone(),
                    },
                    class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                    icons::UsersIcon { class: "w-4 h-4" }
                    "View Community"
                }
            } else {
                span { class: "inline-flex items-center gap-2 px-3 py-2 bg-muted text-muted-foreground rounded-lg text-sm",
                    icons::UsersIcon { class: "w-4 h-4" }
                    "Invalid Community"
                }
            }
        }
    }
}

/// Renders a nostr.blue channel (NIP-28) link as a card
#[component]
pub(super) fn NostrBlueChannelRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id.clone(), &[40], "Channel not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_channel_minicard(ev, &ev.id.to_hex())}
            }
        }
    }
}

fn is_nostr_pubkey(id: &str) -> bool {
    id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit())
}

fn render_music_compact_card(
    image: Option<&str>,
    title: &str,
    subtitle: &str,
    on_play: impl FnMut(MouseEvent) + 'static,
    _fallback_link: Route,
) -> Element {
    let image = image.map(|s| s.to_string());
    let title = title.to_string();
    let subtitle = subtitle.to_string();
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-3 p-3 border border-border rounded-lg bg-card hover:bg-accent/10 transition",
                div { class: "w-12 h-12 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center",
                            icons::MusicIcon { class: "w-6 h-6 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    p { class: "text-xs text-muted-foreground truncate", "{subtitle}" }
                }
                button {
                    class: "shrink-0 w-9 h-9 rounded-full bg-primary text-primary-foreground flex items-center justify-center hover:bg-primary/90 transition",
                    onclick: on_play,
                    span { class: "w-4 h-4", dangerous_inner_html: icons::PLAY }
                }
            }
        }
    }
}

fn render_music_error_card(err: &str, label: &str, route: Route) -> Element {
    rsx! {
        div { class: "my-2 p-3 border border-border rounded-lg bg-card",
            p { class: "text-sm text-muted-foreground mb-2", "{err}" }
            Link {
                to: route,
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                icons::MusicIcon { class: "w-4 h-4" }
                "View {label}"
            }
        }
    }
}

#[allow(clippy::type_complexity)]
#[component]
pub(super) fn NostrBlueRssMusicAlbumRenderer(feed_id: String) -> Element {
    let feed_id_for_link = feed_id.clone();
    let resource: Resource<Result<(podcast_index::PodcastFeed, Vec<podcast_index::Episode>), String>> =
        use_resource(move || {
        let fid = feed_id.clone();
        async move {
            let id = fid.parse::<u64>().map_err(|_| "Invalid album ID".to_string())?;
            let feed = podcast_index::get_podcast_by_id(id)
                .await
                .map_err(|e| e.to_string())?;
            let episodes = podcast_index::get_episodes_by_feed_id(id, Some(100), None)
                .await
                .unwrap_or_default();
            Ok((feed, episodes))
        }
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            match resource.read_unchecked().as_ref() {
                None => nostr_blue_loading_skeleton(),
                Some(Err(err)) => render_music_error_card(
                    err,
                    "Album",
                    Route::MusicRssAlbum { feed_id: feed_id_for_link.parse().unwrap_or(0) },
                ),
                Some(Ok((feed, episodes))) => {
                    let title = feed.title.clone();
                    let image = feed.get_image().map(String::from);
                    let artist = feed.author.clone().unwrap_or_else(|| "Unknown Artist".to_string());
                    let count = episodes.len() as u64;
                    rsx! {
                        {
                            render_music_compact_card(
                                image.as_deref(),
                                &title,
                                &format!("{} · {} {}", artist, count, if count == 1 { "track" } else { "tracks" }),
                                move |_: MouseEvent| {
                                    if let Some(Ok((f, eps))) = resource.read_unchecked().as_ref() {
                                        let tracks: Vec<MusicTrack> = eps
                                            .iter()
                                            .map(|ep| MusicTrack::from_rss_music_track(ep, f, None))
                                            .collect();
                                        if let Some(first) = tracks.first().cloned() {
                                            music_player::play_track(first, Some(tracks), Some(0));
                                        }
                                    }
                                },
                                Route::MusicRssAlbum { feed_id: feed_id_for_link.parse().unwrap_or(0) },
                            )
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn NostrBlueTrackRenderer(track_id: String) -> Element {
    let track_id_for_play = track_id.clone();
    let track_id_for_link = track_id.clone();
    let resource: Resource<Result<MusicTrack, String>> = use_resource(move || {
        let id = track_id.clone();
        async move { fetch_track(&id).await }
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            match resource.read_unchecked().as_ref() {
                None => nostr_blue_loading_skeleton(),
                Some(Err(err)) => render_music_error_card(
                    err,
                    "Track",
                    Route::MusicTrackDetail { track_id: track_id_for_link },
                ),
                Some(Ok(track)) => {
                    let title = track.title.clone();
                    let artist = track.artist.clone();
                    let image = track.album_art_url.clone();
                    let track_clone = track.clone();
                    rsx! {
                        {
                            render_music_compact_card(
                                image.as_deref(),
                                &title,
                                &artist,
                                move |_: MouseEvent| {
                                    music_player::play_track(track_clone.clone(), None, None);
                                },
                                Route::MusicTrackDetail { track_id: track_id_for_play.clone() },
                            )
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn NostrBlueAlbumRenderer(album_id: String) -> Element {
    let album_id_for_link = album_id.clone();
    let resource: Resource<Result<wavlake::WavlakeAlbum, String>> =
        use_resource(move || {
        let id = album_id.clone();
        async move { wavlake::get_album(&id).await }
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            match resource.read_unchecked().as_ref() {
                None => nostr_blue_loading_skeleton(),
                Some(Err(err)) => render_music_error_card(
                    err,
                    "Album",
                    Route::MusicAlbum { album_id: album_id_for_link },
                ),
                Some(Ok(album)) => {
                    let title = album.title.clone();
                    let image = album.album_art_url.clone();
                    let artist = album.artist.clone();
                    let count = album.tracks.len();
                    rsx! {
                        {
                            render_music_compact_card(
                                image.as_deref(),
                                &title,
                                &format!("{} · {} {}", artist, count, if count == 1 { "track" } else { "tracks" }),
                                move |_: MouseEvent| {
                                    if let Some(Ok(a)) = resource.read_unchecked().as_ref() {
                                        let tracks: Vec<MusicTrack> =
                                            a.tracks.iter().map(|t| t.clone().into()).collect();
                                        if let Some(first) = tracks.first().cloned() {
                                            music_player::play_track(first, Some(tracks), Some(0));
                                        }
                                    }
                                },
                                Route::MusicAlbum { album_id: album_id_for_link.clone() },
                            )
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::type_complexity)]
#[component]
pub(super) fn NostrBlueArtistRenderer(artist_id: String) -> Element {
    let artist_id_for_link = artist_id.clone();
    let is_pubkey = is_nostr_pubkey(&artist_id);
    let resource: Resource<Result<(String, Option<String>, Option<usize>), String>> =
        use_resource(move || {
            let id = artist_id.clone();
            let is_pk = is_pubkey;
            async move {
                if is_pk {
                    let profile = profiles::fetch_profile(id.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                    let name = profile
                        .display_name
                        .or(profile.name)
                        .unwrap_or_else(|| id.clone());
                    Ok((name, profile.picture, None))
                } else {
                    let artist = wavlake::get_artist(&id).await?;
                    let image = artist.artist_art_url.clone();
                    Ok((artist.name, image, Some(artist.albums.len())))
                }
            }
        });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            match resource.read_unchecked().as_ref() {
                None => nostr_blue_loading_skeleton(),
                Some(Err(err)) => render_music_error_card(
                    err,
                    "Artist",
                    Route::MusicArtist { artist_id: artist_id_for_link },
                ),
                Some(Ok((name, image, album_count))) => {
                    let name_clone = name.clone();
                    let image_clone = image.clone();
                    let subtitle = match album_count {
                        Some(c) => format!("{} {}", c, if *c == 1 { "album" } else { "albums" }),
                        None => "Nostr Artist".to_string(),
                    };
                    let link = Route::MusicArtist { artist_id: artist_id_for_link.clone() };
                    rsx! {
                        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
                            Link {
                                to: link,
                                class: "flex items-center gap-3 p-3 border border-border rounded-lg bg-card hover:bg-accent/10 transition",
                                div { class: "w-12 h-12 rounded-full bg-muted shrink-0 overflow-hidden",
                                    if let Some(ref img) = image_clone {
                                        img {
                                            src: "{img}",
                                            alt: "{name_clone}",
                                            class: "w-full h-full object-cover",
                                            loading: "lazy",
                                        }
                                    } else {
                                        div { class: "w-full h-full flex items-center justify-center",
                                            icons::UserIcon { class: "w-6 h-6 text-muted-foreground".to_string() }
                                        }
                                    }
                                }
                                div { class: "flex-1 min-w-0",
                                    p { class: "font-medium text-sm truncate", "{name_clone}" }
                                    p { class: "text-xs text-muted-foreground", "{subtitle}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
