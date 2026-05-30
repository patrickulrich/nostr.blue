use crate::components::code::repo_card::CodeRepoCardCompact;
use crate::components::live::stream_card::LiveStreamCard;
use crate::components::{
    EventCardCompact, P2POrderCard, PhotoCard, PollCard, VideoCard, VoiceMessageCard,
};
use crate::routes::Route;
use crate::stores::calendar_store::UnifiedEvent;
use crate::stores::nostr_client;
use crate::stores::nostr_music::{parse_playlist_event, parse_track_event};
use crate::stores::pin_boards_store::parse_pinboard_event;
use crate::stores::profiles;
use crate::stores::publication_store::parse_publication_index;
use crate::utils::nip34::{Issue, PullRequest, Repository};
use crate::utils::nip52::parse_calendar_event;
use crate::utils::nip53::{parse_meeting_room_event, parse_meeting_space, LiveActivityEvent};
use crate::utils::nip54::parse_wiki_article;
use crate::utils::nip58::parse_badge_definition;
use crate::utils::nip69::parse_p2p_order;
use crate::utils::nip99::{parse_collection, parse_product, parse_review};
use crate::utils::nkbip03::parse_citation;
use crate::utils::podcast::parse_podcast_episode;
use crate::utils::recipe::{extract_metadata as extract_recipe_metadata, is_recipe_event};
use dioxus::prelude::*;
use nostr_sdk::nips::nip19::Nip19;
use nostr_sdk::{Event, EventId, Filter, FromBech32, Kind, Metadata};

use super::RichContent;
use super::minicards::*;
use crate::utils::validation::is_valid_http_url;

#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

#[cfg(feature = "web")]
const INTERACTIVE_ELEMENT_SELECTOR: &str =
    "a, button, input, textarea, select, summary, [role='button'], [role='link']:not([data-embedded-note]), [contenteditable='true'], video, audio, iframe, [data-interactive]";

#[component]
pub fn MentionRenderer(mention: String) -> Element {
    let lower = mention.to_lowercase();
    let identifier = lower.strip_prefix("nostr:").unwrap_or(&lower);
    let pubkey_result: Option<nostr_sdk::PublicKey> =
        Nip19::from_bech32(identifier)
            .ok()
            .and_then(|nip19| match nip19 {
                Nip19::Pubkey(pk) => Some(pk),
                Nip19::Profile(profile) => {
                    if !profile.relays.is_empty() {
                        let urls: Vec<String> = profile.relays.iter().map(|r| r.to_string()).collect();
                        crate::stores::relay::coverage::record_user_relays(
                            &profile.public_key.to_hex(), &urls,
                        );
                    }
                    Some(profile.public_key)
                }
                _ => None,
            });
    let cached_metadata = pubkey_result
        .as_ref()
        .and_then(|pk| profiles::get_profile(&pk.to_hex()));
    let mut metadata = use_signal(move || cached_metadata);
    use_effect(move || {
        if metadata.read().is_some() {
            return;
        }
        if let Some(pubkey) = pubkey_result {
            let pubkey_hex = pubkey.to_hex();
            let pk_hex_bg = pubkey_hex.clone();
            spawn(async move {
                let _ = crate::stores::relay::coverage::resolve_user_relays(
                    &pk_hex_bg,
                    crate::stores::relay::coverage::RelayPurpose::Write,
                ).await;
            });
            spawn(async move {
                match profiles::fetch_profile(pubkey_hex).await {
                    Ok(profile) => {
                        metadata.set(Some(profiles::profile_to_metadata(&profile)));
                    }
                    Err(e) => {
                        log::debug!("Failed to fetch profile for mention: {}", e);
                    }
                }
            });
        }
    });
    if let Some(pubkey) = pubkey_result {
        let pubkey_str = pubkey.to_hex();
        let display = if let Some(meta) = metadata.read().as_ref() {
            if let Some(display_name) = &meta.display_name {
                format!("@{}", display_name)
            } else if let Some(name) = &meta.name {
                format!("@{}", name)
            } else if pubkey_str.len() > 16 {
                format!(
                    "@{}...{}",
                    &pubkey_str[..8],
                    &pubkey_str[pubkey_str.len() - 4..],
                )
            } else {
                format!("@{}", pubkey_str)
            }
        } else if pubkey_str.len() > 16 {
            format!(
                "@{}...{}",
                &pubkey_str[..8],
                &pubkey_str[pubkey_str.len() - 4..],
            )
        } else {
            format!("@{}", pubkey_str)
        };
        rsx! {
            Link {
                to: Route::AddressViewer {
                    address: crate::utils::nip19_urls::profile_route_id(&pubkey.to_hex()),
                },
                class: "text-foreground hover:text-foreground/70 font-medium hover:underline",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                "{display}"
            }
        }
    } else {
        rsx! {
            span { class: "text-muted-foreground font-medium", "{mention}" }
        }
    }
}

