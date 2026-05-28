use crate::components::client_initializing::ClientInitializing;
use crate::components::viewers::*;
use crate::routes::Route;
use crate::stores::nostr_client::fetching::{fetch_event_targeted, parse_event_id};
use crate::stores::nostr_client::CLIENT_INITIALIZED;
use crate::utils::route_for_kind::content_label_for_kind;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[component]
pub fn AddressViewer(address: String) -> Element {
    let mut state: Signal<AddressState> = use_signal(|| AddressState::Loading);

    use_effect(use_reactive!(
        |address| {
            state.set(AddressState::Loading);
            let client_initialized = *CLIENT_INITIALIZED.read();
            if !client_initialized {
                return;
            }
            spawn(async move {
                match resolve_address(&address).await {
                    Ok(resolved) => state.set(resolved),
                    Err(e) => state.set(AddressState::Error(e)),
                }
            });
        }
    ));

    let client_initialized = *CLIENT_INITIALIZED.read();
    if !client_initialized {
        return rsx! { ClientInitializing {} };
    }

    {
        use crate::stores::ui::back_navigation::ADDRESS_WIDE_MODE;
        let s = state.cloned();
        *ADDRESS_WIDE_MODE.write() = matches!(
            s,
            AddressState::Video { .. }
                | AddressState::LiveStream { .. }
                | AddressState::Nest { .. }
                | AddressState::MusicTrack { .. }
                | AddressState::Playlist { .. }
                | AddressState::RadioStation { .. }
                | AddressState::CodeRepo { .. }
                | AddressState::CodeIssue { .. }
                | AddressState::CodePull { .. }
                | AddressState::CalendarEvent { .. }
                | AddressState::Badge { .. }
                | AddressState::Pack { .. }
                | AddressState::Pinboard { .. }
                | AddressState::Publication { .. }
                | AddressState::Wiki { .. }
                | AddressState::ShopProduct { .. }
                | AddressState::ShopCollection { .. }
                | AddressState::P2POrder { .. }
                | AddressState::Community { .. }
                | AddressState::Photo { .. }
                | AddressState::Place { .. }
        );
    }

    let current_state = state.cloned();
    match current_state {
        AddressState::Loading => rsx! {
            div { class: "min-h-screen flex items-center justify-center p-4",
                div { class: "text-center",
                    div { class: "text-4xl mb-4 animate-spin", "🔄" }
                    h2 { class: "text-xl font-semibold mb-2", "Loading..." }
                    p { class: "text-muted-foreground text-sm font-mono break-all", "{address}" }
                }
            }
        },
        AddressState::Error(msg) => rsx! {
            div { class: "min-h-screen flex items-center justify-center p-4",
                div { class: "text-center max-w-md",
                    div { class: "text-6xl mb-4", "❌" }
                    h2 { class: "text-2xl font-bold mb-4", "Not Found" }
                    p { class: "text-muted-foreground mb-4", "{msg}" }
                    div { class: "p-3 bg-muted rounded-lg mb-6",
                        p { class: "text-xs font-mono break-all", "{address}" }
                    }
                    Link {
                        to: Route::Home { list: String::new() },
                        class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                        "← Go Home"
                    }
                }
            }
        },
        AddressState::Redirect(route) => {
            navigator().replace(*route);
            rsx! { div { class: "min-h-screen" } }
        }
        AddressState::Profile { pubkey } => {
            rsx! { ProfileViewer { pubkey } }
        }
        AddressState::Photo { photo_id } => {
            rsx! { PhotoViewer { photo_id } }
        }
        AddressState::Video { video_id } => {
            rsx! { VideoViewer { video_id } }
        }
        AddressState::VoiceMessage { voice_id } => {
            rsx! { VoiceViewer { voice_id } }
        }
        AddressState::Note { note_id, from_voice } => {
            rsx! { NoteViewer { note_id, from_voice } }
        }
        AddressState::Poll { noteid } => {
            rsx! { PollViewer { noteid } }
        }
        AddressState::LiveStream { note_id } => {
            rsx! { LiveStreamViewer { note_id } }
        }
        AddressState::CodeIssue { note_id } => {
            rsx! { CodeIssueViewer { note_id } }
        }
        AddressState::CodePull { note_id } => {
            rsx! { CodePullViewer { note_id } }
        }
        AddressState::ChessPgn { note_id } => {
            rsx! { ChessPgnViewer { note_id } }
        }
        AddressState::CodeDiscussion { note_id } => {
            rsx! { CodeDiscussionViewer { note_id } }
        }
        AddressState::CodeSnippet { note_id } => {
            rsx! { CodeSnippetViewer { note_id } }
        }
        AddressState::Article { naddr } => {
            rsx! { ArticleViewer { naddr } }
        }
        AddressState::CodeRepo { naddr } => {
            rsx! { CodeRepoViewer { naddr } }
        }
        AddressState::Badge { naddr } => {
            rsx! { BadgeViewer { naddr } }
        }
        AddressState::Pack { naddr } => {
            rsx! { PackViewer { naddr } }
        }
        AddressState::Pinboard { naddr } => {
            rsx! { PinboardViewer { naddr } }
        }
        AddressState::Publication { naddr } => {
            rsx! { PublicationViewer { naddr } }
        }
        AddressState::CalendarEvent { naddr, from } => {
            rsx! { CalendarEventViewer { naddr, from } }
        }
        AddressState::ShopProduct { naddr } => {
            rsx! { ShopProductViewer { naddr } }
        }
        AddressState::ShopCollection { naddr } => {
            rsx! { ShopCollectionViewer { naddr } }
        }
        AddressState::Nest { naddr } => {
            rsx! { NestViewer { naddr } }
        }
        AddressState::Community { naddr } => {
            rsx! { CommunityViewer { naddr } }
        }
        AddressState::PodcastNostr { naddr } => {
            rsx! { PodcastNostrViewer { naddr } }
        }
        AddressState::PodcastEpisode { naddr } => {
            rsx! { PodcastEpisodeViewer { naddr } }
        }
        AddressState::Playlist { naddr } => {
            rsx! { PlaylistViewer { naddr } }
        }
        AddressState::Citation { naddr } => {
            rsx! { CitationViewer { naddr } }
        }
        AddressState::P2POrder { naddr } => {
            rsx! { P2POrderViewer { naddr } }
        }
        AddressState::Recipe { naddr } => {
            rsx! { RecipeViewer { naddr } }
        }
        AddressState::RadioStation { naddr } => {
            rsx! { RadioViewer { naddr } }
        }
        AddressState::MusicTrack { track_id } => {
            rsx! { MusicTrackViewer { track_id } }
        }
        AddressState::Wiki { npub, identifier } => {
            rsx! { WikiViewer { npub, identifier } }
        }
        AddressState::CodeUserProfile { pubkey } => {
            rsx! { CodeProfileViewer { pubkey } }
        }
        AddressState::WikiAuthor { pubkey } => {
            rsx! { WikiAuthorViewer { pubkey } }
        }
        AddressState::Place { naddr } => {
            rsx! { PlaceViewer { naddr } }
        }
        AddressState::FetchingEvent => rsx! {
            div { class: "min-h-screen flex items-center justify-center p-4",
                div { class: "text-center",
                    div { class: "text-4xl mb-4 animate-spin", "🔄" }
                    h2 { class: "text-xl font-semibold mb-2", "Resolving content type..." }
                    p { class: "text-muted-foreground text-sm font-mono break-all", "{address}" }
                }
            }
        },
    }
}

