use serde::{Deserialize, Serialize};

pub use crate::utils::zeroize_string::ZeroizeString;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BackupBundle {
    pub nsec_hex: ZeroizeString,
    pub nwc_uri: Option<String>,
    pub account_label: Option<String>,
    pub created_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackupEntry {
    pub file_id: String,
    pub npub: String,
    pub display_name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GoogleAuthResult {
    pub sub: String,
    pub access_token: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum GoogleBackupState {
    #[default]
    Idle,
    SigningIn,
    CheckingDrive,
    Choose {
        entries: Vec<BackupEntry>,
        auth: GoogleAuthResult,
    },
    NoBackup(GoogleAuthResult),
    ImportKey {
        auth: GoogleAuthResult,
        nsec_input: String,
        error: Option<String>,
    },
    ShowMnemonic {
        auth: GoogleAuthResult,
        words: String,
        acknowledged: bool,
    },
    Working,
    Done {
        is_new_account: bool,
    },
    Error(String),
}

impl GoogleBackupState {
    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Idle | Self::Done { .. })
    }
}
