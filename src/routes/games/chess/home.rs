use dioxus::prelude::*;
use nostr_sdk::{EventId, PublicKey, TagKind};

use crate::components::chess::{ChallengeCard, ChessCard};
use crate::routes::Route;
use crate::stores::chess::filter_builder;
use crate::stores::chess::jester::{JesterContent, JESTER_CONTENT_KIND_START};
use crate::stores::chess::game_state::GameState;
use crate::stores::chess::lobby_state::{ActiveGame, CHESS_LOBBY};
use crate::stores::chess::state_reconstructor;
use crate::stores::chess::types::{ChessChallenge, PublicGame, CompletedGame, ViewerRole};

fn resolve_display_name(pubkey: &PublicKey) -> String {
    let pk_hex = pubkey.to_hex();
    if let Some(profile) = crate::stores::profiles::get_cached_profile(&pk_hex) {
        let name = profile.display_name.clone().or(profile.name.clone());
        if let Some(n) = name {
            if !n.is_empty() {
                return n;
            }
        }
    }
    crate::utils::format::truncate_pubkey(&pk_hex)
}

#[component]
pub fn ChessHome() -> Element {
    let nav = navigator();
    let my_pubkey = crate::stores::auth_store::AUTH_STATE.read().pubkey.clone().and_then(|pk| PublicKey::from_hex(&pk).ok());

    use_future(move || {
        let my_pubkey = my_pubkey;
        async move {
            CHESS_LOBBY.write().is_loading = true;

            let open_filter = filter_builder::start_event_filter();
            let personal_filters = my_pubkey.as_ref().map(filter_builder::personal_filters);
            let pgn_filter = filter_builder::pgn_games_filter();

            let open_result = crate::stores::nostr_client::fetching::fetch_chess_events(
                open_filter,
                std::time::Duration::from_secs(10),
            ).await;

            let personal_result: Option<Vec<nostr_sdk::Event>> = if let Some(filters) = personal_filters {
                let mut all_events = vec![];
                for f in filters {
                    if let Ok(events) = crate::stores::nostr_client::fetching::fetch_chess_events(
                        f,
                        std::time::Duration::from_secs(10),
                    ).await {
                        all_events.extend(events);
                    }
                }
                if all_events.is_empty() { None } else { Some(all_events) }
            } else {
                None
            };

            let pgn_result = crate::stores::nostr_client::fetching::fetch_chess_events(
                pgn_filter,
                std::time::Duration::from_secs(5),
            ).await.ok();

            let mut challenges = vec![];
            let mut public_games = vec![];
            let mut personal_game_ids: Vec<EventId> = vec![];

            if let Ok(events) = open_result {
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
                        .find(|t| t.kind() == TagKind::p())
                        .and_then(|t| t.content())
                        .and_then(|pk| PublicKey::from_hex(pk).ok());

                    let challenger_color = match content.player_color.as_deref() {
                        Some("black") => rschess::Color::Black,
                        _ => rschess::Color::White,
                    };

                    let challenge = ChessChallenge {
                        event_id: event.id,
                        game_id,
                        challenger_pubkey,
                        opponent_pubkey,
                        challenger_color,
                        created_at: event.created_at.as_secs(),
                    };

                    let is_my_game = my_pubkey.as_ref().is_some_and(|pk| {
                        *pk == challenger_pubkey || opponent_pubkey.as_ref() == Some(pk)
                    });

                    challenges.push(challenge);

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

                    if is_my_game {
                        personal_game_ids.push(game_id);
                    }
                }
            }

            if let Some(events) = personal_result {
                let mut seen_ids = std::collections::HashSet::new();
                for event in events {
                    let content = match JesterContent::parse(&event.content) {
                        Some(c) => c,
                        None => continue,
                    };
                    if content.kind != JESTER_CONTENT_KIND_START {
                        continue;
                    }

                    let game_id = event.id;
                    if personal_game_ids.contains(&game_id) || !seen_ids.insert(game_id) {
                        continue;
                    }
                    personal_game_ids.push(game_id);

                    let challenger_pubkey = event.pubkey;
                    let opponent_pubkey = event.tags.iter()
                        .find(|t| t.kind() == TagKind::p())
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
                }
            }

            let mut active_games = vec![];
            let mut completed_games = vec![];

            if !personal_game_ids.is_empty() {
                let filters = filter_builder::active_game_filters(&personal_game_ids);
                let mut all_vec: Vec<nostr_sdk::Event> = vec![];
                for f in filters {
                    if let Ok(events) = crate::stores::nostr_client::fetching::fetch_chess_events(
                        f,
                        std::time::Duration::from_secs(10),
                    ).await {
                        all_vec.extend(events);
                    }
                }

                if !all_vec.is_empty() {

                    for game_id in &personal_game_ids {
                        let gid_hex = game_id.to_hex();
                        let game_events: Vec<_> = all_vec.iter()
                            .filter(|e| {
                                e.tags.iter().any(|t| {
                                    t.kind() == TagKind::e()
                                        && t.content() == Some(gid_hex.as_str())
                                })
                            })
                            .cloned()
                            .collect();

                        if game_events.is_empty() {
                            continue;
                        }

                        let start_ev = state_reconstructor::find_start_event(&game_events);
                        let best_move = state_reconstructor::find_best_move_event(&game_events);

                        let Some(start_ev) = start_ev else { continue };

                        let start_content = match JesterContent::parse(&start_ev.content) {
                            Some(c) if c.kind == JESTER_CONTENT_KIND_START => c,
                            _ => continue,
                        };

                        let challenger_color = match start_content.player_color.as_deref() {
                            Some("black") => rschess::Color::Black,
                            _ => rschess::Color::White,
                        };

                        let opponent_from_ptag = state_reconstructor::get_ptag_opponent(start_ev);
                        let opponent_from_move = best_move.map(|e| e.pubkey).filter(|p| *p != start_ev.pubkey);
                        let opponent_pk = opponent_from_move.or(opponent_from_ptag);

                        let (white_pk, black_pk) = if challenger_color == rschess::Color::White {
                            (start_ev.pubkey, opponent_pk)
                        } else {
                            (opponent_pk.unwrap_or(start_ev.pubkey), Some(start_ev.pubkey))
                        };

                        let move_count = best_move
                            .and_then(|e| JesterContent::parse(&e.content))
                            .map(|c| c.history.len())
                            .unwrap_or(0);

                        let result = best_move
                            .and_then(|e| JesterContent::parse(&e.content))
                            .and_then(|c| c.result);

                        let is_my_turn = {
                            let my_pk = match my_pubkey {
                                Some(ref pk) => pk,
                                None => continue,
                            };
                            let viewer_color = if *my_pk == white_pk {
                                rschess::Color::White
                            } else if black_pk.as_ref() == Some(my_pk) {
                                rschess::Color::Black
                            } else {
                                continue;
                            };
                            let side_to_move = if move_count % 2 == 0 {
                                rschess::Color::White
                            } else {
                                rschess::Color::Black
                            };
                            result.is_none() && viewer_color == side_to_move
                        };

                        if let Some(ref result_str) = result {
                            completed_games.push(CompletedGame {
                                game_id: *game_id,
                                white_pubkey: white_pk,
                                black_pubkey: black_pk,
                                move_count,
                                result: result_str.clone(),
                                last_move_at: best_move.map(|e| e.created_at.as_secs()).unwrap_or(start_ev.created_at.as_secs()),
                                is_pgn: false,
                            });
                        } else if move_count > 0 {
                            active_games.push(ActiveGame {
                                game_id: *game_id,
                                start_event_id: start_ev.id,
                                white_pubkey: white_pk,
                                black_pubkey: black_pk,
                                viewer_role: if my_pubkey.as_ref() == Some(&white_pk) {
                                    ViewerRole::WhitePlayer
                                } else {
                                    ViewerRole::BlackPlayer
                                },
                                move_count,
                                last_move_san: best_move
                                    .and_then(|e| JesterContent::parse(&e.content))
                                    .and_then(|c| c.mv),
                                is_my_turn,
                                last_move_at: best_move.map(|e| e.created_at.as_secs()).unwrap_or(start_ev.created_at.as_secs()),
                            });
                        }
                    }
                }
            }

            if let Some(events) = pgn_result {
                for event in events {
                    if event.kind.as_u16() != 64 {
                        continue;
                    }
                    let game_id = event.id;
                    if completed_games.iter().any(|g| g.game_id == game_id) {
                        continue;
                    }
                    let content = &event.content;
                    if content.is_empty() {
                        continue;
                    }

                    let p_tag_pk = event.tags.iter()
                        .find(|t| t.kind() == TagKind::p())
                        .and_then(|t| t.content())
                        .and_then(|pk| PublicKey::from_hex(pk).ok());

                    let (white_pk, black_pk, result_str, pgn_move_count) =
                        if let Ok((gs, tags)) = GameState::from_pgn(content) {
                            let result_val = tags.iter().find(|(k, _)| k == "Result").map(|(_, v)| v.clone());
                            let wp = p_tag_pk.unwrap_or_else(|| {
                                PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                            });
                            (wp, None::<PublicKey>, result_val.unwrap_or_else(|| "*".to_string()), gs.san_list().len())
                        } else {
                            let wp = match p_tag_pk {
                                Some(pk) => pk,
                                None => continue,
                            };
                            (wp, None::<PublicKey>, "PGN".to_string(), 0)
                        };

                    completed_games.push(CompletedGame {
                        game_id,
                        white_pubkey: white_pk,
                        black_pubkey: black_pk,
                        move_count: pgn_move_count,
                        result: result_str,
                        last_move_at: event.created_at.as_secs(),
                        is_pgn: true,
                    });
                }
            }

            CHESS_LOBBY.write().challenges = challenges;
            CHESS_LOBBY.write().public_games = public_games;
            CHESS_LOBBY.write().active_games = active_games;
            CHESS_LOBBY.write().completed_games = completed_games;
            CHESS_LOBBY.write().is_loading = false;
        }
    });

    let challenges = CHESS_LOBBY.read().challenges.clone();
    let public_games = CHESS_LOBBY.read().public_games.clone();
    let active_games = CHESS_LOBBY.read().active_games.clone();
    let completed_games = CHESS_LOBBY.read().completed_games.clone();
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
                                        let game_id_for_accept = challenge.game_id;
                                        rsx! {
                                            ChallengeCard {
                                                challenge,
                                                on_accept: move |_| {
                                                    if let Some(pk) = my_pubkey {
                                                        CHESS_LOBBY.write().mark_challenge_accepted(&game_id_for_accept, &pk);
                                                    }
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

            // Active games
            if !active_games.is_empty() {
                div { class: "space-y-3",
                    h2 { class: "text-lg font-semibold text-foreground", "Your Games" }
                    div { class: "space-y-2",
                        for game in active_games {
                            {
                                let gid = game.game_id.to_hex();
                                let white_name = resolve_display_name(&game.white_pubkey);
                                let black_name = game.black_pubkey.as_ref()
                                    .map(resolve_display_name)
                                    .unwrap_or_else(|| "Waiting...".to_string());
                                let turn_label = if game.is_my_turn {
                                    "Your turn".to_string()
                                } else {
                                    "Opponent's turn".to_string()
                                };
                                let move_label = format!("{} moves", game.move_count);
                                rsx! {
                                    button {
                                        class: "w-full text-left block rounded-xl border border-border bg-card p-3 hover:bg-accent/5 transition",
                                        onclick: {
                                            let gid = gid.clone();
                                            move |_| {
                                                let _ = nav.push(Route::ChessGameDetail { game_id: gid.clone() });
                                            }
                                        },
                                        div { class: "flex items-center justify-between",
                                            span { class: "text-sm text-foreground", "{white_name} vs {black_name}" }
                                            span { class: "text-xs text-muted-foreground", "{move_label}" }
                                        }
                                        div { class: "flex items-center justify-between mt-1",
                                            span { class: "text-xs text-muted-foreground", "{turn_label}" }
                                        }
                                    }
                                }
                            }
                        }
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
                                        let challenger = resolve_display_name(&game.challenger_pubkey);
                                        let color_label = match game.challenger_color {
                                            rschess::Color::White => "White",
                                            rschess::Color::Black => "Black",
                                        };
                                        rsx! {
                                            button {
                                                class: "w-full text-left block rounded-xl border border-border bg-card p-3 hover:bg-accent/5 transition",
                                                onclick: {
                                                    let gid = gid.clone();
                                                    let game_id = game.game_id;
                                                    move |_| {
                                                        if let Some(pk) = my_pubkey {
                                                            CHESS_LOBBY.write().mark_challenge_accepted(&game_id, &pk);
                                                        }
                                                        let _ = nav.push(Route::ChessGameDetail { game_id: gid.clone() });
                                                    }
                                                },
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

            // Completed games
            if !completed_games.is_empty() {
                div { class: "space-y-3",
                    h2 { class: "text-lg font-semibold text-foreground", "Completed Games" }
                    div { class: "space-y-2",
                        for game in completed_games.iter().take(20) {
                            {
                                let gid = game.game_id.to_hex();
                                let white_name = resolve_display_name(&game.white_pubkey);
                                let black_name = game.black_pubkey.as_ref()
                                    .map(resolve_display_name)
                                    .unwrap_or_else(|| "Unknown".to_string());
                                let result = game.result.clone();
                                let is_pgn = game.is_pgn;
                                rsx! {
                                    button {
                                        class: "w-full text-left block rounded-xl border border-border bg-card p-3 hover:bg-accent/5 transition",
                                        onclick: {
                                            let gid = gid.clone();
                                            move |_| {
                                                if is_pgn {
                                                    let _ = nav.push(Route::ChessPgnViewer { note_id: gid.clone() });
                                                } else {
                                                    let _ = nav.push(Route::ChessGameDetail { game_id: gid.clone() });
                                                }
                                            }
                                        },
                                        div { class: "flex items-center justify-between",
                                            span { class: "text-sm text-foreground", "{white_name} vs {black_name}" }
                                            span { class: "text-xs text-muted-foreground", "{result}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
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