#[derive(Clone, PartialEq)]
#[allow(dead_code)]
enum AddressState {
    Loading,
    FetchingEvent,
    Error(String),
    Redirect(Box<Route>),
    Profile { pubkey: String },
    Photo { photo_id: String },
    Video { video_id: String },
    VoiceMessage { voice_id: String },
    Note { note_id: String, from_voice: Option<String> },
    Poll { noteid: String },
    LiveStream { note_id: String },
    CodeIssue { note_id: String },
    CodePull { note_id: String },
    CodeDiscussion { note_id: String },
    CodeSnippet { note_id: String },
    Article { naddr: String },
    CodeRepo { naddr: String },
    Badge { naddr: String },
    Pack { naddr: String },
    Pinboard { naddr: String },
    Publication { naddr: String },
    CalendarEvent { naddr: String, from: Option<String> },
    ChessPgn { note_id: String },
    ShopProduct { naddr: String },
    ShopCollection { naddr: String },
    Nest { naddr: String },
    PodcastNostr { naddr: String },
    PodcastEpisode { naddr: String },
    Playlist { naddr: String },
    Citation { naddr: String },
    P2POrder { naddr: String },
    Recipe { naddr: String },
    RadioStation { naddr: String },
    MusicTrack { track_id: String },
    Community { naddr: String },
    Wiki { npub: String, identifier: String },
    CodeUserProfile { pubkey: String },
    WikiAuthor { pubkey: String },
    Place { naddr: String },
}

