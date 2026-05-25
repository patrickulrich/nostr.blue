use rschess::{Board, Move, Color, Piece, PieceType, GameResult};

use super::types::{ViewerRole, ChessGameStatus};

#[derive(Debug, Clone)]
pub struct HistoryStep {
    pub board: Board,
    #[expect(dead_code)]
    pub san_move: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BoardSnapshot {
    board: Board,
    history: Vec<HistoryStep>,
    pointer: usize,
    san_list: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GameState {
    board: Board,
    history: Vec<HistoryStep>,
    pointer: usize,
    san_list: Vec<String>,
}

impl GameState {
    pub fn new_game() -> Self {
        Self::from_fen(None).expect("default FEN is valid")
    }

    pub fn from_fen(fen: Option<&str>) -> Result<Self, String> {
        let board = match fen {
            Some(f) => Board::from_fen(rschess::Fen::try_from(f).map_err(|e| format!("Invalid FEN: {:?}", e))?),
            None => Board::default(),
        };
        let board_clone = board.clone();
        Ok(Self {
            board,
            history: vec![HistoryStep {
                board: board_clone,
                san_move: None,
            }],
            pointer: 0,
            san_list: vec![],
        })
    }

    pub fn from_pgn(pgn_text: &str) -> Result<Self, String> {
        let pgn = rschess::pgn::Pgn::try_from(pgn_text)
            .map_err(|e| format!("Invalid PGN: {:?}", e))?;
        let board = pgn.board().clone();
        Ok(Self {
            board: board.clone(),
            history: vec![HistoryStep {
                board,
                san_move: None,
            }],
            pointer: 0,
            san_list: vec![],
        })
    }

    pub fn make_move_san(&mut self, san: &str) -> Result<(), String> {
        self.go_to_end();
        let m = self
            .board
            .san_to_move(san)
            .map_err(|e| format!("Invalid SAN: {:?}", e))?;
        self.board
            .make_move(m)
            .map_err(|e| format!("Illegal move: {:?}", e))?;
        self.history.truncate(self.pointer + 1);
        self.history.push(HistoryStep {
            board: self.board.clone(),
            san_move: Some(san.to_string()),
        });
        self.san_list.push(san.to_string());
        self.pointer = self.history.len() - 1;
        Ok(())
    }

    pub fn current_board(&self) -> &Board {
        &self.history[self.pointer].board
    }

    pub fn step_back(&mut self) -> bool {
        if self.pointer > 0 {
            self.pointer -= 1;
            true
        } else {
            false
        }
    }

    pub fn step_forward(&mut self) -> bool {
        if self.pointer < self.history.len() - 1 {
            self.pointer += 1;
            true
        } else {
            false
        }
    }

    pub fn go_to_start(&mut self) {
        self.pointer = 0;
    }

    pub fn go_to_end(&mut self) {
        self.pointer = self.history.len() - 1;
    }

    pub fn pointer(&self) -> usize {
        self.pointer
    }

    pub fn total_moves(&self) -> usize {
        self.history.len().saturating_sub(1)
    }

    pub fn side_to_move(&self) -> Color {
        self.current_board().side_to_move()
    }

    pub fn legal_moves_from(&self, file: char, rank: char) -> Vec<Move> {
        let idx = match rschess::sq_to_idx(file, rank) {
            Ok(i) => i,
            Err(_) => return vec![],
        };
        self.current_board()
            .gen_legal_moves()
            .into_iter()
            .filter(|m| {
                let (f, _r) = m.from_square();
                f == file && m.from_square() == rschess::idx_to_sq(idx).unwrap()
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn legal_moves_to_san_from(&self, file: char, rank: char) -> Vec<String> {
        let board = self.current_board();
        let idx = match rschess::sq_to_idx(file, rank) {
            Ok(i) => i,
            Err(_) => return vec![],
        };
        board
            .gen_legal_moves()
            .into_iter()
            .filter(|m| {
                let (f, _r) = m.from_square();
                f == file && m.from_square() == rschess::idx_to_sq(idx).unwrap()
            })
            .filter_map(|m| board.move_to_san(m).ok())
            .collect()
    }

    #[allow(dead_code)]
    pub fn is_legal_move(&self, from_file: char, from_rank: char, to_file: char, to_rank: char) -> bool {
        let board = self.current_board();
        board.gen_legal_moves().iter().any(|m| {
            let (ff, fr) = m.from_square();
            let (tf, tr) = m.to_square();
            ff == from_file && fr == from_rank && tf == to_file && tr == to_rank
        })
    }

    pub fn get_legal_move(&self, from_file: char, from_rank: char, to_file: char, to_rank: char) -> Option<Move> {
        let board = self.current_board();
        board
            .gen_legal_moves()
            .into_iter()
            .find(|m| {
                let (ff, fr) = m.from_square();
                let (tf, tr) = m.to_square();
                ff == from_file && fr == from_rank && tf == to_file && tr == to_rank
            })
    }

    pub fn occupant(&self, file: char, rank: char) -> Option<Piece> {
        self.current_board()
            .occupant_of_square(file, rank)
            .ok()
            .flatten()
    }

    pub fn to_pgn(&self, tags: Vec<(String, String)>) -> Result<String, String> {
        let mut board = Board::default();
        for san in &self.san_list {
            board
                .make_move_san(san)
                .map_err(|e| format!("Failed to replay move {}: {:?}", san, e))?;
        }
        let result_tag = board
            .game_result()
            .map(|r| format!("{}", r))
            .unwrap_or_else(|| "*".to_string());
        let mut all_tags = vec![
            ("Event".to_string(), "Live Chess Game".to_string()),
            ("Site".to_string(), "Nostr".to_string()),
            ("Result".to_string(), result_tag),
        ];
        all_tags.extend(tags);
        let pgn = rschess::pgn::Pgn::from_board(board, all_tags)
            .map_err(|e| format!("PGN generation error: {:?}", e))?;
        Ok(format!("{}", pgn))
    }

    pub fn game_result(&self) -> Option<GameResult> {
        self.current_board().game_result()
    }

    pub fn is_game_over(&self) -> bool {
        self.current_board().is_game_over()
    }

    pub fn is_check(&self) -> bool {
        self.current_board().is_check()
    }

    pub fn is_checkmate(&self) -> bool {
        self.current_board().is_checkmate()
    }

    pub fn is_stalemate(&self) -> bool {
        self.current_board().is_stalemate()
    }

    pub fn fen(&self) -> String {
        format!("{}", self.current_board().to_fen())
    }

    pub fn san_list(&self) -> &[String] {
        &self.san_list
    }

    pub fn movetext(&self) -> String {
        self.current_board().gen_movetext()
    }

    #[allow(dead_code)]
    pub fn status(&self, _viewer: &ViewerRole) -> ChessGameStatus {
        let board = self.current_board();
        if board.is_checkmate() {
            let winner = match board.checkmated_side() {
                Some(Color::White) => "0-1",
                Some(Color::Black) => "1-0",
                None => "*",
            };
            ChessGameStatus::Completed(winner.to_string())
        } else if board.is_stalemate() {
            ChessGameStatus::Completed("1/2-1/2".to_string())
        } else if board.game_result().is_some() {
            let r = format!("{}", board.game_result().unwrap());
            ChessGameStatus::Completed(r)
        } else {
            ChessGameStatus::Active
        }
    }

    #[allow(dead_code)]
    pub fn is_my_turn(&self, viewer: &ViewerRole) -> bool {
        let my_color = match viewer {
            ViewerRole::WhitePlayer => Color::White,
            ViewerRole::BlackPlayer => Color::Black,
            ViewerRole::Spectator => return false,
        };
        !self.is_game_over() && self.side_to_move() == my_color
    }

    pub fn snapshot(&self) -> BoardSnapshot {
        BoardSnapshot {
            board: self.board.clone(),
            history: self.history.clone(),
            pointer: self.pointer,
            san_list: self.san_list.clone(),
        }
    }

    pub fn restore_snapshot(&mut self, snap: BoardSnapshot) {
        self.board = snap.board;
        self.history = snap.history;
        self.pointer = snap.pointer;
        self.san_list = snap.san_list;
    }

    pub fn checked_king_square(&self) -> Option<(char, char)> {
        if !self.is_check() {
            return None;
        }
        let board = self.current_board();
        let side = board.side_to_move();
        for file in 'a'..='h' {
            for rank in '1'..='8' {
                if let Ok(Some(piece)) = board.occupant_of_square(file, rank) {
                    if piece.piece_type() == PieceType::K && piece.color() == side {
                        return Some((file, rank));
                    }
                }
            }
        }
        None
    }
}

