pub mod clipboard;
pub mod download;
pub(crate) mod http;
pub mod spawn;
pub mod storage;
pub mod timer;
pub mod timestamp;

#[cfg(feature = "mobile")]
pub mod android_signer;
#[cfg(feature = "mobile")]
pub use android_signer::{IntentPollResult, Nip55Signer};

#[cfg(feature = "mobile")]
pub mod mobile;
#[cfg(feature = "mobile")]
pub use mobile::download_file;
