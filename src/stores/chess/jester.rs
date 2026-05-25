use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JesterContent {
    pub version: String,
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fen: Option<String>,
    #[serde(rename = "move", skip_serializing_if = "Option::is_none")]
    pub mv: Option<String>,
    #[serde(default)]
    pub history: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination: Option<String>,
}

pub const JESTER_CONTENT_KIND_START: u8 = 0;
pub const JESTER_CONTENT_KIND_MOVE: u8 = 1;
#[allow(dead_code)]
pub const JESTER_CONTENT_KIND_CHAT: u8 = 2;

impl JesterContent {
    pub fn new_start(player_color: rschess::Color) -> Self {
        let color_str = match player_color {
            rschess::Color::White => "white",
            rschess::Color::Black => "black",
        };
        Self {
            version: "0".to_string(),
            kind: JESTER_CONTENT_KIND_START,
            fen: None,
            mv: None,
            history: vec![],
            nonce: Some(format!("{:08x}", rand::random::<u32>())),
            player_color: Some(color_str.to_string()),
            result: None,
            termination: None,
        }
    }

    #[allow(dead_code)]
    pub fn new_move(fen: &str, san: &str, history: &[String]) -> Self {
        let mut full_history = history.to_vec();
        full_history.push(san.to_string());
        Self {
            version: "0".to_string(),
            kind: JESTER_CONTENT_KIND_MOVE,
            fen: Some(fen.to_string()),
            mv: Some(san.to_string()),
            history: full_history,
            nonce: None,
            player_color: None,
            result: None,
            termination: None,
        }
    }

    #[allow(dead_code)]
    pub fn new_end(fen: &str, san: &str, history: &[String], result: &str, termination: &str) -> Self {
        let mut content = Self::new_move(fen, san, history);
        content.result = Some(result.to_string());
        content.termination = Some(termination.to_string());
        content
    }

    pub fn parse(event_content: &str) -> Option<Self> {
        serde_json::from_str(event_content).ok()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}
