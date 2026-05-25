use dioxus::prelude::*;
use rschess::{Board, Color, Move};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum BoardAction {
    Apply(String),
    StepBack(String),
    StepForward(String),
    SetStart,
    SetEnd,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct HistoryStep {
    board: Board,
    san_move: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct BoardHistory {
    steps: Vec<HistoryStep>,
    pointer: usize,
    on_move: Option<Coroutine<BoardAction>>,
}

impl std::fmt::Debug for BoardHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoardHistory")
            .field("steps", &self.steps)
            .field("pointer", &self.pointer)
            .field("on_move", &self.on_move.is_some())
            .finish()
    }
}

#[allow(dead_code)]
impl BoardHistory {
    pub fn initialize(
        fen: Option<&str>,
        on_move: Option<Coroutine<BoardAction>>,
    ) -> Result<Self, String> {
        let board = match fen {
            Some(f) => Board::from_fen(rschess::Fen::try_from(f).map_err(|e| format!("{:?}", e))?),
            None => Board::default(),
        };
        Ok(Self {
            steps: vec![HistoryStep {
                board,
                san_move: None,
            }],
            pointer: 0,
            on_move,
        })
    }

    pub fn make_move(&mut self, m: Move) -> Result<(), String> {
        self.steps.truncate(self.pointer + 1);
        let current = self.steps.last().expect("history has at least 1 step");
        let mut new_board = current.board.clone();
        let san = new_board
            .move_to_san(m)
            .map_err(|e| format!("{:?}", e))?;
        new_board
            .make_move(m)
            .map_err(|e| format!("{:?}", e))?;

        self.steps.push(HistoryStep {
            board: current.board.clone(),
            san_move: Some(san.clone()),
        });
        self.steps.push(HistoryStep {
            board: new_board,
            san_move: None,
        });
        self.pointer = self.steps.len() - 1;

        if let Some(ref tx) = self.on_move {
            tx.send(BoardAction::Apply(san));
        }
        Ok(())
    }

    pub fn current_board(&self) -> &Board {
        &self
            .steps
            .get(self.pointer)
            .expect("valid pointer")
            .board
    }

    pub fn side_to_move(&self) -> Color {
        self.current_board().side_to_move()
    }

    pub fn step_back(&mut self) {
        if self.pointer == 0 {
            return;
        }
        self.pointer -= 1;
        let step = &self.steps[self.pointer];
        if let (Some(ref tx), Some(ref san)) = (&self.on_move, &step.san_move) {
            tx.send(BoardAction::StepBack(san.clone()));
        }
    }

    pub fn step_forward(&mut self) {
        if self.pointer >= self.steps.len() - 1 {
            return;
        }
        let step = &self.steps[self.pointer];
        if let (Some(ref tx), Some(ref san)) = (&self.on_move, &step.san_move) {
            tx.send(BoardAction::StepForward(san.clone()));
        }
        self.pointer += 1;
    }

    pub fn set_start(&mut self) {
        self.pointer = 0;
        if let Some(ref tx) = self.on_move {
            tx.send(BoardAction::SetStart);
        }
    }

    pub fn set_end(&mut self) {
        self.pointer = self.steps.len() - 1;
        if let Some(ref tx) = self.on_move {
            tx.send(BoardAction::SetEnd);
        }
    }

    pub fn last_move_san(&self) -> Option<String> {
        if self.steps.len() >= 2 {
            self.steps
                .get(self.steps.len() - 2)
                .and_then(|s| s.san_move.clone())
        } else {
            None
        }
    }

    pub fn can_step_back(&self) -> bool {
        self.pointer > 0
    }

    pub fn can_step_forward(&self) -> bool {
        self.pointer < self.steps.len() - 1
    }
}
