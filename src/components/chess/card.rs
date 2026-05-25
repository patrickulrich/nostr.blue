use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::chess::types::PublicGame;

#[derive(Props, Clone, PartialEq)]
pub struct ChessCardProps {
    pub game: PublicGame,
}

#[component]
pub fn ChessCard(props: ChessCardProps) -> Element {
    let game_id_str = props.game.game_id.to_hex();

    rsx! {
        Link {
            to: Route::ChessGameDetail { game_id: game_id_str },
            class: "block rounded-2xl border border-border bg-card p-4 shadow-sm hover:bg-accent/5 transition",
            div { class: "space-y-2",
                div { class: "flex items-center justify-between",
                    span { class: "text-sm font-medium text-foreground",
                        { format!("{} moves", props.game.move_count) }
                    }
                    if props.game.is_active {
                        span { class: "text-xs bg-green-500/20 text-green-500 px-2 py-0.5 rounded-full",
                            "Live"
                        }
                    }
                }
                div { class: "flex items-center gap-2",
                    div { class: "w-3 h-3 rounded-full bg-white border border-border" }
                    span { class: "text-xs text-muted-foreground",
                        { crate::utils::format::truncate_pubkey(&props.game.white_pubkey.to_hex()) }
                    }
                }
                if let Some(black) = props.game.black_pubkey {
                    div { class: "flex items-center gap-2",
                        div { class: "w-3 h-3 rounded-full bg-gray-800 border border-border" }
                        span { class: "text-xs text-muted-foreground",
                            { crate::utils::format::truncate_pubkey(&black.to_hex()) }
                        }
                    }
                }
            }
        }
    }
}
