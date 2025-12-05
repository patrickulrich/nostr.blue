// Global state management
// Stores provide shared state across the application

pub mod nostr_client;
pub mod auth_store;
pub mod theme_store;
pub mod signer;
pub mod bookmarks;
pub mod dms;
pub mod notifications;
pub mod profiles;
pub mod settings_store;
pub mod blossom_store;
pub mod music_player;
pub mod nostr_music;
pub mod emoji_store;
pub mod gif_store;
pub mod relay_metadata;
pub mod voice_messages_store;
pub mod webbookmarks;
pub mod cashu_cdk_bridge;
pub mod cashu;  // NIP-60 Cashu wallet
pub mod nwc_store;
pub mod indexeddb_database;
pub mod reactions_store;  // NIP-78 preferred reactions
pub mod dvm_store;  // NIP-90 Data Vending Machines
pub mod nip96_store;  // NIP-96 HTTP File Storage

