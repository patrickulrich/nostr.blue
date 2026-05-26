use dioxus::prelude::*;

use crate::stores::chess::lobby_state::ActiveGame;
use crate::stores::chess::types::{ChessChallenge, CompletedGame};

#[derive(Props, Clone, PartialEq)]
pub struct ChallengeCardProps {
    pub challenge: ChessChallenge,
    pub on_accept: EventHandler<()>,
}

#[component]
pub fn ChallengeCard(props: ChallengeCardProps) -> Element {
    let challenger = crate::utils::format::truncate_pubkey(&props.challenge.challenger_pubkey.to_hex());
    let color_label = match props.challenge.challenger_color {
        rschess::Color::White => "plays White",
        rschess::Color::Black => "plays Black",
    };

    rsx! {
        div { class: "rounded-xl border border-border bg-card p-3 flex items-center justify-between",
            div { class: "space-y-1",
                span { class: "text-sm text-foreground", {challenger} }
                span { class: "text-xs text-muted-foreground block", {color_label} }
            }
            button {
                class: "px-3 py-1.5 bg-primary text-primary-foreground rounded-lg text-sm hover:bg-primary/90 transition",
                onclick: move |_| props.on_accept.call(()),
                "Accept"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ActiveGameCardProps {
    pub game: ActiveGame,
}

#[component]
pub fn ActiveGameCard(props: ActiveGameCardProps) -> Element {
    let game_id = props.game.game_id.to_hex();
    let white = crate::utils::format::truncate_pubkey(&props.game.white_pubkey.to_hex());
    let black = props
        .game
        .black_pubkey
        .map(|p| crate::utils::format::truncate_pubkey(&p.to_hex()))
        .unwrap_or_else(|| "Waiting...".to_string());

    rsx! {
        Link {
            to: crate::routes::Route::ChessGameDetail { game_id },
            class: "block rounded-xl border border-border bg-card p-3 hover:bg-accent/5 transition",
            div { class: "flex items-center justify-between",
                div { class: "space-y-1",
                    div { class: "flex items-center gap-2",
                        div { class: "w-2.5 h-2.5 rounded-full bg-white border border-border" }
                        span { class: "text-sm text-foreground truncate max-w-[120px]", {white} }
                    }
                    div { class: "flex items-center gap-2",
                        div { class: "w-2.5 h-2.5 rounded-full bg-gray-800 border border-border" }
                        span { class: "text-sm text-foreground truncate max-w-[120px]", {black} }
                    }
                }
                div { class: "text-right",
                    span { class: "text-xs text-muted-foreground",
                        { format!("Move {}", props.game.move_count) }
                    }
                    if props.game.is_my_turn {
                        span { class: "block text-xs text-green-500 font-medium mt-0.5",
                            "Your turn"
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct CompletedGameCardProps {
    pub game: CompletedGame,
}

#[component]
pub fn CompletedGameCard(props: CompletedGameCardProps) -> Element {
    let white = crate::utils::format::truncate_pubkey(&props.game.white_pubkey.to_hex());
    let black = props.game.black_pubkey
        .as_ref()
        .map(|pk| crate::utils::format::truncate_pubkey(&pk.to_hex()))
        .unwrap_or_else(|| "Unknown".to_string());
    let result_badge = match props.game.result.as_str() {
        "1-0" => ("White won", "bg-white/20 text-foreground"),
        "0-1" => ("Black won", "bg-gray-800/20 text-foreground"),
        _ => ("Draw", "bg-yellow-500/20 text-yellow-500"),
    };

    rsx! {
        div { class: "rounded-xl border border-border bg-card p-3",
            div { class: "flex items-center justify-between",
                div { class: "space-y-1",
                    div { class: "flex items-center gap-2",
                        div { class: "w-2.5 h-2.5 rounded-full bg-white border border-border" }
                        span { class: "text-sm text-foreground truncate max-w-[100px]", {white} }
                    }
                    div { class: "flex items-center gap-2",
                        div { class: "w-2.5 h-2.5 rounded-full bg-gray-800 border border-border" }
                        span { class: "text-sm text-foreground truncate max-w-[100px]", {black} }
                    }
                }
                div { class: "text-right space-y-1",
                    span { class: "text-xs px-2 py-0.5 rounded-full {result_badge.1}", {result_badge.0} }
                    span { class: "block text-xs text-muted-foreground",
                        { format!("{} moves", props.game.move_count) }
                    }
                }
            }
        }
    }
}
