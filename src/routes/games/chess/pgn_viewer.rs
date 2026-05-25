use dioxus::prelude::*;
use nostr_sdk::EventId;

use crate::components::chess::ChessBoard;
use crate::routes::Route;
use crate::stores::chess::game_state::GameState;
use crate::stores::chess::types::ViewerRole;

fn pgn_tag_value(tags: &[(String, String)], key: &str) -> Option<String> {
    tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

#[component]
pub fn ChessPgnViewer(note_id: String) -> Element {
    let mut game_state: Signal<GameState> = use_signal(GameState::new_game);
    let pgn_headers: Signal<Vec<(String, String)>> = use_signal(Vec::new);
    let is_loading = use_signal(|| true);
    let error = use_signal(|| None::<String>);
    let nav = navigator();

    let resolved_id = crate::utils::nips::chess::parse_game_id(&note_id)
        .and_then(|hex| EventId::from_hex(&hex).ok());
    let event_id = resolved_id;

    use_future(move || {
        let mut game_state = game_state;
        let mut pgn_headers = pgn_headers;
        let mut is_loading = is_loading;
        let mut error = error;
        async move {
            let Some(eid) = event_id else {
                error.set(Some("Invalid note ID".to_string()));
                is_loading.set(false);
                return;
            };

            let filter = nostr_sdk::Filter::new()
                .ids([eid])
                .kind(nostr_sdk::Kind::Custom(64));

            let result = crate::stores::nostr_client::fetching::fetch_chess_events(
                filter.clone(),
                std::time::Duration::from_secs(10),
            )
            .await;

            let events = match result {
                Ok(events) if !events.is_empty() => events,
                Ok(_) => {
                    let fallback =
                        crate::stores::nostr_client::fetching::fetch_events_from_connected_relays(
                            filter,
                            std::time::Duration::from_secs(10),
                        )
                        .await
                        .unwrap_or_default();
                    fallback
                }
                Err(_) => {
                    let fallback =
                        crate::stores::nostr_client::fetching::fetch_events_from_connected_relays(
                            filter,
                            std::time::Duration::from_secs(10),
                        )
                        .await
                        .unwrap_or_default();
                    fallback
                }
            };

            if let Some(event) = events.into_iter().next() {
                match GameState::from_pgn(&event.content) {
                    Ok((gs, tags)) => {
                        game_state.set(gs);
                        pgn_headers.set(tags);
                    }
                    Err(e) => error.set(Some(e)),
                }
            } else {
                error.set(Some("Game not found".to_string()));
            }
            is_loading.set(false);
        }
    });

    let headers = pgn_headers.read();
    let white = pgn_tag_value(&headers, "White").unwrap_or_else(|| "?".to_string());
    let black = pgn_tag_value(&headers, "Black").unwrap_or_else(|| "?".to_string());
    let result = pgn_tag_value(&headers, "Result").unwrap_or_else(|| "*".to_string());
    let event_name = pgn_tag_value(&headers, "Event");
    let date = pgn_tag_value(&headers, "Date");
    drop(headers);

    rsx! {
        div { class: "max-w-2xl mx-auto px-4 py-4 space-y-4",
            div { class: "flex items-center gap-3",
                button {
                    class: "p-2 hover:bg-accent rounded-lg transition",
                    onclick: move |_| { let _ = nav.push(Route::ChessHome {}); },
                    "←"
                }
                h1 { class: "text-lg font-semibold text-foreground", "Chess Game" }
            }

            if *is_loading.read() {
                div { class: "flex justify-center py-16",
                    div { class: "animate-spin w-8 h-8 border-2 border-primary border-t-transparent rounded-full" }
                }
            } else if let Some(err) = error.read().as_ref() {
                div { class: "rounded-xl border border-red-500/30 bg-red-500/10 p-4 text-center",
                    p { class: "text-red-500", {err.clone()} }
                }
            } else {
                // Metadata
                div { class: "rounded-xl border border-border bg-card p-3 space-y-1",
                    if let Some(name) = event_name {
                        p { class: "text-sm font-medium text-foreground", {name} }
                    }
                    div { class: "flex items-center justify-between",
                        span { class: "text-sm text-foreground", "{white} vs {black}" }
                        span { class: "text-sm font-mono text-muted-foreground", {result.clone()} }
                    }
                    if let Some(d) = date {
                        p { class: "text-xs text-muted-foreground", {d} }
                    }
                }

                // Board
                ChessBoard {
                    game_state: game_state,
                    interactive: false,
                    viewer_role: ViewerRole::Spectator,
                    perspective: rschess::Color::White,
                    on_move: None,
                }

                // Move navigation
                div { class: "flex items-center justify-center gap-2",
                    button {
                        class: "px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-accent transition disabled:opacity-30",
                        disabled: game_state.read().pointer() == 0,
                        onclick: move |_| { game_state.write().go_to_start(); },
                        "⏮"
                    }
                    button {
                        class: "px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-accent transition disabled:opacity-30",
                        disabled: game_state.read().pointer() == 0,
                        onclick: move |_| { game_state.write().step_back(); },
                        "◀"
                    }
                    span { class: "text-xs text-muted-foreground min-w-[3rem] text-center",
                        {
                            let gs = game_state.read();
                            format!("{}/{}", gs.pointer(), gs.total_moves())
                        }
                    }
                    button {
                        class: "px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-accent transition disabled:opacity-30",
                        disabled: game_state.read().pointer() >= game_state.read().total_moves(),
                        onclick: move |_| { game_state.write().step_forward(); },
                        "▶"
                    }
                    button {
                        class: "px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-accent transition disabled:opacity-30",
                        disabled: game_state.read().pointer() >= game_state.read().total_moves(),
                        onclick: move |_| { game_state.write().go_to_end(); },
                        "⏭"
                    }
                }

                // Moves
                div { class: "mt-2 rounded-xl border border-border bg-card p-3",
                    h3 { class: "text-sm font-medium text-foreground mb-2", "Moves" }
                    pre { class: "text-xs text-muted-foreground whitespace-pre-wrap font-mono leading-relaxed",
                        { game_state.read().movetext() }
                    }
                }
            }
        }
    }
}
