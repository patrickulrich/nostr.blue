//! Unified signer management for all authentication methods
use std::fmt;
use std::pin::Pin;
use std::future::Future;

use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr::event::{Event, UnsignedEvent};
use nostr::key::{AsyncGetPublicKey, Keys, PublicKey};
use nostr::nips::nip04::AsyncNip04;
use nostr::nips::nip44::AsyncNip44;
use nostr::event::AsyncSignEvent;
#[cfg(feature = "mobile_platform")]
use crate::platform::Nip55Signer;
#[cfg(target_family = "wasm")]
use nostr_browser_signer::BrowserSigner;
use nostr_connect::client::NostrConnect;
use nostr_sdk::authenticator::Authenticator;
use nostr_sdk::error::Error as SdkError;
use nostr::types::url::RelayUrl;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Composite signer abstraction over nostr 0.45's decomposed signing traits.
///
/// The SDK removed `NostrSigner` in favor of `AsyncGetPublicKey`,
/// `AsyncSignEvent`, `AsyncNip44` and `AsyncNip04`. All nostr.blue backends
/// implement those four traits; this supertrait (with a blanket impl) gives
/// the app a single `Arc<dyn BlueSigner>` object again. Errors are normalized
/// to `String`, matching how every call site already formats them.
#[async_trait::async_trait]
pub trait BlueSigner: Send + Sync {
    async fn get_public_key(&self) -> Result<PublicKey, String>;
    async fn sign_event(&self, unsigned: UnsignedEvent) -> Result<Event, String>;
    async fn nip44_encrypt(&self, key: &PublicKey, plaintext: &str) -> Result<String, String>;
    async fn nip44_decrypt(&self, key: &PublicKey, payload: &str) -> Result<String, String>;
    #[allow(dead_code)]
    async fn nip04_encrypt(&self, key: &PublicKey, plaintext: &str) -> Result<String, String>;
    async fn nip04_decrypt(&self, key: &PublicKey, payload: &str) -> Result<String, String>;
}

#[async_trait::async_trait]
impl<T> BlueSigner for T
where
    T: AsyncGetPublicKey + AsyncSignEvent + AsyncNip44 + AsyncNip04 + Sync,
{
    async fn get_public_key(&self) -> Result<PublicKey, String> {
        self.get_public_key_async().await.map_err(|e| e.to_string())
    }
    async fn sign_event(&self, unsigned: UnsignedEvent) -> Result<Event, String> {
        self.sign_event_async(unsigned).await.map_err(|e| e.to_string())
    }
    async fn nip44_encrypt(&self, key: &PublicKey, plaintext: &str) -> Result<String, String> {
        self.nip44_encrypt_async(key, plaintext)
            .await
            .map_err(|e| e.to_string())
    }
    async fn nip44_decrypt(&self, key: &PublicKey, payload: &str) -> Result<String, String> {
        self.nip44_decrypt_async(key, payload)
            .await
            .map_err(|e| e.to_string())
    }
    async fn nip04_encrypt(&self, key: &PublicKey, plaintext: &str) -> Result<String, String> {
        self.nip04_encrypt_async(key, plaintext)
            .await
            .map_err(|e| e.to_string())
    }
    async fn nip04_decrypt(&self, key: &PublicKey, payload: &str) -> Result<String, String> {
        self.nip04_decrypt_async(key, payload)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Unwrap a NIP-59 gift wrap using the unified [`BlueSigner`] abstraction.
///
/// Replacement for the removed `Client::unwrap_gift_wrap` SDK method, following
/// the upstream `UnwrappedGift::from_gift_wrap` logic: check the event kind,
/// verify the outer event, decrypt + verify the seal, decrypt the rumor and
/// ensure the rumor author matches the seal author.
pub async fn unwrap_gift_wrap(
    signer: &dyn BlueSigner,
    gift_wrap: &Event,
) -> Result<nostr::nips::nip59::UnwrappedGift, String> {
    use nostr::event::Kind;
    if gift_wrap.kind != Kind::GiftWrap {
        return Err(format!("not a gift wrap event: kind {}", gift_wrap.kind));
    }
    gift_wrap.verify().map_err(|e| e.to_string())?;
    let seal_json = signer
        .nip44_decrypt(&gift_wrap.pubkey, &gift_wrap.content)
        .await?;
    let seal: Event = serde_json::from_str(&seal_json).map_err(|e| e.to_string())?;
    seal.verify().map_err(|e| e.to_string())?;
    let rumor_json = signer.nip44_decrypt(&seal.pubkey, &seal.content).await?;
    let rumor: UnsignedEvent = serde_json::from_str(&rumor_json).map_err(|e| e.to_string())?;
    if rumor.pubkey != seal.pubkey {
        return Err("gift wrap sender mismatch: rumor pubkey != seal pubkey".to_string());
    }
    Ok(nostr::nips::nip59::UnwrappedGift {
        sender: seal.pubkey,
        rumor,
    })
}

/// NIP-42 authenticator adapter over [`BlueSigner`].
///
/// `SignerAuthenticator` is generic over the concrete signer type; this
/// adapter lets the rebuilt `Client` authenticate with whichever backend is
/// active behind `Arc<dyn BlueSigner>`.
pub struct BlueAuthenticator(pub Arc<dyn BlueSigner>);

impl fmt::Debug for BlueAuthenticator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlueAuthenticator").finish()
    }
}