async fn resolve_address(address: &str) -> std::result::Result<AddressState, String> {
    if address.starts_with("nsec") || address.starts_with("ncryptsec") {
        return Err(
            "🔒 This is a private key! Never share it or paste it into websites.".to_string(),
        );
    }

    if address.starts_with("nrelay") {
        return Err("Relay URLs (nrelay) are not yet supported.".to_string());
    }

    match Nip19::from_bech32(address) {
        Ok(nip19) => dispatch_nip19(nip19, address).await,
        Err(bech32_err) => dispatch_raw_coordinate(address, bech32_err),
    }
}

async fn dispatch_nip19(nip19: Nip19, address: &str) -> std::result::Result<AddressState, String> {
    match nip19 {
        Nip19::Pubkey(pubkey) => {
            Ok(AddressState::Profile {
                pubkey: pubkey.to_bech32().unwrap_or_else(|_| pubkey.to_hex()),
            })
        }
        Nip19::Profile(profile) => {
            if !profile.relays.is_empty() {
                let urls: Vec<String> = profile.relays.iter().map(|r| r.to_string()).collect();
                crate::stores::relay::coverage::record_user_relays(
                    &profile.public_key.to_hex(),
                    &urls,
                );
            }
            Ok(AddressState::Profile {
                pubkey: profile.to_bech32().unwrap_or_else(|_| profile.public_key.to_hex()),
            })
        }
        Nip19::EventId(event_id) => {
            let id_str = event_id.to_bech32().unwrap_or_else(|_| event_id.to_hex());
            dispatch_event_by_kind(event_id, None, &id_str).await
        }
        Nip19::Event(nevent) => {
            if !nevent.relays.is_empty() {
                let urls: Vec<String> = nevent.relays.iter().map(|r| r.to_string()).collect();
                if let Some(author) = &nevent.author {
                    crate::stores::relay::coverage::record_user_relays(
                        &author.to_hex(),
                        &urls,
                    );
                }
            }
            let id_str = nevent.to_bech32().unwrap_or_else(|_| nevent.event_id.to_hex());
            dispatch_event_by_kind(nevent.event_id, nevent.kind, &id_str).await
        }
        Nip19::Coordinate(coord) => {
            let kind = coord.coordinate.kind.as_u16();
            let naddr = address.to_string();
            dispatch_naddr(kind, naddr, &coord)
        }
        Nip19::Secret(_) => {
            Err("🔒 Private key detected. Keep it safe!".to_string())
        }
        Nip19::EncryptedSecret(_) => {
            Err("🔐 Encrypted private key. Import it safely via Settings.".to_string())
        }
    }
}

fn dispatch_raw_coordinate(
    address: &str,
    bech32_err: nostr_sdk::nips::nip19::Error,
) -> std::result::Result<AddressState, String> {
    match crate::utils::nip19::parse_naddr(address) {
        Ok(parsed) => {
            let pubkey = PublicKey::from_hex(&parsed.pubkey)
                .map_err(|e| format!("Invalid pubkey in coordinate: {}", e))?;
            let coord = Coordinate::new(Kind::from(parsed.kind), pubkey)
                .identifier(parsed.identifier);
            let nip19_coord = Nip19Coordinate::new(coord, vec![]);
            dispatch_naddr(parsed.kind, address.to_string(), &nip19_coord)
        }
        Err(_) => Err(format!(
            "Failed to decode '{}': {}",
            &address[..address.len().min(20)],
            bech32_err
        )),
    }
}

