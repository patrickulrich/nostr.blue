use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::{EventId, PublicKey};

use crate::components::chess::ChessBoard;
use crate::hooks::use_relay_subscription;
use crate::routes::Route;
use crate::stores::chess::game_state::GameState;
use crate::stores::chess::jester::{
    JesterContent, JESTER_CONTENT_KIND_MOVE, JESTER_CONTENT_KIND_START,
};
use crate::stores::chess::types::ViewerRole;

const POLL_INTERVAL_SECS: u64 = 15;

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

fn apply_game_delta(
    events: &[nostr_sdk::Event],
    mut game_state: Signal<GameState>,
    mut head_event_id: Signal<Option<EventId>>,
    mut desync_warning: Signal<Option<String>>,
    mut game_title: Signal<String>,
) {
    let best = crate::stores::chess::state_reconstructor::find_best_move_event(events);
    let best = match best {
        Some(b) => b,
        None => return,
    };

    let content = match JesterContent::parse(&best.content) {
        Some(c) if c.kind == JESTER_CONTENT_KIND_MOVE => c,
        _ => return,
    };

    let current_len = game_state.read().san_list().len();
    if content.history.len() <= current_len {
        return;
    }

    {
        let mut gs = game_state.write();
        gs.go_to_end();
        for san in &content.history[current_len..] {
            if gs.make_move_san(san).is_err() {
                break;
            }
        }
    }

    head_event_id.set(Some(best.id));

    if let Some(ref reported_fen) = content.fen {
        let actual_fen = game_state.read().fen();
        if !GameState::fen_matches_lenient(&actual_fen, reported_fen) {
            desync_warning.set(Some("Board state mismatch with relay data".to_string()));
        } else {
            desync_warning.set(None);
        }
    }

    if content.result.is_some() || game_state.read().is_game_over() {
        let result = content.result.clone().unwrap_or_default();
        let title = game_title.read().clone();
        if !title.contains("Game Over") {
            game_title.set(format!("{} — Game Over: {}", title, result));
        }
    }
}

