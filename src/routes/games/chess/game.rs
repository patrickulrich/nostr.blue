use dioxus::prelude::*;
use nostr_sdk::{EventId, PublicKey};

use crate::components::chess::ChessBoard;
use crate::hooks::use_relay_subscription;
use crate::routes::Route;
use crate::stores::chess::game_state::GameState;
use crate::stores::chess::jester::{
    JesterContent, JESTER_CONTENT_KIND_MOVE, JESTER_CONTENT_KIND_START,
};
use crate::stores::chess::types::ViewerRole;

#[component]
pub fn ChessGameDetail(game_id: String) -> Element {
    let game_state: Signal<GameState> = use_signal(GameState::new_game);
    let viewer_role = use_signal(|| ViewerRole::Spectator);
    let is_loading = use_signal(|| true);
    let mut game_title = use_signal(String::new);
    let start_event_id: Signal<Option<EventId>> = use_signal(|| None);
    let head_event_id: Signal<Option<EventId>> = use_signal(|| None);
    let opponent_pubkey: Signal<Option<PublicKey>> = use_signal(|| None);
    let publish_error: Signal<Option<String>> = use_signal(|| None);
    let mut show_resign_confirm = use_signal(|| false);
    let seen_event_ids: Signal<Vec<EventId>> = use_signal(Vec::new);

    let event_id = EventId::from_hex(&game_id).ok();
    let nav = navigator();

    let my_pubkey = crate::stores::auth_store::AUTH_STATE
        .read()
        .pubkey
        .clone()
        .and_then(|pk| PublicKey::from_hex(&pk).ok());

    let is_interactive = !matches!(*viewer_role.read(), ViewerRole::Spectator)
        && !game_state.read().is_game_over();

    let sub_filter = event_id.map(|eid| {
        crate::stores::chess::filter_builder::game_events_filter(&eid)
    });

    let mut gs_for_sub = game_state;
    let mut head_for_sub = head_event_id;
    let mut seen_for_sub = seen_event_ids;
    let mut start_for_sub = start_event_id;
    let viewer_for_sub = viewer_role;

    use_relay_subscription(sub_filter, move |event| {
        let eid = event.id;
        if seen_for_sub.read().contains(&eid) {
            return;
        }
        seen_for_sub.write().push(eid);

        let content = match JesterContent::parse(&event.content) {
            Some(c) => c,
            None => return,
        };

        if content.kind == JESTER_CONTENT_KIND_START {
            if start_for_sub.read().is_none() {
                start_for_sub.set(Some(eid));
            }
            return;
        }

        if content.kind != JESTER_CONTENT_KIND_MOVE {
            return;
        }

        let current_history_len = gs_for_sub.read().san_list().len();
        if content.history.len() <= current_history_len {
            return;
        }

        let mut gs = gs_for_sub.write();
        gs.go_to_end();
        for san in &content.history[current_history_len..] {
            if gs.make_move_san(san).is_err() {
                break;
            }
        }
        drop(gs);

        head_for_sub.set(Some(eid));

        if content.result.is_some() || gs_for_sub.read().is_game_over() {
            let result = content.result.unwrap_or_default();
            let termination = content.termination.unwrap_or_else(|| "normal".to_string());
            let mut title = game_title.read().clone();
            if !title.contains("Game Over") {
                title = format!("{} — Game Over: {}", title, result);
                game_title.set(title);
            }
            if *viewer_for_sub.read() != ViewerRole::Spectator {
                let _ = termination;
            }
        }
    });

    use_future(move || {
        let mut game_state = game_state;
        let mut viewer_role = viewer_role;
        let mut is_loading = is_loading;
        let mut game_title = game_title;
        let mut start_event_id = start_event_id;
        let mut head_event_id = head_event_id;
        let mut opponent_pubkey = opponent_pubkey;
        async move {
            let Some(eid) = event_id else {
                is_loading.set(false);
                return;
            };

            let filter = crate::stores::chess::filter_builder::game_events_filter(&eid);
            let result = crate::stores::nostr_client::fetching::fetch_events_from_connected_relays(
                filter,
                std::time::Duration::from_secs(10),
            )
            .await;

            match result {
                Ok(events) => {
                    let events_vec: Vec<_> = events.into_iter().collect();

                    if let Some(my_pk) = my_pubkey {
                        if let Ok(reconstructed) =
                            crate::stores::chess::state_reconstructor::reconstruct(
                                &events_vec,
                                &my_pk,
                            )
                        {
                            game_state.set(reconstructed.game_state);
                            viewer_role.set(reconstructed.viewer_role);
                            start_event_id.set(Some(reconstructed.start_event_id));

                            let mut latest_head: Option<EventId> = None;
                            let mut best_len = 0;
                            for ev in &events_vec {
                                if let Some(c) = JesterContent::parse(&ev.content) {
                                    if c.kind == JESTER_CONTENT_KIND_MOVE
                                        && c.history.len() > best_len
                                    {
                                        best_len = c.history.len();
                                        latest_head = Some(ev.id);
                                    }
                                }
                            }
                            head_event_id.set(latest_head.or(Some(reconstructed.start_event_id)));

                            let opponent = if reconstructed.white_pubkey == my_pk {
                                reconstructed.black_pubkey
                            } else {
                                Some(reconstructed.white_pubkey)
                            };
                            opponent_pubkey.set(opponent);

                            let w = crate::utils::format::truncate_pubkey(
                                &reconstructed.white_pubkey.to_hex(),
                            );
                            let b = reconstructed
                                .black_pubkey
                                .map(|p| crate::utils::format::truncate_pubkey(&p.to_hex()))
                                .unwrap_or_else(|| "Waiting...".to_string());
                            let status = if reconstructed.is_game_over {
                                let r = reconstructed.result.unwrap_or_default();
                                format!("{} vs {} — Game Over: {}", w, b, r)
                            } else {
                                format!("{} vs {}", w, b)
                            };
                            game_title.set(status);
                        }
                    } else {
                        let mut gs = GameState::new_game();
                        let mut best_history: Vec<String> = vec![];
                        let mut best_head: Option<EventId> = None;
                        let mut found_start: Option<EventId> = None;

                        for event in &events_vec {
                            if let Some(content) = JesterContent::parse(&event.content) {
                                if content.kind == JESTER_CONTENT_KIND_START {
                                    found_start = Some(event.id);
                                } else if content.kind == JESTER_CONTENT_KIND_MOVE
                                    && content.history.len() > best_history.len()
                                {
                                    best_history = content.history;
                                    best_head = Some(event.id);
                                }
                            }
                        }

                        for san in &best_history {
                            let _ = gs.make_move_san(san);
                        }
                        game_state.set(gs);
                        start_event_id.set(found_start);
                        head_event_id.set(best_head.or(found_start));
                        game_title.set("Spectating".to_string());
                    }
                }
                Err(_) => {
                    game_title.set("Failed to load game".to_string());
                }
            }
            is_loading.set(false);
        }
    });

    let perspective = match *viewer_role.read() {
        ViewerRole::BlackPlayer => rschess::Color::Black,
        _ => rschess::Color::White,
    };

    let on_move_handler: Option<EventHandler<String>> = if is_interactive {
        let mut game_state = game_state;
        let mut head_event_id = head_event_id;
        let mut publish_error = publish_error;
        let mut game_title = game_title;
        Some(EventHandler::new(move |san: String| {
            let snapshot = game_state.read().snapshot();
            let start = *start_event_id.read();
            let head = *head_event_id.read();
            let opponent = *opponent_pubkey.read();
            let role = *viewer_role.read();

            let (Some(start_eid), Some(head_eid), Some(opp_pk)) = (start, head, opponent) else {
                log::warn!("Cannot publish move: missing game context");
                return;
            };

            let gs = game_state.read();
            let fen = gs.fen();
            let history = gs.san_list().to_vec();
            let is_over = gs.is_game_over();
            let game_result = gs.game_result();
            drop(gs);

            let content = JesterContent::new_move(&fen, &san, &history);

            spawn(async move {
                match crate::stores::chess::publish::publish_move(
                    &start_eid,
                    &head_eid,
                    &opp_pk,
                    content,
                )
                .await
                {
                    Ok(new_event_id) => {
                        head_event_id.set(Some(new_event_id));
                        publish_error.set(None);
                    }
                    Err(e) => {
                        log::warn!("Failed to publish move: {}", e);
                        game_state.write().restore_snapshot(snapshot);
                        publish_error.set(Some(format!("Failed to publish move: {}", e)));
                    }
                }

                if is_over {
                    let result_str = match game_result {
                        Some(r) => format!("{}", r),
                        None => "*".to_string(),
                    };
                    let termination = if game_state.read().is_checkmate() {
                        "checkmate"
                    } else if game_state.read().is_stalemate() {
                        "stalemate"
                    } else {
                        "normal"
                    };
                    let gs = game_state.read();
                    let fen = gs.fen();
                    let history = gs.san_list().to_vec();
                    let last_san = history.last().cloned().unwrap_or_default();
                    drop(gs);

                    let end_content = JesterContent::new_end(
                        &fen,
                        &last_san,
                        &history,
                        &result_str,
                        termination,
                    );

                    let current_head = *head_event_id.read();
                    if let Some(h) = current_head {
                        let _ = crate::stores::chess::publish::publish_game_end(
                            &start_eid,
                            &h,
                            &opp_pk,
                            end_content,
                        )
                        .await;
                    }

                    let mut title = game_title.read().clone();
                    if !title.contains("Game Over") {
                        title = format!("{} — Game Over: {}", title, result_str);
                        game_title.set(title);
                    }

                    let white_pk = match role {
                        ViewerRole::WhitePlayer => {
                            crate::stores::auth_store::AUTH_STATE
                                .read()
                                .pubkey
                                .clone()
                                .and_then(|pk| PublicKey::from_hex(&pk).ok())
                                .unwrap_or_else(|| {
                                    PublicKey::from_hex(
                                        "0000000000000000000000000000000000000000000000000000000000000001",
                                    )
                                    .unwrap()
                                })
                        }
                        ViewerRole::BlackPlayer => opp_pk,
                        ViewerRole::Spectator => {
                            return;
                        }
                    };
                    let black_pk = match role {
                        ViewerRole::BlackPlayer => {
                            crate::stores::auth_store::AUTH_STATE
                                .read()
                                .pubkey
                                .clone()
                                .and_then(|pk| PublicKey::from_hex(&pk).ok())
                                .unwrap_or_else(|| {
                                    PublicKey::from_hex(
                                        "0000000000000000000000000000000000000000000000000000000000000001",
                                    )
                                    .unwrap()
                                })
                        }
                        ViewerRole::WhitePlayer => opp_pk,
                        ViewerRole::Spectator => {
                            return;
                        }
                    };

                    let tags = vec![
                        ("White".to_string(), crate::utils::format::truncate_pubkey(&white_pk.to_hex())),
                        ("Black".to_string(), crate::utils::format::truncate_pubkey(&black_pk.to_hex())),
                    ];

                    let gs = game_state.read();
                    if let Ok(pgn_text) = gs.to_pgn(tags) {
                        let _ = crate::stores::chess::publish::publish_pgn_game(
                            pgn_text,
                            Some(opp_pk),
                        )
                        .await;
                    }
                }
            });
        }))
    } else {
        None
    };

    rsx! {
        div { class: "max-w-2xl mx-auto px-4 py-4 space-y-4",
            // Header
            div { class: "flex items-center gap-3",
                button {
                    class: "p-2 hover:bg-accent rounded-lg transition",
                    onclick: move |_| { let _ = nav.push(Route::ChessHome {}); },
                    "←"
                }
                h1 { class: "text-lg font-semibold text-foreground truncate",
                    {game_title.read().clone()}
                }
            }

            if *is_loading.read() {
                div { class: "flex justify-center py-16",
                    div { class: "animate-spin w-8 h-8 border-2 border-primary border-t-transparent rounded-full" }
                }
            } else {
                // Black player info (top)
                div { class: "flex items-center gap-2 px-1",
                    div { class: "w-4 h-4 rounded-full bg-gray-800 border border-border" }
                    span { class: "text-sm text-foreground", "Black" }
                }

                // Board
                ChessBoard {
                    game_state: game_state,
                    interactive: is_interactive,
                    viewer_role: *viewer_role.read(),
                    perspective: perspective,
                    on_move: on_move_handler,
                }

                // White player info (bottom)
                div { class: "flex items-center gap-2 px-1",
                    div { class: "w-4 h-4 rounded-full bg-white border border-border" }
                    span { class: "text-sm text-foreground", "White" }
                }

                // Move history
                div { class: "mt-4 rounded-xl border border-border bg-card p-3",
                    h3 { class: "text-sm font-medium text-foreground mb-2", "Moves" }
                    div { class: "text-sm text-muted-foreground font-mono leading-relaxed",
                        {
                            let movetext = game_state.read().movetext();
                            if movetext.is_empty() {
                                "No moves yet".to_string()
                            } else {
                                movetext
                            }
                        }
                    }
                }

                // Publish error
                if let Some(err) = publish_error.read().as_ref() {
                    div { class: "rounded-xl border border-red-500/30 bg-red-500/10 p-3",
                        p { class: "text-sm text-red-500", {err.clone()} }
                    }
                }

                // Actions
                div { class: "flex gap-2 mt-4",
                    if !game_state.read().is_game_over() && is_interactive {
                        button {
                            class: "px-4 py-2 border border-red-500/30 text-red-500 rounded-xl text-sm hover:bg-red-500/10 transition",
                            disabled: *show_resign_confirm.read(),
                            onclick: move |_| {
                                show_resign_confirm.set(true);
                            },
                            "Resign"
                        }
                    }
                }

                // Resign confirmation
                if *show_resign_confirm.read() {
                    div { class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
                        div { class: "bg-card border border-border rounded-xl p-6 mx-4 max-w-sm w-full space-y-4",
                            h3 { class: "text-lg font-semibold text-foreground", "Confirm Resign" }
                            p { class: "text-sm text-muted-foreground",
                                "Are you sure you want to resign? This cannot be undone."
                            }
                            div { class: "flex gap-3",
                                button {
                                    class: "flex-1 px-4 py-2 border border-border rounded-xl text-sm hover:bg-accent/5 transition",
                                    onclick: move |_| {
                                        show_resign_confirm.set(false);
                                    },
                                    "Cancel"
                                }
                                button {
                                    class: "flex-1 px-4 py-2 bg-red-500 text-white rounded-xl text-sm hover:bg-red-600 transition",
                                    onclick: {
                                        let mut show_resign_confirm = show_resign_confirm;
                                        let mut game_title = game_title;
                                        let mut publish_error = publish_error;
                                        move |_| {
                                            show_resign_confirm.set(false);
                                            let start = *start_event_id.read();
                                            let head = *head_event_id.read();
                                            let opponent = *opponent_pubkey.read();
                                            let role = *viewer_role.read();

                                            let (Some(start_eid), Some(head_eid), Some(opp_pk)) = (start, head, opponent) else {
                                                publish_error.set(Some("Cannot resign: missing game context".to_string()));
                                                return;
                                            };

                                            let result = match role {
                                                ViewerRole::WhitePlayer => "0-1",
                                                ViewerRole::BlackPlayer => "1-0",
                                                ViewerRole::Spectator => return,
                                            };

                                            let gs = game_state.read();
                                            let fen = gs.fen();
                                            let history = gs.san_list().to_vec();
                                            let last_san = history.last().cloned().unwrap_or_default();
                                            drop(gs);

                                            let end_content = JesterContent::new_end(
                                                &fen,
                                                &last_san,
                                                &history,
                                                result,
                                                "resign",
                                            );

                                            spawn(async move {
                                                match crate::stores::chess::publish::publish_game_end(
                                                    &start_eid,
                                                    &head_eid,
                                                    &opp_pk,
                                                    end_content,
                                                )
                                                .await
                                                {
                                                    Ok(_) => {
                                                        let mut title = game_title.read().clone();
                                                        if !title.contains("Game Over") {
                                                            title = format!("{} — Game Over: {}", title, result);
                                                            game_title.set(title);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        publish_error.set(Some(format!("Failed to resign: {}", e)));
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    "Resign"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