fn dispatch_naddr(kind: u16, naddr: String, coord: &Nip19Coordinate) -> std::result::Result<AddressState, String> {
    match kind {
        30009 => Ok(AddressState::Badge { naddr }),
        30023 => {
            let identifier = coord.coordinate.identifier.as_str();
            if is_recipe_identifier(identifier) {
                Ok(AddressState::Recipe { naddr })
            } else {
                Ok(AddressState::Article { naddr })
            }
        }
        30040 => Ok(AddressState::Publication { naddr }),
        30054 => Ok(AddressState::PodcastEpisode { naddr }),
        30078 => Ok(AddressState::PodcastNostr { naddr }),
        30311 => Ok(AddressState::LiveStream { note_id: naddr }),
        30312 => Ok(AddressState::Nest { naddr }),
        30402 => Ok(AddressState::ShopProduct { naddr }),
        30405 => Ok(AddressState::ShopCollection { naddr }),
        30617 => Ok(AddressState::CodeRepo { naddr }),
        30818 => {
            let npub = coord.coordinate.public_key.to_bech32().unwrap_or_else(|_| coord.coordinate.public_key.to_hex());
            let identifier = coord.coordinate.identifier.to_string();
            Ok(AddressState::Wiki { npub, identifier })
        }
        30030..=30033 => Ok(AddressState::Citation { naddr }),
        31922 | 31923 => Ok(AddressState::CalendarEvent { naddr, from: None }),
        30313 => Ok(AddressState::CalendarEvent { naddr, from: None }),
        31237 => Ok(AddressState::RadioStation { naddr }),
        34139 => Ok(AddressState::Playlist { naddr }),
        34235 | 34236 => Ok(AddressState::Video { video_id: naddr }),
        36787 => Ok(AddressState::MusicTrack { track_id: naddr }),
        30067 => Ok(AddressState::Pinboard { naddr }),
        38383 => Ok(AddressState::P2POrder { naddr }),
        39089 => Ok(AddressState::Pack { naddr }),
        34550 => Ok(AddressState::Community { naddr }),
        37515 => Ok(AddressState::Place { naddr }),
        _ => Err(format!(
            "Addressable event kind {} ({}) is not yet supported.",
            kind,
            content_label_for_kind(kind)
        )),
    }
}

fn is_recipe_identifier(identifier: &str) -> bool {
    identifier.starts_with("recipe:") || identifier.contains("nostrcooking")
}

async fn dispatch_event_by_kind(
    event_id: EventId,
    known_kind: Option<Kind>,
    id_str: &str,
) -> std::result::Result<AddressState, String> {
    if let Some(kind) = known_kind {
        return dispatch_by_event_kind(kind.as_u16(), id_str);
    }

    if let Some(client) = crate::stores::nostr_client::get_client() {
        if let Ok(Some(event)) = client.database().event_by_id(&event_id).await {
            return dispatch_by_event_kind(event.kind.as_u16(), id_str);
        }
    }

    if let Some(parsed) = parse_event_id(&event_id.to_hex()) {
        match fetch_event_targeted(parsed, std::time::Duration::from_secs(10)).await {
            Ok(Some(event)) => dispatch_by_event_kind(event.kind.as_u16(), id_str),
            Ok(None) => Err("Event not found".to_string()),
            Err(e) => Err(e),
        }
    } else {
        Ok(AddressState::Note {
            note_id: id_str.to_string(),
            from_voice: None,
        })
    }
}

fn dispatch_by_event_kind(kind: u16, id_str: &str) -> std::result::Result<AddressState, String> {
    match kind {
        1 | 6 | 1059 | 1111 => Ok(AddressState::Note {
            note_id: id_str.to_string(),
            from_voice: None,
        }),
        20 => Ok(AddressState::Photo {
            photo_id: id_str.to_string(),
        }),
        21 | 22 | 34235 | 34236 => Ok(AddressState::Video {
            video_id: id_str.to_string(),
        }),
        1040 => Ok(AddressState::VoiceMessage {
            voice_id: id_str.to_string(),
        }),
        1068 => Ok(AddressState::Poll {
            noteid: id_str.to_string(),
        }),
        1621 => Ok(AddressState::CodeIssue {
            note_id: id_str.to_string(),
        }),
        1622 => Ok(AddressState::CodePull {
            note_id: id_str.to_string(),
        }),
        64 => Ok(AddressState::ChessPgn {
            note_id: id_str.to_string(),
        }),
        36787 => Ok(AddressState::MusicTrack {
            track_id: id_str.to_string(),
        }),
        _ => Ok(AddressState::Note {
            note_id: id_str.to_string(),
            from_voice: None,
        }),
    }
}
