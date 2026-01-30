// Global state management
// Stores provide shared state across the application

pub mod auth_store;
#[cfg(target_arch = "wasm32")]
pub mod bible_store;
pub mod blossom_store;
pub mod bookmarks;
pub mod calendar_store; // NIP-52 Calendar events + NIP-53 Live activities
pub mod cashu; // NIP-60 Cashu wallet
pub mod cashu_cdk_bridge;
pub mod citation_store; // NKBIP-03 Citations (kinds 30-33)
pub mod code_store; // NIP-34 Git hosting + NIP-C0 Code snippets
pub mod community_store; // NIP-72 Moderated Communities
pub mod directory_store; // NKBIP-04 Directory System (kinds 30042-30045)
pub mod dms;
pub mod draft_store; // NIP-37 Draft Wraps (kind 31234) + Private Relays (kind 10013)
pub mod dvm_store; // NIP-90 Data Vending Machines
pub mod embedding_store; // NKBIP-02 Embeddings (kind 1987)
pub mod emoji_store;
pub mod feed_cache; // Feed cache service API
pub mod feed_cache_db; // IndexedDB feed cache database
pub mod gif_store;
pub mod grasp_servers; // NIP-34 GRASP server discovery
pub mod indexeddb_database;
pub mod music_player;
pub mod nip96_store; // NIP-96 HTTP File Storage
pub mod nostr_client;
pub mod nostr_music;
pub mod notifications;
pub mod nwc_store;
pub mod p2p_store; // NIP-69 P2P orders
pub mod pending_comments; // Optimistic updates for comments
pub mod pin_boards_store; // Kind 33889 Pin Boards (Pinstr compatible)
pub mod pinned_communities; // NIP-51 Pinned Communities (kind 10004)
pub mod pinned_notes; // NIP-51 Pinned Notes (kind 10001)
pub mod podcast_subscription; // NIP-51 Podcast Subscriptions (kind 30003)
pub mod profiles;
pub mod publication_store; // NKBIP-01 Publications (kind 30040/30041)
pub mod reactions_store; // NIP-78 preferred reactions
pub mod recipe_store; // Kind 30023 Recipes
pub mod relay; // Organized relay management (signals, pool, connection, nip65, display)
pub mod settings_store;
pub mod shop_store; // NIP-99 Marketplace (products, collections, orders)
pub mod sidebar_store; // NIP-78 sidebar layout preferences
pub mod signer;
pub mod subscription_manager; // SDK subscription auto-close helpers
pub mod theme_store;
pub mod voice_messages_store;
pub mod webbookmarks;
pub mod wiki_store; // NIP-54 Wiki (kind 30818) // NIP-84 Bible highlights (kind 9802) + HelloAO API (WASM-only)
