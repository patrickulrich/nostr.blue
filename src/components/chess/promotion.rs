use dioxus::prelude::*;
use rschess::Color;

use super::pieces::piece_svg_path;

#[component]
pub fn PromotionDialog(
    color: Color,
    file: char,
    rank: char,
    on_select: EventHandler<super::move_builder::PromotionPiece>,
) -> Element {
    let pieces = [
        super::move_builder::PromotionPiece::Queen,
        super::move_builder::PromotionPiece::Knight,
        super::move_builder::PromotionPiece::Rook,
        super::move_builder::PromotionPiece::Bishop,
    ];

    let file_idx = (file as u8 - b'a') as usize;
    let is_top = rank == '8' || rank == '1';
    let promo_color = if (rank == '8' && color == Color::White) || (rank == '1' && color == Color::Black) {
        Color::White
    } else {
        Color::Black
    };

    let left_style = format!("left: {}%; width: 12.5%;", file_idx as f64 * 12.5);

    rsx! {
        div {
            class: "absolute z-50",
            style: "{left_style}",
            style: if is_top { "top: 0" } else { "bottom: 0" },
            div { class: "bg-card border border-border rounded shadow-lg",
                for piece in pieces {
                    { render_promo_piece(piece, promo_color, color, on_select) }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn render_promo_piece(
    piece: super::move_builder::PromotionPiece,
    piece_color: Color,
    _board_color: Color,
    on_select: EventHandler<super::move_builder::PromotionPiece>,
) -> Element {
    let piece_type = match piece {
        super::move_builder::PromotionPiece::Queen => rschess::PieceType::Q,
        super::move_builder::PromotionPiece::Rook => rschess::PieceType::R,
        super::move_builder::PromotionPiece::Bishop => rschess::PieceType::B,
        super::move_builder::PromotionPiece::Knight => rschess::PieceType::N,
    };
    let svg = piece_svg_path(piece_type, piece_color);
    let p = piece;

    rsx! {
        div {
            class: "p-1 cursor-pointer hover:bg-accent/20 transition",
            onclick: move |_| on_select.call(p),
            img { src: svg, class: "w-full h-auto" }
        }
    }
}
