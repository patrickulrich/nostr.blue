use dioxus::prelude::*;
use nostr_sdk::EventId;

use crate::components::chess::ChessBoard;
use crate::routes::Route;
use crate::stores::chess::game_state::GameState;
use crate::stores::chess::jester::JesterContent;
use crate::stores::chess::types::ViewerRole;

#[component]
pub fn ChessGameDetail(game_id: String) -> Element {
    let game_state: Signal<GameState> = use_signal(GameState::new_game);
    let viewer_role = use_signal(|| ViewerRole::Spectator);
    let is_loading = use_signal(|| true);
    let game_title = use_signal(String::new);
    let last_move_san = use_signal(String::new);

    let event_id = EventId::from_hex(&game_id).ok();
    let nav = navigator();

    use_future(move || {
        let mut game_state = game_state;
        let mut viewer_role = viewer_role;
        let mut is_loading = is_loading;
        let mut game_title = game_title;
        let _last_move_san = last_move_san;
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
                    let my_pk = crate::stores::auth_store::AUTH_STATE.read().pubkey.clone().and_then(|pk| nostr_sdk::PublicKey::from_hex(&pk).ok());

                    if let Some(my_pk) = my_pk {
                        if let Ok(reconstructed) =
                            crate::stores::chess::state_reconstructor::reconstruct(
                                &events_vec,
                                &my_pk,
                            )
                        {
                            game_state.set(reconstructed.game_state);
                            viewer_role.set(reconstructed.viewer_role);

                            let w = crate::utils::format::truncate_pubkey(&reconstructed.white_pubkey.to_hex());
                            let b = reconstructed
                                .black_pubkey
                                .map(|p| crate::utils::format::truncate_pubkey(&p.to_hex()))
                                .unwrap_or_else(|| "Waiting...".to_string());
                            game_title.set(format!("{} vs {}", w, b));
                        }
                    } else {
                        let mut gs = GameState::new_game();

                        let mut best_history: Vec<String> = vec![];

                        for event in &events_vec {
                            if let Some(content) = JesterContent::parse(&event.content) {
                                if content.kind == 0 {
                                } else if content.kind == 1 && content.history.len() > best_history.len() {
                                    best_history = content.history;
                                }
                            }
                        }

                        for san in &best_history {
                            let _ = gs.make_move_san(san);
                        }
                        game_state.set(gs);
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
    let is_interactive = !matches!(*viewer_role.read(), ViewerRole::Spectator);

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
                    on_move: None,
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

                // Actions
                div { class: "flex gap-2 mt-4",
                    if !game_state.read().is_game_over() && is_interactive {
                        button {
                            class: "px-4 py-2 border border-red-500/30 text-red-500 rounded-xl text-sm hover:bg-red-500/10 transition",
                            onclick: move |_| {
                                // TODO: publish resign
                            },
                            "Resign"
                        }
                    }
                }
            }
        }
    }
}