#[component]
pub fn ChessGameDetail(game_id: String) -> Element {
    let game_state: Signal<GameState> = use_signal(GameState::new_game);
    let viewer_role = use_signal(|| ViewerRole::Spectator);
    let is_loading = use_signal(|| true);
    let game_title = use_signal(String::new);
    let start_event_id: Signal<Option<EventId>> = use_signal(|| None);
    let head_event_id: Signal<Option<EventId>> = use_signal(|| None);
    let opponent_pubkey: Signal<Option<PublicKey>> = use_signal(|| None);
    let publish_error: Signal<Option<String>> = use_signal(|| None);
    let desync_warning: Signal<Option<String>> = use_signal(|| None);
    let mut show_resign_confirm = use_signal(|| false);
    let mut draw_offered_by_opponent: Signal<bool> = use_signal(|| false);
    let seen_event_ids: Signal<Vec<EventId>> = use_signal(Vec::new);

    let mut poll_task: Signal<Option<Task>> = use_signal(|| None);

    let resolved_id = crate::utils::nips::chess::parse_game_id(&game_id)
        .and_then(|hex| EventId::from_hex(&hex).ok());
    let event_id = resolved_id;
    let nav = navigator();

    let my_pubkey = crate::stores::auth_store::AUTH_STATE
        .read()
        .pubkey
        .clone()
        .and_then(|pk| PublicKey::from_hex(&pk).ok());

    let is_interactive = !matches!(*viewer_role.read(), ViewerRole::Spectator)
        && !game_state.read().is_game_over();

    let sub_filter = event_id.map(|eid| {
        crate::stores::chess::filter_builder::game_moves_subscription_filter(&eid)
    });

    let gs_for_sub = game_state;
    let head_for_sub = head_event_id;
    let mut seen_for_sub = seen_event_ids;
    let mut start_for_sub = start_event_id;
    let desync_for_sub = desync_warning;
    let viewer_for_sub = viewer_role;
    let title_for_sub = game_title;

    use_relay_subscription(sub_filter, move |event| {
        let eid = event.id;
        if seen_for_sub.read().contains(&eid) {
            return;
        }
        {
            let mut seen = seen_for_sub.write();
            seen.push(eid);
            if seen.len() > 500 {
                let excess = seen.len() - 500;
                *seen = seen.split_off(excess);
            }
        }

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

        if content.draw_offered == Some(true) {
            let role = *viewer_for_sub.read();
            if role != ViewerRole::Spectator {
                draw_offered_by_opponent.set(true);
            }
        }

        apply_game_delta(
            std::slice::from_ref(event),
            gs_for_sub,
            head_for_sub,
            desync_for_sub,
            title_for_sub,
        );
    });

    use_effect(move || {
        let mut game_state = game_state;
        let mut viewer_role = viewer_role;
        let mut is_loading = is_loading;
        let mut game_title = game_title;
        let mut start_event_id = start_event_id;
        let mut head_event_id = head_event_id;
        let mut opponent_pubkey = opponent_pubkey;
        let desync_warning = desync_warning;
        let event_id = event_id;
        let my_pubkey = my_pubkey;

        if let Some(t) = poll_task.write().take() {
            t.cancel();
        }

        let client_initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        let task = spawn(async move {
                let Some(eid) = event_id else {
                    is_loading.set(false);
                    return;
                };

                let filters = crate::stores::chess::filter_builder::game_events_filter(&eid);
                let mut all_events = vec![];
                for f in filters {
                    let fallback_filter = f.clone();
                    match crate::stores::nostr_client::fetching::fetch_chess_events(
                        f,
                        std::time::Duration::from_secs(10),
                    )
                    .await
                    {
                        Ok(events) => all_events.extend(events),
                        Err(e) => {
                            log::warn!("Chess relay fetch failed, trying connected relays: {}", e);
                            if let Ok(fallback) = crate::stores::nostr_client::fetching::fetch_events_from_connected_relays(
                                fallback_filter,
                                std::time::Duration::from_secs(10),
                            )
                            .await
                            {
                                all_events.extend(fallback);
                            }
                        }
                    }
                }

                let events_vec: Vec<_> = all_events;

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

                        let best =
                            crate::stores::chess::state_reconstructor::find_best_move_event(&events_vec);
                        let head_id = best
                            .map(|e| e.id)
                            .or(Some(reconstructed.start_event_id));
                        head_event_id.set(head_id);

                        let opponent = if reconstructed.white_pubkey == my_pk {
                            reconstructed.black_pubkey
                        } else {
                            Some(reconstructed.white_pubkey)
                        };
                        opponent_pubkey.set(opponent);

                        let w = resolve_display_name(&reconstructed.white_pubkey);
                        let b = reconstructed
                            .black_pubkey
                            .as_ref()
                            .map(resolve_display_name)
                            .unwrap_or_else(|| "Waiting...".to_string());
                        let status = if reconstructed.is_game_over {
                            let r = reconstructed.result.unwrap_or_default();
                            format!("{} vs {} — Game Over: {}", w, b, r)
                        } else {
                            format!("{} vs {}", w, b)
                        };
                        game_title.set(status);
                    } else {
                        game_title.set("Failed to reconstruct game".to_string());
                    }
                } else {
                    let mut gs = GameState::new_game();
                    let found_start =
                        crate::stores::chess::state_reconstructor::find_start_event(&events_vec)
                            .map(|e| e.id);

                    let best =
                        crate::stores::chess::state_reconstructor::find_best_move_event(&events_vec);
                    let best_history = best
                        .and_then(|e| JesterContent::parse(&e.content))
                        .map(|c| c.history)
                        .unwrap_or_default();
                    let best_head = best.map(|e| e.id);

                    for san in &best_history {
                        let _ = gs.make_move_san(san);
                    }
                    game_state.set(gs);
                    start_event_id.set(found_start);
                    head_event_id.set(best_head.or(found_start));
                    game_title.set("Spectating".to_string());
                }
                is_loading.set(false);

                if my_pubkey.is_none() || event_id.is_none() {
                    return;
                }
                let eid = event_id.unwrap();

                loop {
                    crate::platform::timer::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

                    let poll_filters = crate::stores::chess::filter_builder::game_events_filter(&eid);
                    let mut poll_events = vec![];
                    for f in poll_filters {
                        if let Ok(events) = crate::stores::nostr_client::fetching::fetch_chess_events(
                            f,
                            std::time::Duration::from_secs(5),
                        )
                        .await
                        {
                            poll_events.extend(events);
                        }
                    }
                    if !poll_events.is_empty() {
                        apply_game_delta(&poll_events, game_state, head_event_id, desync_warning, game_title);
                    }
                }
            });
        poll_task.set(Some(task));
        });

    let perspective = match *viewer_role.read() {
        ViewerRole::BlackPlayer => rschess::Color::Black,
        _ => rschess::Color::White,
    };

    let white_pubkey = {
        let role = *viewer_role.read();
        let opp = *opponent_pubkey.read();
        match role {
            ViewerRole::WhitePlayer => my_pubkey,
            ViewerRole::BlackPlayer => opp,
            ViewerRole::Spectator => None,
        }
    };
    let black_pubkey = {
        let role = *viewer_role.read();
        let opp = *opponent_pubkey.read();
        match role {
            ViewerRole::BlackPlayer => my_pubkey,
            ViewerRole::WhitePlayer => opp,
            ViewerRole::Spectator => None,
        }
    };

    let white_name = white_pubkey
        .as_ref()
        .map(resolve_display_name)
        .unwrap_or_else(|| "White".to_string());
    let black_name = black_pubkey
        .as_ref()
        .map(resolve_display_name)
        .unwrap_or_else(|| "Black".to_string());

    let on_move_handler: Option<EventHandler<String>> = if is_interactive {
        let mut game_state = game_state;
        let mut head_event_id = head_event_id;
        let mut publish_error = publish_error;
        let mut game_title = game_title;
        let mut draw_offered_by_opponent = draw_offered_by_opponent;
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

            let mut content = JesterContent::new_move(&fen, &san, &history);
            if *draw_offered_by_opponent.read() {
                content.result = Some("1/2-1/2".to_string());
                content.termination = Some("draw_agreement".to_string());
            } else if is_over {
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
                content.result = Some(result_str);
                content.termination = Some(termination.to_string());
            }

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
                        draw_offered_by_opponent.set(false);
                    }
                    Err(e) => {
                        log::warn!("Failed to publish move: {}", e);
                        game_state.write().restore_snapshot(snapshot);
                        publish_error.set(Some(format!("Failed to publish move: {}", e)));
                        return;
                    }
                }

                if is_over {
                    let result_str = match game_result {
                        Some(r) => format!("{}", r),
                        None => "*".to_string(),
                    };

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

                    let white_name = resolve_display_name(&white_pk);
                    let black_name = resolve_display_name(&black_pk);
                    let alt = format!("Chess: {} vs {} — {}", white_name, black_name, result_str);

                    let tags = vec![
                        ("White".to_string(), white_name),
                        ("Black".to_string(), black_name),
                    ];

                    let gs = game_state.read();
                    if let Ok(pgn_text) = gs.to_pgn(tags) {
                        let _ = crate::stores::chess::publish::publish_pgn_game(
                            pgn_text,
                            Some(opp_pk),
                            alt,
                            Some(start_eid),
                        )
                        .await;
                    }
                }
            });
        }))
    } else {
        None
    };

    let on_draw_offer = {
        let mut publish_error = publish_error;
        move |_| {
            let start = *start_event_id.read();
            let head = *head_event_id.read();
            let opponent = *opponent_pubkey.read();
            let (Some(start_eid), Some(head_eid), Some(opp_pk)) = (start, head, opponent) else {
                publish_error.set(Some("Cannot offer draw: missing game context".to_string()));
                return;
            };
            let gs = game_state.read();
            let fen = gs.fen();
            let history = gs.san_list().to_vec();
            let last_san = history.last().cloned().unwrap_or_default();
            drop(gs);

            let mut content = JesterContent::new_move(&fen, &last_san, &history);
            content.draw_offered = Some(true);

            spawn(async move {
                if let Err(e) = crate::stores::chess::publish::publish_move(
                    &start_eid,
                    &head_eid,
                    &opp_pk,
                    content,
                )
                .await
                {
                    publish_error.set(Some(format!("Failed to offer draw: {}", e)));
                }
            });
        }
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
                    span { class: "text-sm text-foreground", {black_name.clone()} }
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
                    span { class: "text-sm text-foreground", {white_name.clone()} }
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

                // Desync warning
                if let Some(warning) = desync_warning.read().as_ref() {
                    div { class: "rounded-xl border border-yellow-500/30 bg-yellow-500/10 p-3 space-y-2",
                        p { class: "text-sm text-yellow-500", {warning.clone()} }
                        {
                            let game_id_for_resync = game_id.clone();
                            let my_pubkey_for_resync = my_pubkey;
                            rsx! {
                                button {
                                    class: "px-3 py-1.5 rounded-lg border border-yellow-500/30 text-yellow-500 text-xs hover:bg-yellow-500/10 transition",
                                    onclick: move |_| {
                                        let eid = EventId::from_hex(&game_id_for_resync).ok();
                                        let my_pk = my_pubkey_for_resync;
                                        let mut game_state = game_state;
                                        let mut desync_warning = desync_warning;
                                        spawn(async move {
                                            let Some(event_id) = eid else { return };
                                            let resync_filters = crate::stores::chess::filter_builder::game_events_filter(&event_id);
                                            let mut resync_events = vec![];
                                            for f in resync_filters {
                                                if let Ok(events) = crate::stores::nostr_client::fetching::fetch_chess_events(
                                                    f,
                                                    std::time::Duration::from_secs(10),
                                                ).await {
                                                    resync_events.extend(events);
                                                }
                                            }
                                            let events_vec: Vec<_> = resync_events;
                                            let Some(my_pk) = my_pk else { return };
                                            let Ok(reconstructed) = crate::stores::chess::state_reconstructor::reconstruct(&events_vec, &my_pk) else { return };
                                            game_state.set(reconstructed.game_state);
                                            desync_warning.set(None);
                                        });
                                    },
                                    "Re-sync from relay"
                                }
                            }
                        }
                    }
                }

                // Draw offer from opponent
                if *draw_offered_by_opponent.read() && is_interactive && !game_state.read().is_game_over() {
                    div { class: "rounded-xl border border-blue-500/30 bg-blue-500/10 p-3 space-y-2",
                        p { class: "text-sm text-blue-500", "Opponent offers a draw" }
                        div { class: "flex gap-2",
                            button {
                                class: "flex-1 px-4 py-2 bg-blue-500 text-white rounded-xl text-sm hover:bg-blue-600 transition",
                                onclick: {
                                    let mut draw_offered_by_opponent = draw_offered_by_opponent;
                                    let mut game_title = game_title;
                                    let mut publish_error = publish_error;
                                    move |_| {
                                        draw_offered_by_opponent.set(false);
                                        let start = *start_event_id.read();
                                        let head = *head_event_id.read();
                                        let opponent = *opponent_pubkey.read();
                                        let (Some(start_eid), Some(head_eid), Some(opp_pk)) = (start, head, opponent) else {
                                            return;
                                        };
                                        let gs = game_state.read();
                                        let fen = gs.fen();
                                        let history = gs.san_list().to_vec();
                                        let last_san = history.last().cloned().unwrap_or_default();
                                        drop(gs);

                                        let content = JesterContent::new_end(
                                            &fen,
                                            &last_san,
                                            &history,
                                            "1/2-1/2",
                                            "draw_agreement",
                                        );

                                        let mut title = game_title.read().clone();
                                        if !title.contains("Game Over") {
                                            title = format!("{} — Game Over: 1/2-1/2", title);
                                            game_title.set(title);
                                        }

                                        spawn(async move {
                                            if let Err(e) = crate::stores::chess::publish::publish_game_end(
                                                &start_eid,
                                                &head_eid,
                                                &opp_pk,
                                                content,
                                            )
                                            .await
                                            {
                                                publish_error.set(Some(format!("Failed to accept draw: {}", e)));
                                            }
                                        });
                                    }
                                },
                                "Accept"
                            }
                            button {
                                class: "flex-1 px-4 py-2 border border-border rounded-xl text-sm hover:bg-accent/5 transition",
                                onclick: move |_| {
                                    draw_offered_by_opponent.set(false);
                                },
                                "Decline"
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
                            class: "px-4 py-2 border border-blue-500/30 text-blue-500 rounded-xl text-sm hover:bg-blue-500/10 transition",
                            onclick: on_draw_offer,
                            "Offer Draw"
                        }
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
                                                "resignation",
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
