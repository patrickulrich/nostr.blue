// Copyright (c) 2025 Patrick Ulrich
// Distributed under the MIT software license

//! Unified signer management for all authentication methods

use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use gloo_storage::{LocalStorage, Storage};
use nostr::{Keys, PublicKey};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[cfg(target_family = "wasm")]
use nostr_browser_signer::BrowserSigner;

use nostr_connect::client::NostrConnect;

/// Types of signers supported by the application
#[derive(Debug, Clone)]
pub enum SignerType {
    /// Private key signer (nsec)
    Keys(Keys),
    /// Browser extension signer (NIP-07)
    #[cfg(target_family = "wasm")]
    BrowserExtension(Arc<BrowserSigner>),
    /// Remote signer (NIP-46)
    NostrConnect(Arc<NostrConnect>),
}

impl SignerType {
    /// Get the public key for this signer
    pub async fn public_key(&self) -> Result<PublicKey, String> {
        match self {
            SignerType::Keys(keys) => Ok(keys.public_key()),
            #[cfg(target_family = "wasm")]
            SignerType::BrowserExtension(signer) => {
                use nostr::signer::NostrSigner;
                signer
                    .get_public_key()
                    .await
                    .map_err(|e| format!("Failed to get public key from browser extension: {}", e))
            }
            SignerType::NostrConnect(nostr_connect) => {
                use nostr::signer::NostrSigner;
                nostr_connect
                    .get_public_key()
                    .await
                    .map_err(|e| format!("Failed to get public key from remote signer: {}", e))
            }
        }
    }

    /// Get the signer backend type as a string
    pub fn backend_name(&self) -> &'static str {
        match self {
            SignerType::Keys(_) => "Keys",
            #[cfg(target_family = "wasm")]
            SignerType::BrowserExtension(_) => "Browser Extension",
            SignerType::NostrConnect(_) => "Remote Signer",
        }
    }

    /// Convert to Arc<dyn NostrSigner> for use with Client
    #[allow(dead_code)]
    pub fn into_nostr_signer(self) -> Arc<dyn nostr::signer::NostrSigner> {
        match self {
            SignerType::Keys(keys) => Arc::new(keys),
            #[cfg(target_family = "wasm")]
            SignerType::BrowserExtension(signer) => signer,
            SignerType::NostrConnect(nostr_connect) => nostr_connect,
        }
    }

    /// Get a reference as Arc<dyn NostrSigner>
    pub fn as_nostr_signer(&self) -> Arc<dyn nostr::signer::NostrSigner> {
        match self {
            SignerType::Keys(keys) => Arc::new(keys.clone()),
            #[cfg(target_family = "wasm")]
            SignerType::BrowserExtension(signer) => signer.clone(),
            SignerType::NostrConnect(nostr_connect) => nostr_connect.clone(),
        }
    }
}

/// Persisted signer information for session restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignerInfo {
    pub public_key: String,
    pub backend: SignerBackend,
}

/// Signer backend types for persistence
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignerBackend {
    Keys,
    #[cfg(target_family = "wasm")]
    BrowserExtension,
    RemoteSigner,
}

impl SignerBackend {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            SignerBackend::Keys => "keys",
            #[cfg(target_family = "wasm")]
            SignerBackend::BrowserExtension => "browser_extension",
            SignerBackend::RemoteSigner => "remote_signer",
        }
    }
}

/// Global signal for the current signer
pub static CURRENT_SIGNER: GlobalSignal<Option<SignerType>> = Signal::global(|| None);

/// Global signal for signer info (persisted)
pub static SIGNER_INFO: GlobalSignal<Option<SignerInfo>> = Signal::global(|| {
    LocalStorage::get("signer_info").ok()
});

/// Set the current signer and persist session info
pub async fn set_signer(signer: SignerType) -> Result<(), String> {
    // Get public key for persistence
    let public_key = signer.public_key().await?;

    // Determine backend type
    let backend = match &signer {
        SignerType::Keys(_) => SignerBackend::Keys,
        #[cfg(target_family = "wasm")]
        SignerType::BrowserExtension(_) => SignerBackend::BrowserExtension,
        SignerType::NostrConnect(_) => SignerBackend::RemoteSigner,
    };

    // Create signer info
    let info = SignerInfo {
        public_key: public_key.to_string(),
        backend,
    };

    // Persist to localStorage
    LocalStorage::set("signer_info", &info)
        .map_err(|e| format!("Failed to persist signer info: {}", e))?;

    // Update global signals
    *SIGNER_INFO.write() = Some(info);
    *CURRENT_SIGNER.write() = Some(signer);

    Ok(())
}

/// Clear the current signer and remove persisted session
#[allow(dead_code)]
pub fn clear_signer() {
    LocalStorage::delete("signer_info");
    *SIGNER_INFO.write() = None;
    *CURRENT_SIGNER.write() = None;
}

/// Get the current signer
#[allow(dead_code)]
pub fn get_signer() -> Option<SignerType> {
    CURRENT_SIGNER.read().clone()
}

/// Get the current signer info
#[allow(dead_code)]
pub fn get_signer_info() -> Option<SignerInfo> {
    SIGNER_INFO.read().clone()
}

/// Check if a signer is currently set
#[allow(dead_code)]
pub fn has_signer() -> bool {
    CURRENT_SIGNER.read().is_some()
}

/// Initialize signer from stored session (call on app startup)
#[allow(dead_code)]
pub async fn restore_session() -> Result<(), String> {
    if let Some(info) = get_signer_info() {
        match info.backend {
            #[cfg(target_family = "wasm")]
            SignerBackend::BrowserExtension => {
                // Try to restore browser extension signer
                match BrowserSigner::new() {
                    Ok(signer) => {
                        let signer_type = SignerType::BrowserExtension(Arc::new(signer));
                        // Verify the public key matches
                        let pk = signer_type.public_key().await?;
                        if pk.to_string() == info.public_key {
                            *CURRENT_SIGNER.write() = Some(signer_type);
                            return Ok(());
                        } else {
                            // Public key mismatch, clear session
                            clear_signer();
                            return Err("Public key mismatch with stored session".to_string());
                        }
                    }
                    Err(_) => {
                        // Browser extension not available, clear session
                        clear_signer();
                        return Err("Browser extension no longer available".to_string());
                    }
                }
            }
            SignerBackend::Keys => {
                // Keys-based auth requires re-login (we don't persist private keys)
                clear_signer();
                return Err("Private key session requires re-login".to_string());
            }
            SignerBackend::RemoteSigner => {
                // Remote signer session requires re-login (handled in auth_store)
                clear_signer();
                return Err("Remote signer session requires re-login".to_string());
            }
        }
    }
    Ok(())
}
