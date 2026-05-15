use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::Deref;
use zeroize::Zeroize;

#[derive(Clone, PartialEq)]
pub struct ZeroizeString(pub String);

impl ZeroizeString {}

impl Deref for ZeroizeString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for ZeroizeString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Serialize for ZeroizeString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ZeroizeString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(ZeroizeString(String::deserialize(deserializer)?))
    }
}

impl std::fmt::Debug for ZeroizeString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

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