impl Authenticator for BlueAuthenticator {
    fn make_auth_event<'a>(
        &'a self,
        relay_url: &'a RelayUrl,
        challenge: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Event, SdkError>> + Send + 'a>> {
        Box::pin(async move {
            use nostr::event::FinalizeUnsignedEvent;
            use nostr::nips::nip42::ClientAuthentication;
            let public_key = self
                .0
                .get_public_key()
                .await
                .map_err(SdkError::other)?;
            let unsigned = ClientAuthentication::new(challenge, relay_url.clone())
                .finalize_unsigned(public_key);
            let event = self.0.sign_event(unsigned).await.map_err(SdkError::other)?;
            Ok(event)
        })
    }
}

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
    /// Android signer (NIP-55)
    #[cfg(feature = "mobile_platform")]
    AndroidSigner(Arc<Nip55Signer>),
}

impl SignerType {
    /// Get the public key for this signer.
    /// For Keys signers, returns immediately from memory.
    /// For external signers, falls back to the async call.
    pub async fn public_key(&self) -> Result<PublicKey, String> {
        match self {
            SignerType::Keys(keys) => Ok(keys.public_key()),
            #[cfg(target_family = "wasm")]
            SignerType::BrowserExtension(_) => {
                self.as_nostr_signer().get_public_key().await
            }
            SignerType::NostrConnect(_) => self.as_nostr_signer().get_public_key().await,
            #[cfg(feature = "mobile_platform")]
            SignerType::AndroidSigner(_) => self.as_nostr_signer().get_public_key().await,
        }
    }
    /// Get the signer backend type as a string
    pub fn backend_name(&self) -> &'static str {
        match self {
            SignerType::Keys(_) => "Keys",
            #[cfg(target_family = "wasm")]
            SignerType::BrowserExtension(_) => "Browser Extension",
            SignerType::NostrConnect(_) => "Remote Signer",
            #[cfg(feature = "mobile_platform")]
            SignerType::AndroidSigner(_) => "Android Signer",
        }
    }
    /// Convert into `Arc<dyn BlueSigner>`
    #[allow(dead_code)]
    pub fn into_nostr_signer(self) -> Arc<dyn BlueSigner> {
        match self {
            SignerType::Keys(keys) => Arc::new(keys),
            #[cfg(target_family = "wasm")]
            SignerType::BrowserExtension(signer) => signer,
            SignerType::NostrConnect(nostr_connect) => nostr_connect,
            #[cfg(feature = "mobile_platform")]
            SignerType::AndroidSigner(signer) => signer,
        }
    }
    /// Get a reference as `Arc<dyn BlueSigner>`
    pub fn as_nostr_signer(&self) -> Arc<dyn BlueSigner> {
        match self {
            SignerType::Keys(keys) => Arc::new(keys.clone()),
            #[cfg(target_family = "wasm")]
            SignerType::BrowserExtension(signer) => signer.clone(),
            SignerType::NostrConnect(nostr_connect) => nostr_connect.clone(),
            #[cfg(feature = "mobile_platform")]
            SignerType::AndroidSigner(signer) => signer.clone(),
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
    #[cfg(feature = "mobile_platform")]
    AndroidSigner,
}
impl SignerBackend {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            SignerBackend::Keys => "keys",
            #[cfg(target_family = "wasm")]
            SignerBackend::BrowserExtension => "browser_extension",
            SignerBackend::RemoteSigner => "remote_signer",
            #[cfg(feature = "mobile_platform")]
            SignerBackend::AndroidSigner => "android_signer",
        }
    }
}
/// Global signal for the current signer
pub static CURRENT_SIGNER: GlobalSignal<Option<SignerType>> = Signal::global(|| None);
/// Global signal for signer info (persisted)
pub static SIGNER_INFO: GlobalSignal<Option<SignerInfo>> =
    Signal::global(|| crate::platform::storage::get("signer_info").ok());
