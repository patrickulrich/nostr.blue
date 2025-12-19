use dioxus::prelude::*;
use crate::routes::Route;
use nostr_sdk::prelude::*;

#[component]
pub fn Nip19Handler(identifier: String) -> Element {
    let mut error_msg = use_signal(|| None::<String>);
    let mut processing = use_signal(|| true);

    // Clone identifier for use in different contexts
    let identifier_for_effect = identifier.clone();
    let identifier_for_display = identifier.clone();

    // Decode and redirect on mount
    use_effect(move || {
        let id = identifier_for_effect.clone();

        spawn(async move {
            match decode_and_redirect(&id).await {
                Ok(route) => {
                    // Navigate to the appropriate route
                    navigator().push(route);
                }
                Err(e) => {
                    error_msg.set(Some(e));
                    processing.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center p-4",

            if *processing.read() {
                div {
                    class: "text-center",
                    div {
                        class: "text-4xl mb-4 animate-spin",
                        "🔄"
                    }
                    h2 {
                        class: "text-xl font-semibold mb-2",
                        "Processing identifier..."
                    }
                    p {
                        class: "text-muted-foreground text-sm font-mono break-all",
                        "{identifier_for_display}"
                    }
                }
            } else if let Some(err) = error_msg.read().as_ref() {
                div {
                    class: "text-center max-w-md",
                    div {
                        class: "text-6xl mb-4",
                        "❌"
                    }
                    h2 {
                        class: "text-2xl font-bold mb-4",
                        "Invalid Identifier"
                    }
                    p {
                        class: "text-muted-foreground mb-4",
                        "{err}"
                    }
                    div {
                        class: "p-3 bg-muted rounded-lg mb-6",
                        p {
                            class: "text-xs font-mono break-all",
                            "{identifier_for_display}"
                        }
                    }
                    Link {
                        to: Route::Home {},
                        class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                        "← Go Home"
                    }
                }
            }
        }
    }
}

// Decode NIP-19 identifier and determine redirect route
async fn decode_and_redirect(identifier: &str) -> std::result::Result<Route, String> {
    log::info!("Decoding NIP-19 identifier: {}", identifier);

    // Handle nsec early with security warning
    if identifier.starts_with("nsec") {
        return Err("🔒 This is a private key (nsec)! Never share your private key with anyone or paste it into websites. Keep it safe!".to_string());
    }

    // Handle nrelay (relay URL)
    if identifier.starts_with("nrelay") {
        return Err("Relay URLs (nrelay) are not yet supported. Relay management coming soon.".to_string());
    }

    // Use Nip19::from_bech32 for unified type detection
    match Nip19::from_bech32(identifier) {
        Ok(nip19) => match nip19 {
            Nip19::Pubkey(pubkey) => {
                log::info!("Decoded npub: {}", pubkey);
                Ok(Route::Profile {
                    pubkey: pubkey.to_hex()
                })
            }
            Nip19::Profile(profile) => {
                log::info!("Decoded nprofile: {} with {} relay hints",
                    profile.public_key,
                    profile.relays.len()
                );
                // TODO: Could use relay hints to fetch profile data
                Ok(Route::Profile {
                    pubkey: profile.public_key.to_hex()
                })
            }
            Nip19::EventId(event_id) => {
                log::info!("Decoded note: {}", event_id);
                Ok(Route::Note {
                    note_id: event_id.to_hex(),
                    from_voice: None,
                })
            }
            Nip19::Event(nevent) => {
                log::info!("Decoded nevent: {} with {} relay hints",
                    nevent.event_id,
                    nevent.relays.len()
                );
                // TODO: Could use relay hints to fetch the event
                Ok(Route::Note {
                    note_id: nevent.event_id.to_hex(),
                    from_voice: None,
                })
            }
            Nip19::Coordinate(coord) => {
                log::info!("Decoded naddr: kind={} pubkey={} id={}",
                    coord.coordinate.kind.as_u16(),
                    coord.coordinate.public_key,
                    coord.coordinate.identifier
                );
                // Route based on event kind
                let kind = coord.coordinate.kind.as_u16();
                match kind {
                    // Long-form content (articles)
                    30023 => Ok(Route::ArticleDetail {
                        naddr: identifier.to_string()
                    }),
                    // Badge definition
                    30009 => Ok(Route::BadgeDetail {
                        naddr: identifier.to_string()
                    }),
                    // Live streams (kind 30311 uses note_id, not naddr - redirect to detail)
                    30311 => {
                        // Live stream events use note_id from event
                        // For now, return error since we need to fetch the actual event
                        Err(format!("Live stream naddr routing requires event fetch. Please use the event ID directly."))
                    },
                    // Calendar events (NIP-52)
                    31922 | 31923 => Ok(Route::CalendarEventDetail {
                        naddr: identifier.to_string(),
                        from: None
                    }),
                    // Music tracks - use MusicPlaylistDetail
                    32123 => Ok(Route::MusicPlaylistDetail {
                        naddr: identifier.to_string()
                    }),
                    // Podcast shows
                    30078 => Ok(Route::PodcastNostrDetail {
                        naddr: identifier.to_string()
                    }),
                    // Podcast episodes
                    30054 => Ok(Route::PodcastNostrEpisodeDetail {
                        naddr: identifier.to_string()
                    }),
                    // Code repositories
                    30617 => Ok(Route::CodeRepo {
                        naddr: identifier.to_string()
                    }),
                    // P2P orders (NIP-69)
                    38383 => Ok(Route::P2POrderDetail {
                        naddr: identifier.to_string()
                    }),
                    // Wiki articles (NIP-54)
                    30818 => Ok(Route::WikiDetail {
                        identifier: coord.coordinate.identifier.clone()
                    }),
                    // Publications (NKBIP-01)
                    30040 => Ok(Route::PublicationDetail {
                        naddr: identifier.to_string()
                    }),
                    // Pin Boards
                    33889 => Ok(Route::PinBoardDetail {
                        naddr: identifier.to_string()
                    }),
                    // Generic fallback for unknown kinds
                    _ => Err(format!(
                        "Addressable event kind {} is not yet supported. naddr: {}",
                        kind,
                        identifier
                    ))
                }
            }
            Nip19::Secret(_) => {
                // Should be caught earlier, but just in case
                Err("🔒 This is a private key (nsec)! Never share your private key with anyone.".to_string())
            }
        },
        Err(e) => Err(format!(
            "Failed to decode NIP-19 identifier '{}...': {}",
            identifier.chars().take(20).collect::<String>(),
            e
        ))
    }
}
