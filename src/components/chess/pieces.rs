use rschess::{Color, PieceType};

pub fn piece_svg_path(piece_type: PieceType, color: Color) -> String {
    let color_str = match color {
        Color::White => "white",
        Color::Black => "black",
    };
    let piece_char = match piece_type {
        PieceType::K => "k",
        PieceType::Q => "q",
        PieceType::R => "r",
        PieceType::B => "b",
        PieceType::N => "n",
        PieceType::P => "p",
    };
    format!("/pieces/chess/{}-{}.svg", piece_char, color_str)
}
