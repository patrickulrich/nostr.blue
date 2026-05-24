use crate::routes::Route;
use crate::stores::ui::settings_store::get_canonical_external_origin;
use crate::utils::nip19_urls::note_route_id_with_kind;
use nostr_sdk::nips::nip19::{Nip19, ToBech32};
use nostr_sdk::prelude::*;

pub fn route_for_naddr(
    kind: u16,
    naddr: String,
    _pubkey: PublicKey,
    _identifier: String,
) -> Option<Route> {
    match kind {
        30009 | 30023 | 30040 | 30054 | 30078 | 30311 | 30402 | 30405 | 30617
        | 30818 | 30030..=30033 | 31922 | 31923 | 32123 | 30067 | 34235 | 34236
        | 36787 | 38383 | 39089 => Some(Route::AddressViewer { address: naddr }),
        _ => None,
    }
}

pub fn route_for_event(event: &Event) -> Route {
    let hex_id = event.id.to_hex();

    if event.kind.is_addressable() {
        if let Some(coord) = event.coordinate() {
            if let Ok(naddr) = coord.to_bech32() {
                return Route::AddressViewer { address: naddr };
            }
        }
    }

    Route::AddressViewer {
        address: note_route_id_with_kind(
            &hex_id,
            Some(&event.pubkey.to_hex()),
            Some(event.kind),
        ),
    }
}

pub fn share_url_for_event(event: &Event) -> String {
    let route = route_for_event(event);
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
        30067 => "Pinboard",
        34139 => "Playlist",
        36787 => "Track",
        38383 => "P2P Order",
        _ => "Content",
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub enum ContentSection {
    Home,
    Dms,
    Videos,
    CashuWallet,
    Music,
    Podcast,
    Radio,
    Nips,
    Badges,
    Packs,
    Code,
    P2P,
    Chats,
    Community,
    Groups,
    Topics,
    Events,
    Recipes,
    Pinboards,
    Wiki,
    Publications,
    Shop,
    Blossom,
    Bible,
}

pub fn section_from_address(address: &str) -> Option<ContentSection> {
    let nip19 = Nip19::from_bech32(address).ok()?;
    let kind = match &nip19 {
        Nip19::Event(nevent) => nevent.kind?,
        Nip19::Coordinate(naddr) => naddr.coordinate.kind,
        _ => return None,
    };
    section_from_kind(kind.as_u16())
}

pub fn section_from_kind(kind: u16) -> Option<ContentSection> {
    match kind {
        21 | 22 | 34235 | 34236 => Some(ContentSection::Videos),
        30311 => Some(ContentSection::Videos),
        30009 => Some(ContentSection::Badges),
        30023 => None,
        30040 => Some(ContentSection::Publications),
        30054 | 30078 => Some(ContentSection::Podcast),
        30402 | 30405 => Some(ContentSection::Shop),
        30617 | 1621 | 1622 | 1617 => Some(ContentSection::Code),
        30818 => Some(ContentSection::Wiki),
        31922 | 31923 => Some(ContentSection::Events),
        32123 => Some(ContentSection::Music),
        30067 => Some(ContentSection::Pinboards),
        36787 => Some(ContentSection::Music),
        38383 => Some(ContentSection::P2P),
        39089 => Some(ContentSection::Packs),
        30030..=30033 => None,
        1 | 6 | 1059 | 1068 | 1111 => None,
        20 => None,
        1040 => None,
        _ => None,
    }
}
