use dioxus::prelude::*;
use nostr_sdk::EventId;

use crate::components::chess::ChessBoard;
use crate::routes::Route;
use crate::stores::chess::game_state::GameState;
use crate::stores::chess::types::ViewerRole;

#[component]
pub fn ChessPgnViewer(note_id: String) -> Element {
    let game_state: Signal<GameState> = use_signal(GameState::new_game);
    let is_loading = use_signal(|| true);
    let error = use_signal(|| None::<String>);
    let _pgn_headers = use_signal(Vec::<(String, String)>::new);
    let nav = navigator();

    let event_id = EventId::from_hex(&note_id).ok();

    use_future(move || {
        let mut game_state = game_state;
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

            let result = crate::stores::nostr_client::fetching::fetch_events_from_connected_relays(
                filter,
                std::time::Duration::from_secs(10),
            )
            .await;

            match result {
                Ok(events) => {
                    if let Some(event) = events.into_iter().next() {
                        match GameState::from_pgn(&event.content) {
                            Ok(gs) => game_state.set(gs),
                            Err(e) => error.set(Some(e)),
                        }
                    } else {
                        error.set(Some("Game not found".to_string()));
                    }
                }
                Err(_) => {
                    error.set(Some("Failed to fetch game".to_string()));
                }
            }
            is_loading.set(false);
        }
    });

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
                ChessBoard {
                    game_state: game_state,
                    interactive: false,
                    viewer_role: ViewerRole::Spectator,
                    perspective: rschess::Color::White,
                    on_move: None,
                }

                div { class: "mt-4 rounded-xl border border-border bg-card p-3",
                    h3 { class: "text-sm font-medium text-foreground mb-2", "PGN" }
                    pre { class: "text-xs text-muted-foreground whitespace-pre-wrap font-mono",
                        { game_state.read().movetext() }
                    }
                }
            }
        }
    }
}