/// Try to extract event ID from a nevent string even when SDK parsing fails
/// This handles cases where the nevent has invalid relay URLs (e.g., empty strings)
/// by using lower-level bech32 decoding and manually parsing the TLV data
fn try_extract_event_id_from_nevent(identifier: &str) -> Option<EventId> {
    use bech32::Hrp;
    if !identifier.starts_with("nevent1") {
        return None;
    }
    let (hrp, data) = bech32::decode(identifier).ok()?;
    if hrp != Hrp::parse("nevent").ok()? {
        return None;
    }
    let mut pos = 0;
    while pos + 2 <= data.len() {
        let tlv_type = data[pos];
        let tlv_len = data[pos + 1] as usize;
        if pos + 2 + tlv_len > data.len() {
            break;
        }
        if tlv_type == 0 && tlv_len == 32 {
            let event_id_bytes: [u8; 32] = data[pos + 2..pos + 2 + 32].try_into().ok()?;
            return EventId::from_byte_array(event_id_bytes).into();
        }
        pos += 2 + tlv_len;
    }
    None
}

#[component]
pub fn EventMentionRenderer(mention: String) -> Element {
    let lower = mention.to_lowercase();
    let identifier = lower.strip_prefix("nostr:").unwrap_or(&lower);
    let nip19_result = Nip19::from_bech32(identifier).ok();
    if matches!(&nip19_result, Some(Nip19::Coordinate(_))) {
        return rsx! {
            NaddrMentionRenderer { mention: mention.clone() }
        };
    }
    let parsed_event: Option<(EventId, Vec<String>, Option<Kind>)> = nip19_result.and_then(|nip19| match nip19 {
        Nip19::Event(nevent) => {
            let relays: Vec<String> = nevent.relays.iter().map(|r| r.to_string()).collect();
            Some((nevent.event_id, relays, nevent.kind))
        }
        Nip19::EventId(id) => Some((id, Vec::new(), None)),
        _ => None,
    });
    let (event_id_result, relay_hints, kind_hint) = if let Some((id, relays, k)) = parsed_event {
        (Some(id), relays, k)
    } else if let Some(id) = try_extract_event_id_from_nevent(identifier) {
        (Some(id), Vec::new(), None)
    } else {
        (None, Vec::new(), None)
    };
    let mut embedded_event = use_signal(|| None::<Event>);
    let mut author_metadata = use_signal(|| None::<Metadata>);
    use_effect(move || {
        if let Some(event_id) = event_id_result {
            let relay_hints_clone = relay_hints.clone();
            spawn(async move {
                let event_filter = Filter::new().id(event_id).limit(1);
                let fetch_result = if !relay_hints_clone.is_empty() {
                    if let Some(client) = nostr_client::get_client() {
                        let relay_urls: Vec<nostr_sdk::Url> = relay_hints_clone
                            .iter()
                            .filter_map(|r| nostr_sdk::Url::parse(r).ok())
                            .collect();
                        if !relay_urls.is_empty() {
                            nostr_client::ensure_relays_ready(&client).await;
                            client
                                .fetch_events_from(
                                    relay_urls,
                                    event_filter.clone(),
                                    std::time::Duration::from_secs(5),
                                )
                                .await
                                .map(|events| events.into_iter().collect::<Vec<_>>())
                                .ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                let events = match fetch_result {
                    Some(events) if !events.is_empty() => events,
                    _ => nostr_client::fetch_events_aggregated(
                        event_filter,
                        std::time::Duration::from_secs(5),
                    )
                    .await
                    .unwrap_or_default(),
                };
                if let Some(event) = events.into_iter().next() {
                    let author_pubkey = event.pubkey;
                    embedded_event.set(Some(event));
                    let pk_hex = author_pubkey.to_hex();
                    if let Some(meta) = profiles::get_profile(&pk_hex) {
                        author_metadata.set(Some(meta));
                    } else {
                        match profiles::fetch_profile(pk_hex).await {
                            Ok(profile) => {
                                author_metadata
                                    .set(Some(profiles::profile_to_metadata(&profile)));
                            }
                            Err(e) => {
                                log::debug!(
                                    "Failed to fetch profile for embedded note author: {}",
                                    e
                                );
                            }
                        }
                    }
                }
            });
        }
    });
    if let Some(event_id) = event_id_result {
        let has_event = embedded_event.read().is_some();
        let event_clone = embedded_event.read().clone();
        let metadata_clone = author_metadata.read().clone();
        if has_event {
            let event = event_clone.unwrap();
            let event_kind = event.kind.as_u16();
            match event_kind {
                20 => {
                    rsx! {
                        PhotoCard { event }
                    }
                }
                21 | 22 => {
                    rsx! {
                        VideoCard { event }
                    }
                }
                1040 => {
                    rsx! {
                        VoiceMessageCard { event }
                    }
                }
                1068 => {
                    rsx! {
                        div { onclick: move |e: MouseEvent| e.stop_propagation(),
                            PollCard { event }
                        }
                    }
                }
                1621 => {
                    if let Some(issue) = Issue::from_event(&event) {
                        rsx! {
                            {render_issue_minicard(&issue)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_note(&event, metadata_clone.as_ref())}
                        }
                    }
                }
                1622 => {
                    if let Some(pr) = PullRequest::from_event(&event) {
                        rsx! {
                            {render_pr_minicard(&pr)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_note(&event, metadata_clone.as_ref())}
                        }
                    }
                }
                6 => {
                    rsx! {
                        {render_repost_minicard(&event)}
                    }
                }
                1111 => {
                    rsx! {
                        {render_comment_minicard(&event, metadata_clone.as_ref())}
                    }
                }
                30..=33 => {
                    if let Ok(citation) = parse_citation(&event) {
                        rsx! {
                            {render_citation_minicard(&citation)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_note(&event, metadata_clone.as_ref())}
                        }
                    }
                }
                40 => {
                    // NIP-28 channel creation
                    rsx! {
                        {render_channel_minicard(&event, &event.id.to_hex())}
                    }
                }
                64 => {
                    rsx! {
                        {render_chess_pgn_minicard(&event)}
                    }
                }
                _ => {
                    rsx! {
                        {render_embedded_note(&event, metadata_clone.as_ref())}
                    }
                }
            }
        } else {
            let event_str = event_id.to_hex();
            let short = if event_str.len() > 16 {
                format!(
                    "note:{}...{}",
                    &event_str[..8],
                    &event_str[event_str.len() - 4..],
                )
            } else {
                format!("note:{}", event_str)
            };
            rsx! {
                Link {
                    to: Route::AddressViewer {
                        address: crate::utils::nip19_urls::note_route_id_with_kind(&event_id.to_hex(), None, kind_hint),
                    },
                    class: "text-foreground hover:text-foreground/70 font-medium hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "{short}"
                }
            }
        }
    } else {
        rsx! {
            span { class: "text-muted-foreground font-medium", "{mention}" }
        }
    }
}

pub(super) fn render_embedded_note(event: &Event, metadata: Option<&Metadata>) -> Element {
    let event_id = event.id.to_hex();
    let content = event.content.clone();
    let tags = event.tags.iter().cloned().collect();
    let pubkey = event.pubkey;
    let pubkey_str = pubkey.to_hex();
    let event_id_nav = event_id.clone();
    let event_id_click = event_id.clone();
    let pubkey_str_click = pubkey_str.clone();
    let kind_nav = event.kind;
    let kind_click = event.kind;
    let display_name = if let Some(meta) = metadata {
        meta.display_name
            .clone()
            .or_else(|| meta.name.clone())
            .unwrap_or_else(|| {
                format!(
                    "{}...{}",
                    &pubkey_str[..8],
                    &pubkey_str[pubkey_str.len() - 4..]
                )
            })
    } else {
        format!(
            "{}...{}",
            &pubkey_str[..8],
            &pubkey_str[pubkey_str.len() - 4..]
        )
    };
    rsx! {
        div {
            class: "block my-2 bg-card border border-border rounded-lg p-4 hover:bg-accent/10 transition cursor-pointer",
            "data-embedded-note": "true",
            role: "link",
            tabindex: "0",
            onkeydown: move |evt: KeyboardEvent| {
                let activate = matches!(evt.key(), Key::Enter);
                if !activate { return; }
                evt.stop_propagation();
                #[cfg(feature = "web")]
                {
                    if let Some(target) = evt.data.as_web_event().target() {
                        if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                            if element.closest(INTERACTIVE_ELEMENT_SELECTOR).ok().flatten().is_some() {
                                return;
                            }
                        }
                    }
                }
                evt.prevent_default();
                navigator().push(Route::AddressViewer {
                    address: crate::utils::nip19_urls::note_route_id_with_kind(&event_id_nav, Some(&pubkey_str), Some(kind_nav)),
                });
            },
            onclick: move |_evt: MouseEvent| {
                _evt.stop_propagation();
                #[cfg(feature = "web")]
                {
                    if let Some(target) = _evt.data.as_web_event().target() {
                        if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                            if element.closest(INTERACTIVE_ELEMENT_SELECTOR).ok().flatten().is_some() {
                                return;
                            }
                        }
                    }
                }
                navigator().push(Route::AddressViewer {
                    address: crate::utils::nip19_urls::note_route_id_with_kind(&event_id_click, Some(&pubkey_str_click), Some(kind_click)),
                });
            },
            div { class: "flex items-center gap-2 mb-2",
                if let Some(meta) = metadata {
                    if let Some(picture) = meta.picture.as_ref().filter(|u| is_valid_http_url(u)) {
                        img {
                            class: "w-8 h-8 rounded-full",
                            src: "{picture}",
                            alt: "Avatar",
                        }
                    } else {
                        div { class: "w-8 h-8 rounded-full bg-accent flex items-center justify-center text-foreground text-xs font-bold",
                            "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                } else {
                    div { class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center text-muted-foreground text-xs",
                        "?"
                    }
                }
                span { class: "font-semibold text-sm", "{display_name}" }
            }
            RichContent {
                content,
                tags,
                collapsible: true,
                interactive_media: true,
            }
        }
    }
}

#[component]
pub fn NaddrMentionRenderer(mention: String) -> Element {
    let lower = mention.to_lowercase();
    let identifier = lower.strip_prefix("nostr:").unwrap_or(&lower);
    let coord_data = nostr_sdk::nips::nip19::Nip19Coordinate::from_bech32(identifier)
        .ok()
        .map(|coord| {
            let relay_hints: Vec<String> = coord.relays.iter().map(|r| r.to_string()).collect();
            (
                coord.public_key.to_hex(),
                coord.identifier.clone(),
                coord.kind.as_u16(),
                relay_hints,
            )
        });
    let mut article_event = use_signal(|| None::<Event>);
    let mut author_metadata = use_signal(|| None::<Metadata>);
    let mut loading = use_signal(|| true);
    let coord_data_for_effect = coord_data.clone();
    use_effect(move || {
        if let Some((ref pubkey, ref ident, kind, ref relays)) = coord_data_for_effect {
            let pubkey = pubkey.clone();
            let ident = ident.clone();
            let relay_hints = relays.clone();
            spawn(async move {
                loading.set(true);
                match crate::stores::nostr_client::fetch_event_by_coordinate_with_relays(
                    kind,
                    pubkey.clone(),
                    ident,
                    relay_hints,
                )
                .await
                {
                    Ok(Some(event)) => {
                        let author_pubkey = event.pubkey;
                        article_event.set(Some(event));
                        let pk_hex = author_pubkey.to_hex();
                        if let Some(meta) = profiles::get_profile(&pk_hex) {
                            author_metadata.set(Some(meta));
                        } else {
                            match profiles::fetch_profile(pk_hex).await {
                                Ok(profile) => {
                                    author_metadata
                                        .set(Some(profiles::profile_to_metadata(&profile)));
                                }
                                Err(e) => {
                                    log::debug!(
                                        "Failed to fetch profile for naddr author: {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        log::warn!("Article not found for coordinate");
                    }
                    Err(e) => {
                        log::error!("Failed to fetch article: {}", e);
                    }
                }
                loading.set(false);
            });
        }
    });
    if let Some(ref coord_ref) = coord_data {
        let (ref _pubkey, ref _ident, ref kind, ref _relays) = coord_ref;
        let kind = *kind;
        let naddr_for_link = identifier.to_string();
        let has_event = article_event.read().is_some();
        let event_clone = article_event.read().clone();
        let metadata_clone = author_metadata.read().clone();
        if has_event {
            let event = event_clone.unwrap();
            const LIVE_EVENT: u16 = 30311;
            const ARTICLE: u16 = 30023;
            const GIT_REPO: u16 = 30617;
            const P2P_ORDER: u16 = 38383;
            const DATE_CALENDAR: u16 = 31922;
            const TIME_CALENDAR: u16 = 31923;
            const MEETING_SPACE: u16 = 30312;
            const MEETING_ROOM: u16 = 30313;
            const PODCAST_EPISODE: u16 = 30054;
            const WIKI_ARTICLE: u16 = 30818;
            const PUBLICATION_INDEX: u16 = 30040;
            const PINBOARD: u16 = 30067;
            const BADGE_DEFINITION: u16 = 30009;
            const PRODUCT: u16 = 30402;
            const COLLECTION: u16 = 30405;
            const REVIEW: u16 = 31555;
            const MUSIC_TRACK: u16 = 36787;
            const PLAYLIST: u16 = 34139;
            match kind {
                LIVE_EVENT => {
                    rsx! {
                        div { onclick: move |e: MouseEvent| e.stop_propagation(),
                            LiveStreamCard { event }
                        }
                    }
                }
                ARTICLE => {
                    if is_recipe_event(&event) {
                        let recipe_meta = extract_recipe_metadata(&event);
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_recipe_minicard(&recipe_meta, &naddr_clone, &event)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                DATE_CALENDAR | TIME_CALENDAR => {
                    if let Ok(cal_event) = parse_calendar_event(&event) {
                        let unified = UnifiedEvent::Calendar(cal_event);
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                EventCardCompact { event: unified }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                MEETING_SPACE => {
                    if let Ok(space) = parse_meeting_space(&event) {
                        let unified = UnifiedEvent::Live(LiveActivityEvent::Space(space));
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                EventCardCompact { event: unified }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                MEETING_ROOM => {
                    if let Ok(room) = parse_meeting_room_event(&event) {
                        let unified = UnifiedEvent::Live(LiveActivityEvent::Meeting(room));
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                EventCardCompact { event: unified }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                GIT_REPO => {
                    if let Some(repo) = Repository::from_event(&event) {
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                CodeRepoCardCompact { repo }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PODCAST_EPISODE => {
                    if let Ok(episode) = parse_podcast_episode(&event) {
                        let episode_title = episode.title.clone();
                        rsx! {
                            Link {
                                to: Route::PodcastNostrDetail {
                                    naddr: naddr_for_link.clone(),
                                },
                                class: "bg-card border border-border rounded-lg p-4 flex items-center gap-2 hover:bg-accent/50 transition",
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                svg {
                                    class: "w-8 h-8 text-muted-foreground shrink-0",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z",
                                    }
                                }
                                div { class: "flex-1 min-w-0",
                                    p { class: "font-medium truncate", "{episode_title}" }
                                    p { class: "text-xs text-muted-foreground", "Podcast Episode" }
                                }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                P2P_ORDER => {
                    if let Ok(order) = parse_p2p_order(&event) {
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                P2POrderCard { order }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                WIKI_ARTICLE => {
                    if let Ok(wiki) = parse_wiki_article(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_wiki_minicard(&wiki, &naddr_clone, &event)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PRODUCT => {
                    if let Ok(product) = parse_product(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_product_minicard(&product, &naddr_clone, &event)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                BADGE_DEFINITION => {
                    if let Ok(badge) = parse_badge_definition(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_badge_minicard(&badge, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                MUSIC_TRACK => {
                    if let Ok(track) = parse_track_event(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_track_minicard(&track, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PLAYLIST => {
                    if let Ok(playlist) = parse_playlist_event(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_playlist_minicard(&playlist, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PUBLICATION_INDEX => {
                    if let Some(pub_index) = parse_publication_index(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_publication_minicard(&pub_index, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PINBOARD => {
                    if let Some(board) = parse_pinboard_event(&event, None) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_pinboard_minicard(&board, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                COLLECTION => {
                    if let Ok(collection) = parse_collection(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_collection_minicard(&collection, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                REVIEW => {
                    if let Ok(review) = parse_review(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_review_minicard(&review, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                _ => {
                    rsx! {
                        {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                    }
                }
            }
        } else if *loading.read() {
            rsx! {
                div { class: "my-2 bg-card border border-border rounded-lg p-4 animate-pulse",
                    div { class: "h-4 bg-muted rounded w-3/4 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/2" }
                }
            }
        } else {
            let fallback_class =
                "text-foreground hover:text-foreground/70 font-medium hover:underline";
            let (ref pubkey_hex, ref ident, ..) = coord_ref;
            let route = if let Ok(pk) = nostr_sdk::PublicKey::from_hex(pubkey_hex) {
                crate::utils::route_for_kind::route_for_naddr(
                    kind,
                    naddr_for_link.clone(),
                    pk,
                    ident.clone(),
                )
            } else {
                None
            };
            if let Some(route) = route {
                let label = crate::utils::route_for_kind::content_label_for_kind(kind);
                rsx! {
                    Link {
                        to: route,
                        class: fallback_class,
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        "{label}"
                    }
                }
            } else {
                rsx! {
                    span { class: "text-muted-foreground font-medium", "📄 {mention}" }
                }
            }
        }
    } else {
        rsx! {
            span { class: "text-muted-foreground font-medium", "{mention}" }
        }
    }
}

 fn route_by_event_id_and_kind(hex: &str, kind: Kind, author_hex: Option<&str>) -> Route {
    if kind.as_u16() == 40 {
        return Route::ChatDetail {
            channel_id: hex.to_string(),
        };
    }
    Route::AddressViewer {
        address: crate::utils::nip19_urls::note_route_id_with_kind(hex, author_hex, Some(kind)),
    }
 }

#[component]
pub fn TextLinkNaddr(mention: String) -> Element {
    let lower = mention.to_lowercase();
    let identifier = lower.strip_prefix("nostr:").unwrap_or(&lower);
    let coord_data =
        nostr_sdk::nips::nip19::Nip19Coordinate::from_bech32(identifier).ok().map(|coord| {
            (
                coord.kind.as_u16(),
                coord.public_key.to_owned(),
                coord.identifier.clone(),
            )
        });
    if let Some((kind, pubkey, ident)) = coord_data {
        let naddr = identifier.to_string();
        if let Some(route) =
            crate::utils::route_for_kind::route_for_naddr(kind, naddr, pubkey, ident)
        {
            let label = crate::utils::route_for_kind::content_label_for_kind(kind);
            return rsx! {
                Link {
                    to: route,
                    class: "text-foreground hover:text-muted-foreground underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "{label}"
                }
            };
        }
    }
    rsx! { span { class: "text-muted-foreground", "{mention}" } }
}

#[component]
pub fn TextLinkMention(mention: String) -> Element {
    let lower = mention.to_lowercase();
    let identifier = lower.strip_prefix("nostr:").unwrap_or(&lower);
    let nip19_result = Nip19::from_bech32(identifier).ok();

    if matches!(&nip19_result, Some(Nip19::Coordinate(_))) {
        return rsx! { TextLinkNaddr { mention: mention.clone() } };
    }

    let parsed: Option<(EventId, Option<Kind>, Option<nostr_sdk::PublicKey>)> =
        nip19_result.and_then(|n| match n {
            Nip19::Event(ne) => Some((ne.event_id, ne.kind, ne.author)),
            Nip19::EventId(id) => Some((id, None, None)),
            _ => None,
        });

    let (event_id, tlv_kind, author) = if let Some(t) = parsed {
        t
    } else if let Some(id) = try_extract_event_id_from_nevent(identifier) {
        (id, None, None)
    } else {
        return rsx! { span { class: "text-muted-foreground", "{mention}" } };
    };

    let event_id_hex = event_id.to_hex();
    let author_hex = author.as_ref().map(|p| p.to_hex());

    if let Some(kind) = tlv_kind {
        let route =
            route_by_event_id_and_kind(&event_id_hex, kind, author_hex.as_deref());
        let label = crate::utils::route_for_kind::content_label_for_kind(kind.as_u16());
        return rsx! {
            Link {
                to: route,
                class: "text-foreground hover:text-muted-foreground underline",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                "{label}"
            }
        };
    }

    let mut resolved = use_signal(|| None::<(Route, &'static str)>);

    {
        let eid = event_id;
        use_effect(move || {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    if let Ok(Some(event)) = client.database().event_by_id(&eid).await {
                        let route = crate::utils::route_for_kind::route_for_event(&event);
                        let label = crate::utils::route_for_kind::content_label_for_event(&event);
                        resolved.set(Some((route, label)));
                    }
                }
            });
        });
    }

    if let Some((route, label)) = resolved.cloned() {
        rsx! {
            Link {
                to: route,
                class: "text-foreground hover:text-muted-foreground underline",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                "{label}"
            }
        }
    } else {
        let short = if event_id_hex.len() > 16 {
            format!(
                "{}...{}",
                &event_id_hex[..8],
                &event_id_hex[event_id_hex.len() - 4..]
            )
        } else {
            event_id_hex.clone()
        };
        rsx! {
            Link {
                to: Route::AddressViewer {
                    address: crate::utils::nip19_urls::note_route_id(
                        &event_id_hex,
                        author_hex.as_deref(),
                    ),
                },
                class: "text-foreground hover:text-muted-foreground underline",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                "{short}"
            }
        }
    }
}