/// Set the current signer with an already-known public key (avoids redundant external signer call)
pub async fn set_signer_with_pubkey(
    signer: SignerType,
    public_key: PublicKey,
) -> Result<(), String> {
    let backend = match &signer {
        SignerType::Keys(_) => SignerBackend::Keys,
        #[cfg(target_family = "wasm")]
        SignerType::BrowserExtension(_) => SignerBackend::BrowserExtension,
        SignerType::NostrConnect(_) => SignerBackend::RemoteSigner,
        #[cfg(feature = "mobile_platform")]
        SignerType::AndroidSigner(_) => SignerBackend::AndroidSigner,
    };
    let info = SignerInfo {
        public_key: public_key.to_string(),
        backend,
    };
    crate::platform::storage::set("signer_info", &info)
        .map_err(|e| format!("Failed to persist signer info: {}", e))?;
    *SIGNER_INFO.write() = Some(info);
    *CURRENT_SIGNER.write() = Some(signer);
    Ok(())
}

/// Set the current signer and persist session info
///
/// Prefer `set_signer_with_pubkey` when the public key is already known
/// to avoid an unnecessary external signer call.
#[allow(dead_code)]
pub async fn set_signer(signer: SignerType) -> Result<(), String> {
    let public_key = signer.public_key().await?;
    set_signer_with_pubkey(signer, public_key).await
}
/// Clear the current signer and remove persisted session
#[allow(dead_code)]
pub fn clear_signer() {
    let _ = crate::platform::storage::delete("signer_info");
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
            SignerBackend::BrowserExtension => match BrowserSigner::new() {
                Ok(signer) => {
                    let signer_type = SignerType::BrowserExtension(Arc::new(signer));
                    let pk = signer_type.public_key().await?;
                    if pk.to_string() == info.public_key {
                        *CURRENT_SIGNER.write() = Some(signer_type);
                        return Ok(());
                    } else {
                        clear_signer();
                        return Err("Public key mismatch with stored session".to_string());
                    }
                }
                Err(_) => {
                    clear_signer();
                    return Err("Browser extension no longer available".to_string());
                }
            },
            SignerBackend::Keys => {
                clear_signer();
                return Err("Private key session requires re-login".to_string());
            }
            SignerBackend::RemoteSigner => {
                clear_signer();
                return Err("Remote signer session requires re-login".to_string());
            }
            #[cfg(feature = "mobile_platform")]
            SignerBackend::AndroidSigner => {
                let pubkey = PublicKey::parse(&info.public_key)
                    .map_err(|e| format!("Invalid stored public key: {}", e))?;
                let package = crate::platform::storage::get::<String>("signer_package")
                    .unwrap_or_else(|_| Nip55Signer::default_package().to_string());
                let signer = Nip55Signer::new(pubkey, package);
                let signer_type = SignerType::AndroidSigner(Arc::new(signer));
                *CURRENT_SIGNER.write() = Some(signer_type);
                return Ok(());
            }
        }
    }
    Ok(())
}
