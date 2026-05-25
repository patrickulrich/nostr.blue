use dioxus::prelude::*;
use nostr_sdk::PublicKey;

use crate::components::chess::{
    ChallengeCard, ChessCard,
};
use crate::routes::Route;
use crate::stores::chess::filter_builder;
use crate::stores::chess::jester::{JesterContent, JESTER_CONTENT_KIND_START};
use crate::stores::chess::lobby_state::CHESS_LOBBY;
use crate::stores::chess::types::{ChessChallenge, PublicGame};

#[component]
pub fn ChessHome() -> Element {
    let nav = navigator();
    let my_pubkey = crate::stores::auth_store::AUTH_STATE.read().pubkey.clone().and_then(|pk| PublicKey::from_hex(&pk).ok());

    use_future(move || {
        async move {
            CHESS_LOBBY.write().is_loading = true;
            let filter = filter_builder::start_event_filter();
            if let Ok(events) = crate::stores::nostr_client::fetching::fetch_events_from_connected_relays(
                filter,
                std::time::Duration::from_secs(10),
            ).await {
                let mut challenges = vec![];
                let mut public_games = vec![];

                for event in events {
                    let content = match JesterContent::parse(&event.content) {
                        Some(c) => c,
                        None => continue,
                    };
                    if content.kind != JESTER_CONTENT_KIND_START {
                        continue;
                    }

                    let game_id = event.id;
                    let challenger_pubkey = event.pubkey;
                    let opponent_pubkey = event.tags.iter()
                        .find(|t| t.kind() == nostr_sdk::TagKind::p())
                        .and_then(|t| t.content())
                        .and_then(|pk| PublicKey::from_hex(pk).ok());

                    let challenger_color = match content.player_color.as_deref() {
                        Some("black") => rschess::Color::Black,
                        _ => rschess::Color::White,
                    };

                    challenges.push(ChessChallenge {
                        event_id: event.id,
                        game_id,
                        challenger_pubkey,
                        opponent_pubkey,
                        challenger_color,
                        created_at: event.created_at.as_secs(),
                    });

                    let (white_pk, black_pk) = if challenger_color == rschess::Color::White {
                        (challenger_pubkey, opponent_pubkey)
                    } else {
                        (opponent_pubkey.unwrap_or(challenger_pubkey), Some(challenger_pubkey))
                    };

                    public_games.push(PublicGame {
                        game_id,
                        white_pubkey: white_pk,
                        black_pubkey: black_pk,
                        move_count: content.history.len(),
                        last_move_at: event.created_at.as_secs(),
                        is_active: opponent_pubkey.is_none() || content.history.is_empty(),
                    });
                }

                CHESS_LOBBY.write().challenges = challenges;
                CHESS_LOBBY.write().public_games = public_games;
            }
            CHESS_LOBBY.write().is_loading = false;
        }
    });

    let challenges = CHESS_LOBBY.read().challenges.clone();
    let public_games = CHESS_LOBBY.read().public_games.clone();
    let is_loading = CHESS_LOBBY.read().is_loading;

    rsx! {
        div { class: "max-w-2xl mx-auto px-4 py-6 space-y-6",
            div { class: "flex items-center justify-between",
                div { class: "flex items-center gap-3",
                    h1 { class: "text-2xl font-bold text-foreground", "♟ Chess" }
                }
                Link {
                    to: Route::ChessGameNew {},
                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-xl text-sm font-medium hover:bg-primary/90 transition",
                    "+ New Game"
                }
            }

            if is_loading {
                div { class: "flex justify-center py-8",
                    div { class: "animate-spin w-6 h-6 border-2 border-primary border-t-transparent rounded-full" }
                }
            }

            // Incoming challenges
            if let Some(pk) = my_pubkey {
                {
                    let incoming: Vec<_> = challenges.iter().filter(|c| c.is_directed_at(&pk)).cloned().collect();
                    if !incoming.is_empty() {
                        rsx! {
                            div { class: "space-y-3",
                                h2 { class: "text-lg font-semibold text-foreground", "Incoming Challenges" }
                                for challenge in incoming {
                                    {
                                        let game_id_hex = challenge.game_id.to_hex();
                                        rsx! {
                                            ChallengeCard {
                                                challenge,
                                                on_accept: move |_| {
                                                    let _ = nav.push(Route::ChessGameDetail {
                                                        game_id: game_id_hex.clone(),
                                                    });
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }

            // Open challenges
            {
                let open: Vec<_> = if let Some(pk) = my_pubkey {
                    challenges.iter().filter(|c| c.is_open() && !c.is_from(&pk)).cloned().collect()
                } else {
                    challenges.iter().filter(|c| c.is_open()).cloned().collect()
                };
                if !open.is_empty() {
                    rsx! {
                        div { class: "space-y-3",
                            h2 { class: "text-lg font-semibold text-foreground", "Open Games" }
                            div { class: "space-y-2",
                                for game in open {
                                    {
                                        let gid = game.game_id.to_hex();
                                        let challenger = crate::utils::format::truncate_pubkey(&game.challenger_pubkey.to_hex());
                                        let color_label = match game.challenger_color {
                                            rschess::Color::White => "White",
                                            rschess::Color::Black => "Black",
                                        };
                                        rsx! {
                                            Link {
                                                to: Route::ChessGameDetail { game_id: gid },
                                                class: "block rounded-xl border border-border bg-card p-3 hover:bg-accent/5 transition",
                                                div { class: "flex items-center justify-between",
                                                    span { class: "text-sm text-foreground", {challenger} }
                                                    span { class: "text-xs text-muted-foreground", "plays {color_label}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if !is_loading {
                    rsx! {
                        div { class: "text-center py-8 space-y-2",
                            p { class: "text-muted-foreground", "No open games right now." }
                            p { class: "text-sm text-muted-foreground", "Create one to get started!" }
                        }
                    }
                } else {
                    rsx! {}
                }
            }

            // Recent public games
            if !public_games.is_empty() {
                div { class: "space-y-3",
                    h2 { class: "text-lg font-semibold text-foreground", "Recent Games" }
                    div { class: "grid grid-cols-1 gap-2",
                        for game in public_games.iter() {
                            ChessCard { game: game.clone() }
                        }
                    }
                }
            }
        }
    }
}
