use crate::routes::Route;
use crate::stores::ui::settings_store::get_canonical_external_origin;
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::{Event, Kind, PublicKey};

pub fn route_for_naddr(
    kind: u16,
    naddr: String,
    pubkey: PublicKey,
    identifier: String,
) -> Option<Route> {
    match kind {
        30009 => Some(Route::BadgeDetail { naddr }),
        30023 => Some(Route::ArticleDetail { naddr }),
        30040 => Some(Route::PublicationDetail { naddr }),
        30054 => Some(Route::PodcastNostrEpisodeDetail { naddr }),
        30078 => Some(Route::PodcastNostrDetail { naddr }),
        30311 => Some(Route::LiveStreamDetail { note_id: naddr }),
        30402 => Some(Route::ShopProductDetail { naddr }),
        30405 => Some(Route::ShopCollection { naddr }),
        30617 => Some(Route::CodeRepo { naddr }),
        30818 => {
            let npub = pubkey.to_bech32().unwrap_or_else(|_| pubkey.to_hex());
            Some(Route::WikiDetail { npub, identifier })
        }
        30030..=30033 => Some(Route::CitationDetail { naddr }),
        31922 | 31923 => Some(Route::CalendarEventDetail { naddr, from: None }),
        32123 => Some(Route::MusicPlaylistDetail { naddr }),
        33889 => Some(Route::PinBoardDetail { naddr }),
        34235 | 34236 => Some(Route::VideoDetail { video_id: naddr }),
        36787 => Some(Route::MusicTrackDetail { track_id: naddr }),
        38383 => Some(Route::P2POrderDetail { naddr }),
        _ => None,
    }
}

pub fn route_for_event(event: &Event) -> Route {
    let hex_id = event.id.to_hex();
    match event.kind.as_u16() {
        20 => Route::PhotoDetail { photo_id: hex_id },
        21 | 22 => Route::VideoDetail { video_id: hex_id },
        1040 => Route::VoiceMessageDetail { voice_id: hex_id },
        1068 => Route::PollView { noteid: hex_id },
        1621 => Route::CodeIssueDetail { note_id: hex_id },
        1622 => Route::CodePullDetail { note_id: hex_id },
        _ => {
            if event.kind.is_addressable() || event.kind.is_replaceable() {
                if let Some(coord) = event.coordinate() {
                    let kind = event.kind.as_u16();
                    let naddr = coord.to_bech32().unwrap_or_default();
                    let pubkey = (*coord.public_key).to_owned();
                    let identifier = coord.identifier.unwrap_or("").to_string();
                    if let Some(route) = route_for_naddr(kind, naddr, pubkey, identifier) {
                        return route;
                    }
                }
            }
            Route::Note {
                note_id: hex_id,
                from_voice: None,
            }
        }
    }
}

pub fn share_url_for_event(event: &Event) -> String {
    let is_recipe = event.kind == Kind::LongFormTextNote
        && event.tags.hashtags().any(|t| t == "nostrcooking");

    let route = if is_recipe {
        if let Some(coord) = event.coordinate() {
            if let Ok(naddr) = coord.to_bech32() {
                Route::RecipeDetail { naddr }
            } else {
                route_for_event(event)
            }
        } else {
            route_for_event(event)
        }
    } else {
        route_for_event(event)
    };

    let mut url = format!("{}{}", get_canonical_external_origin(), route);
    if url.ends_with('?') {
        url.pop();
    }
    url
}

pub fn content_label_for_event(event: &Event) -> &'static str {
    content_label_for_kind(event.kind.as_u16())
}

pub fn content_label_for_kind(kind: u16) -> &'static str {
    match kind {
        20 => "Photo",
        21 | 22 | 34235 | 34236 => "Video",
        1040 => "Voice Message",
        1068 => "Poll",
        1621 => "Issue",
        1622 => "Pull Request",
        30009 => "Badge",
        30023 => "Article",
        30040 => "Publication",
        30054 => "Podcast Episode",
        30078 => "Podcast",
        30311 => "Livestream",
        30402 => "Product",
        30405 => "Collection",
        30617 => "Repository",
        30818 => "Wiki",
        31922 | 31923 => "Event",
        32123 => "Playlist",
        33889 => "Pinboard",
        34139 => "Playlist",
        36787 => "Track",
        38383 => "P2P Order",
        _ => "Content",
    }
}
