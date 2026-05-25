use dioxus::prelude::*;
use rschess::{Color, PieceType};

use super::pieces::piece_svg_path;

#[component]
pub fn Piece(
    file: char,
    rank: char,
    piece_type: PieceType,
    piece_color: Color,
    hide: bool,
) -> Element {
    if hide {
        return rsx! {};
    }

    let svg = piece_svg_path(piece_type, piece_color);
    let alt = format!("{:?} {:?} at {}{}", piece_color, piece_type, file, rank);

    rsx! {
        img {
            src: svg,
            alt: alt,
            class: "w-[80%] h-[80%] object-contain pointer-events-none select-none",
            draggable: false,
        }
    }
}
