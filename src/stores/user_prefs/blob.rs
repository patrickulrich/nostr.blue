//! [`UserPrefsBlob`] — unified user preferences encrypted to self via the
//! main signer (async NIP-44).

use serde::{Deserialize, Serialize};

use crate::stores::social::reactions_store::PreferredReaction;
use crate::stores::ui::ai_provider_store::AiProviderState;
use crate::stores::ui::settings_store::AppSettings;
use crate::stores::ui::sidebar_store::SidebarPreferencesData;

/// Unified user preference blob.
///
/// Serialized to JSON, encrypted via NIP-44 to self using the main signer,
/// and published as kind 30078 with d-tag `nostr.blue/prefs`.
///
/// ## Forward compatibility
///
/// Every field uses `#[serde(default)]` so blobs written by older versions
/// (missing new fields) deserialize correctly with defaults for the missing
/// fields. The `version` field tracks structural changes.
///
/// ## Migration
///
/// During Phase 1 (dual-read), this blob is assembled from legacy d-tags
/// when `nostr.blue/prefs` is not found on relays. See [`super::encrypt`]
/// for the encrypt/decrypt helpers.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UserPrefsBlob {
    /// Schema version. Bumped on breaking changes; old versions trigger
    /// per-field serde defaults and a re-save in the new format.
    #[serde(default = "default_version")]
    pub version: u32,

    /// App-level settings (theme, blossom servers, sync toggles, etc.).
    #[serde(default)]
    pub settings: AppSettings,

    /// Sidebar layout preferences (item order + page size).
    #[serde(default)]
    pub sidebar: SidebarPreferencesData,

    /// Preferred quick-reaction emojis.
    #[serde(default)]
    pub reactions: Vec<PreferredReaction>,

    /// AI provider credentials + custom provider config. Encrypted at the
    /// blob level (not separately) since the entire blob is NIP-44 encrypted.
    #[serde(default)]
    pub ai_credentials: AiProviderState,

    /// Notification "checked at" timestamp (unix seconds). Monotonic —
    /// `max(remote, local)` merge on apply.
    #[serde(default)]
    pub notifications_checked_at: u64,

    /// Cashu wallet terms acceptance version (`Some(v)` if accepted).
    #[serde(default)]
    pub cashu_terms_accepted: Option<u32>,

    /// Mostro P2P terms acceptance version (`Some(v)` if accepted).
    #[serde(default)]
    pub p2p_terms_accepted: Option<u32>,
}

fn default_version() -> u32 {
    1
}

impl Default for UserPrefsBlob {
    fn default() -> Self {
        Self {
            version: default_version(),
            settings: AppSettings::default(),
            sidebar: SidebarPreferencesData::default(),
            reactions: Vec::new(),
            ai_credentials: AiProviderState::default(),
            notifications_checked_at: 0,
            cashu_terms_accepted: None,
            p2p_terms_accepted: None,
        }
    }
}

impl UserPrefsBlob {
    /// Merge `remote` into `local`, returning the merged value.
    ///
    /// For `notifications_checked_at`, the maximum of local and remote wins
    /// (monotonic across devices). All other fields: remote wins (the event
    /// watermark in the SDK already ensures newer-wins at the event level;
    /// this is the field-level fallback).
    pub fn merge(local: &Self, remote: &Self) -> Self {
        let mut merged = remote.clone();
        merged.notifications_checked_at =
            local.notifications_checked_at.max(remote.notifications_checked_at);
        merged
    }
}
