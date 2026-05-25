#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SquareCoord {
    pub file: char,
    pub rank: char,
}

#[allow(dead_code)]
impl SquareCoord {
    pub fn new(file: char, rank: char) -> Self {
        Self { file, rank }
    }

#[allow(clippy::wrong_self_convention)]
    pub fn to_uci(&self) -> String {
        format!("{}{}", self.file, self.rank)
    }

    pub fn idx(&self) -> usize {
        let f = (self.file as u8 - b'a') as usize;
        let r = (self.rank as u8 - b'1') as usize;
        r * 8 + f
    }

    pub fn from_idx(idx: usize) -> Self {
        let file = (b'a' + (idx % 8) as u8) as char;
        let rank = (b'1' + (idx / 8) as u8) as char;
        Self { file, rank }
    }

    pub fn is_light(&self) -> bool {
        let f = (self.file as u8 - b'a') % 2;
        let r = (self.rank as u8 - b'1') % 2;
        f != r
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PromotionPiece {
    Queen,
    Rook,
    Bishop,
    Knight,
}

impl PromotionPiece {
    pub fn to_piece_char(self) -> char {
        match self {
            Self::Queen => 'Q',
            Self::Rook => 'R',
            Self::Bishop => 'B',
            Self::Knight => 'N',
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MoveBuilderState {
    None,
    Src(SquareCoord),
    Promotion {
        src: SquareCoord,
        dst: SquareCoord,
    },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ApplicableMove {
    pub from: SquareCoord,
    pub to: SquareCoord,
    pub san: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MoveBuilder {
    pub state: MoveBuilderState,
    pub last_move_from: Option<SquareCoord>,
    pub last_move_to: Option<SquareCoord>,
}

#[allow(dead_code)]
impl MoveBuilder {
    pub fn new() -> Self {
        Self {
            state: MoveBuilderState::None,
            last_move_from: None,
            last_move_to: None,
        }
    }

    pub fn selected_src(&self) -> Option<SquareCoord> {
        match &self.state {
            MoveBuilderState::Src(c) => Some(*c),
            _ => None,
        }
    }

    pub fn promotion_src_dst(&self) -> Option<(SquareCoord, SquareCoord)> {
        match &self.state {
            MoveBuilderState::Promotion { src, dst } => Some((*src, *dst)),
            _ => None,
        }
    }

    pub fn apply_last_move(&mut self, from: SquareCoord, to: SquareCoord) {
        self.last_move_from = Some(from);
        self.last_move_to = Some(to);
        self.state = MoveBuilderState::None;
    }

    pub fn clear(&mut self) {
        self.state = MoveBuilderState::None;
    }
}
