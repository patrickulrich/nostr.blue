//! On-device media downloads layer (podcasts + music) for Android and Linux.
//!
//! Structure:
//! - `model`    — cross-platform types (compiles on web for the store/progress)
//! - `store`    — reactive global state + device-local settings
//! - `progress` — continue-listening positions (native: SQLite, web: localStorage)
//! - `db`       — SQLite persistence (native only)
//! - `manager`  — download engine: queue, streaming, pause/resume, eviction (native only)
//! - `resolver` — remote → local URL rewriting for playback (native only)
//! - `server`   — embedded localhost HTTP server with Range support (desktop only)
//! - `sync`     — podcast library sync + auto-download + Android Auto mirrors (native only)

pub mod model;
pub mod progress;
pub mod store;

#[cfg(feature = "native")]
pub mod db;
#[cfg(feature = "native")]
pub mod manager;
#[cfg(feature = "native")]
pub mod resolver;
#[cfg(feature = "native")]
pub mod sync;

#[cfg(feature = "desktop")]
pub mod server;
