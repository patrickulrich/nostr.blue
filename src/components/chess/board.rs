use dioxus::prelude::*;
use rschess::{Color, PieceType};

use super::move_builder::{PromotionPiece, SquareCoord};
use super::piece::Piece;
use super::pieces::piece_svg_path;
use crate::stores::chess::game_state::GameState;
use crate::stores::chess::types::ViewerRole;

#[derive(Props, Clone, PartialEq)]
pub struct ChessBoardProps {
    pub game_state: Signal<GameState>,
    pub interactive: bool,
    pub viewer_role: ViewerRole,
    pub perspective: Color,
    pub on_move: Option<EventHandler<String>>,
}

#[component]
pub fn ChessBoard(props: ChessBoardProps) -> Element {
    let mut selected = use_signal(|| Option::<SquareCoord>::None);
    let mut legal_targets = use_signal(Vec::<SquareCoord>::new);
    let mut promotion_pending = use_signal(|| Option::<(SquareCoord, SquareCoord)>::None);

    let mut gs = props.game_state;
    let board = gs.read();
    let perspective = props.perspective;
    let is_check = board.is_check();
    let checked_king = board.checked_king_square();
    drop(board);

    let files: Vec<char> = if perspective == Color::White {
        ('a'..='h').collect()
    } else {
        ('a'..='h').rev().collect()
    };
    let ranks: Vec<char> = if perspective == Color::White {
        ('1'..='8').collect()
    } else {
        ('1'..='8').rev().collect()
    };

    let is_interactive = props.interactive
        && !gs.read().is_game_over()
        && !matches!(props.viewer_role, ViewerRole::Spectator);

    let show_promotion = promotion_pending.read().is_some();
    let on_move = props.on_move;

    rsx! {
        div { class: "relative select-none",
            div { class: "grid grid-cols-8 aspect-square w-full max-w-[600px] mx-auto",
                for rank in ranks.iter() {
                    for file in files.iter() {
                        {
                            let f = *file;
                            let r = *rank;
                            let coord = SquareCoord::new(f, r);
                            let is_light = coord.is_light();
                            let sel = *selected.read();
                            let is_selected = sel == Some(coord);
                            let is_legal_target = legal_targets.read().contains(&coord);
                            let is_check_square = is_check && checked_king == Some((f, r));
                            let piece = gs.read().occupant(f, r);
                            let promo_hide = show_promotion
                                && (*promotion_pending.read()).map(|(s,d)| s == coord || d == coord).unwrap_or(false);
                            let mut square_classes = vec!["relative", "aspect-square", "flex", "items-center", "justify-center"];
                            if is_light {
                                square_classes.push("bg-[#f0d9b5]");
                            } else {
                                square_classes.push("bg-[#b58863]");
                            }
                            if is_selected {
                                square_classes.push("!bg-[#7fa650]/60");
                            }
                            if is_legal_target {
                                square_classes.push("cursor-pointer");
                            }
                            if is_check_square {
                                square_classes.push("!bg-red-500/50");
                            }

                            rsx! {
                                div {
                                    key: "{f}{r}",
                                    class: square_classes.join(" "),
                                    onclick: move |_| {
                                        if !is_interactive {
                                            return;
                                        }
                                        let coord = SquareCoord::new(f, r);
                                        let current_sel = *selected.read();

                                        match current_sel {
                                            None => {
                                                let piece = gs.read().occupant(f, r);
                                                if let Some(p) = piece {
                                                    let gs_read = gs.read();
                                                    if p.color() == gs_read.side_to_move() {
                                                        let moves = gs_read.legal_moves_from(f, r);
                                                        let targets: Vec<SquareCoord> = moves
                                                            .iter()
                                                            .map(|m| {
                                                                let (tf, tr) = m.to_square();
                                                                SquareCoord::new(tf, tr)
                                                            })
                                                            .collect();
                                                        if !targets.is_empty() {
                                                            selected.set(Some(coord));
                                                            legal_targets.set(targets);
                                                        }
                                                    }
                                                }
                                            }
                                            Some(src) => {
                                                if src == coord {
                                                    selected.set(None);
                                                    legal_targets.set(vec![]);
                                                    return;
                                                }

                                                let is_legal = legal_targets.read().contains(&coord);
                                                if !is_legal {
                                                    let piece = gs.read().occupant(f, r);
                                                    if let Some(p) = piece {
                                                        let gs_read = gs.read();
                                                        if p.color() == gs_read.side_to_move() {
                                                            let moves = gs_read.legal_moves_from(f, r);
                                                            let targets: Vec<SquareCoord> = moves
                                                                .iter()
                                                                .map(|m| {
                                                                    let (tf, tr) = m.to_square();
                                                                    SquareCoord::new(tf, tr)
                                                                })
                                                                .collect();
                                                            if !targets.is_empty() {
                                                                selected.set(Some(coord));
                                                                legal_targets.set(targets);
                                                                return;
                                                            }
                                                        }
                                                    }
                                                    selected.set(None);
                                                    legal_targets.set(vec![]);
                                                    return;
                                                }

                                                let src_piece = gs.read().occupant(src.file, src.rank);
                                                let is_promotion = src_piece.map(|p| p.piece_type() == PieceType::P).unwrap_or(false)
                                                    && (r == '8' || r == '1');

                                                if is_promotion {
                                                    promotion_pending.set(Some((src, coord)));
                                                    selected.set(None);
                                                    legal_targets.set(vec![]);
                                                    return;
                                                }

                                                apply_move(src, coord, None, gs, selected, legal_targets, promotion_pending, on_move);
                                            }
                                        }
                                    },

                                    if is_legal_target && piece.is_none() {
                                        div { class: "w-[30%] h-[30%] rounded-full bg-[#7fa650]/40" }
                                    }
                                    if is_legal_target && piece.is_some() {
                                        div { class: "absolute inset-0 rounded-full border-[3px] border-[#7fa650]/40" }
                                    }

                                    if let Some(p) = piece {
                                        Piece {
                                            file: f,
                                            rank: r,
                                            piece_type: p.piece_type(),
                                            piece_color: p.color(),
                                            hide: promo_hide,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // File labels
            div { class: "flex max-w-[600px] mx-auto",
                for file in files.iter() {
                    div {
                        key: "f-{file}",
                        class: "flex-1 text-center text-xs text-muted-foreground/60 py-0.5",
                        { file.to_string() }
                    }
                }
            }

            // Promotion dialog
            if let Some((src, dst)) = *promotion_pending.read() {
                {
                    let side = gs.read().side_to_move();
                    let piece_color = side;
                    let pieces = [
                        (PromotionPiece::Queen, rschess::PieceType::Q),
                        (PromotionPiece::Knight, rschess::PieceType::N),
                        (PromotionPiece::Rook, rschess::PieceType::R),
                        (PromotionPiece::Bishop, rschess::PieceType::B),
                    ];
                    let file_idx = (dst.file as u8 - b'a') as usize;
                    let left_pct = file_idx as f64 * 12.5;

                    rsx! {
                        div {
                            class: "absolute z-50 bg-card border border-border rounded-lg shadow-lg overflow-hidden",
                            style: "left: {left_pct}%; width: 12.5%; top: 0;",
                            for (promo, pt) in pieces {
                                {
                                    let svg = piece_svg_path(pt, piece_color);
                                    let p = promo;
                                    let on_move_clone = on_move;
                                    rsx! {
                                        div {
                                            class: "p-1.5 cursor-pointer hover:bg-accent/20 transition",
                                            onclick: move |_| {
                                                promotion_pending.set(None);
                                                apply_move(src, dst, Some(p), gs, selected, legal_targets, promotion_pending, on_move_clone);
                                            },
                                            img { src: svg, class: "w-full h-auto" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Move navigation
            div { class: "flex items-center justify-center gap-2 mt-2",
                button {
                    class: "p-1.5 hover:bg-accent rounded transition text-sm",
                    onclick: move |_| { gs.write().go_to_start(); selected.set(None); legal_targets.set(vec![]); },
                    "⟨⟨"
                }
                button {
                    class: "p-1.5 hover:bg-accent rounded transition text-sm",
                    onclick: move |_| { gs.write().step_back(); selected.set(None); legal_targets.set(vec![]); },
                    "⟨"
                }
                span { class: "text-xs text-muted-foreground min-w-[60px] text-center",
                    { format!("{}/{}", gs.read().pointer(), gs.read().total_moves()) }
                }
                button {
                    class: "p-1.5 hover:bg-accent rounded transition text-sm",
                    onclick: move |_| { gs.write().step_forward(); selected.set(None); legal_targets.set(vec![]); },
                    "⟩"
                }
                button {
                    class: "p-1.5 hover:bg-accent rounded transition text-sm",
                    onclick: move |_| { gs.write().go_to_end(); selected.set(None); legal_targets.set(vec![]); },
                    "⟩⟩"
                }
            }

            // Status
            div { class: "flex items-center justify-center mt-1",
                span { class: "text-xs text-muted-foreground",
                    {
                        let gs_r = gs.read();
                        if gs_r.is_game_over() {
                            if gs_r.is_checkmate() {
                                let winner = gs_r.game_result().map(|r| format!("{}", r)).unwrap_or_default();
                                format!("Checkmate {}", winner)
                            } else if gs_r.is_stalemate() {
                                "Stalemate - Draw".to_string()
                            } else {
                                "Game Over".to_string()
                            }
                        } else {
                            let side = if gs_r.side_to_move() == Color::White { "White" } else { "Black" };
                            let check = if gs_r.is_check() { " (Check)" } else { "" };
                            format!("{} to move{}", side, check)
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_move(
    src: SquareCoord,
    dst: SquareCoord,
    promo: Option<PromotionPiece>,
    mut gs: Signal<GameState>,
    mut selected: Signal<Option<SquareCoord>>,
    mut legal_targets: Signal<Vec<SquareCoord>>,
    mut _promotion_pending: Signal<Option<(SquareCoord, SquareCoord)>>,
    on_move: Option<EventHandler<String>>,
) {
    let board = gs.read();
    let legal_move = match promo {
        Some(p) => board.get_legal_promotion_move(
            src.file, src.rank, dst.file, dst.rank, p.to_piece_char(),
        ),
        None => board.get_legal_move(src.file, src.rank, dst.file, dst.rank),
    };
    drop(board);

    if let Some(m) = legal_move {
        let mut gs_w = gs.write();
        let san_result = gs_w.current_board().move_to_san(m);
        match san_result {
            Ok(san) => {
                if let Err(e) = gs_w.make_move_san(&san) {
                    log::warn!("Failed to make move: {}", e);
                } else if let Some(handler) = on_move {
                    handler.call(san);
                }
            }
            Err(e) => {
                log::warn!("Failed to get SAN: {:?}", e);
            }
        }
    }

    selected.set(None);
    legal_targets.set(vec![]);
}
