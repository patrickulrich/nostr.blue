//! Shared types for git operations across web and native backends.
use serde::{Deserialize, Serialize};

/// File entry from git tree listing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub path: String,
}

impl FileEntry {
    pub fn is_directory(&self) -> bool {
        self.entry_type == "tree"
    }
}

/// Commit entry from git log
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitEntry {
    pub oid: String,
    pub message: String,
    pub author: String,
    pub email: String,
    pub timestamp: u64,
    pub parent: Option<String>,
}
