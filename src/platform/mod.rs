// Compile-time exclusivity guard: ensure exactly one platform is enabled
#[cfg(all(feature = "web", feature = "desktop"))]
compile_error!("Cannot enable both 'web' and 'desktop' features");

#[cfg(all(feature = "web", feature = "mobile"))]
compile_error!("Cannot enable both 'web' and 'mobile' features");

#[cfg(all(feature = "desktop", feature = "mobile"))]
compile_error!("Cannot enable both 'desktop' and 'mobile' features");

#[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
compile_error!("Must enable exactly one of 'web', 'desktop', or 'mobile' feature");

pub mod clipboard;
pub mod download;
pub mod future;
pub(crate) mod http;
pub mod lightning;
pub mod spawn;
pub mod storage;
pub mod timer;
pub mod timestamp;

pub use lightning::open_lightning_invoice;

#[cfg(feature = "mobile")]
pub mod android_signer;
#[cfg(feature = "mobile")]
pub use android_signer::{IntentPollResult, Nip55Signer};

#[cfg(feature = "mobile")]
pub mod mobile;
#[cfg(feature = "mobile")]
pub use mobile::download_file;

#[cfg(feature = "mobile")]
pub mod android_media;
