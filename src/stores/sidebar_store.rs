/// NIP-78: Sidebar Preferences Storage
/// Stores user's sidebar layout preferences on Nostr relays using kind 30078 events
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, Kind, Tag, FromBech32};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use strum::{EnumIter, IntoEnumIterator};

use crate::stores::{auth_store, nostr_client};
use crate::routes::Route;

/// All customizable sidebar navigation items
/// Auth-required items will be filtered at render time when user is logged out
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum SidebarItem {
    // Main sidebar items (default first 12)
    Home,
    Explore,
    Articles,
    Music,
    Photos,
    Videos,
    Live,
    Notifications,
    Messages,
    Bookmarks,
    Profile,
    Settings,

    // More menu items (default remaining 11)
    VoiceMessages,
    Polls,
    WebBookmarks,
    Podcasts,
    Radio,
    Wallet,
    P2PTrading,
    Communities,
    Events,
    Calendar,
    Recipes,
    PinBoards,

    // Available by default (not in active list, users can add them)
    Trending,
    Nips,
    Badges,
    Citations,
    Code,
    Lists,
    Dvm,
    Wiki,
    Publications,
    Shop,
    Blossom,
    Bible,
}

impl SidebarItem {
    /// Returns true if this item requires authentication
    pub fn requires_auth(&self) -> bool {
        matches!(
            self,
            SidebarItem::Photos
                | SidebarItem::Videos
                | SidebarItem::Live
                | SidebarItem::Profile
                | SidebarItem::Notifications
                | SidebarItem::Messages
                | SidebarItem::Bookmarks
                | SidebarItem::Settings
                | SidebarItem::Wallet
                | SidebarItem::VoiceMessages
                | SidebarItem::Polls
                | SidebarItem::WebBookmarks
                | SidebarItem::Lists
                | SidebarItem::Badges
                | SidebarItem::Citations
                | SidebarItem::Blossom
        )
    }

    /// Human-readable display label
    pub fn label(&self) -> &'static str {
        match self {
            SidebarItem::Home => "Home",
            SidebarItem::Explore => "Explore",
            SidebarItem::Articles => "Articles",
            SidebarItem::Music => "Music",
            SidebarItem::Photos => "Photos",
            SidebarItem::Videos => "Videos",
            SidebarItem::Live => "Live",
            SidebarItem::Notifications => "Notifications",
            SidebarItem::Messages => "Messages",
            SidebarItem::Bookmarks => "Bookmarks",
            SidebarItem::Profile => "Profile",
            SidebarItem::Settings => "Settings",
            SidebarItem::VoiceMessages => "Voice Messages",
            SidebarItem::Polls => "Polls",
            SidebarItem::WebBookmarks => "Web Bookmarks",
            SidebarItem::Podcasts => "Podcasts",
            SidebarItem::Radio => "Radio",
            SidebarItem::Wallet => "Wallet",
            SidebarItem::P2PTrading => "P2P Trading",
            SidebarItem::Communities => "Communities",
            SidebarItem::Events => "Events",
            SidebarItem::Calendar => "Calendar",
            SidebarItem::Recipes => "Recipes",
            SidebarItem::PinBoards => "Pinboards",
            SidebarItem::Trending => "Trending",
            SidebarItem::Nips => "NIPs",
            SidebarItem::Badges => "Badges",
            SidebarItem::Citations => "Citations",
            SidebarItem::Code => "Code",
            SidebarItem::Lists => "Lists",
            SidebarItem::Dvm => "DVM",
            SidebarItem::Wiki => "Wiki",
            SidebarItem::Publications => "Publications",
            SidebarItem::Shop => "Marketplace",
            SidebarItem::Blossom => "Blossom",
            SidebarItem::Bible => "Bible",
        }
    }

    /// Returns the Route for this sidebar item
    /// Note: Profile requires pubkey parameter, returns None if not available
    pub fn as_route(&self, pubkey: Option<&str>) -> Option<Route> {
        match self {
            SidebarItem::Home => Some(Route::Home { list: String::new() }),
            SidebarItem::Explore => Some(Route::Explore {}),
            SidebarItem::Articles => Some(Route::Articles {}),
            SidebarItem::Music => Some(Route::MusicHome {}),
            SidebarItem::Photos => Some(Route::Photos {}),
            SidebarItem::Videos => Some(Route::Videos {}),
            SidebarItem::Live => Some(Route::VideosLive {}),
            SidebarItem::Notifications => Some(Route::Notifications {}),
            SidebarItem::Messages => Some(Route::DMs {}),
            SidebarItem::Bookmarks => Some(Route::Bookmarks {}),
            SidebarItem::Profile => pubkey.map(|pk| Route::Profile { pubkey: pk.to_string() }),
            SidebarItem::Settings => Some(Route::Settings {}),
            SidebarItem::VoiceMessages => Some(Route::VoiceMessages {}),
            SidebarItem::Polls => Some(Route::Polls {}),
            SidebarItem::WebBookmarks => Some(Route::WebBookmarks {}),
            SidebarItem::Podcasts => Some(Route::PodcastHome {}),
            SidebarItem::Radio => Some(Route::RadioHome {}),
            SidebarItem::Wallet => Some(Route::CashuWallet {}),
            SidebarItem::P2PTrading => Some(Route::P2PHome {}),
            SidebarItem::Communities => Some(Route::Communities {}),
            SidebarItem::Events => Some(Route::Events {}),
            SidebarItem::Calendar => Some(Route::Calendar {}),
            SidebarItem::Recipes => Some(Route::RecipesHome {}),
            SidebarItem::PinBoards => Some(Route::PinBoardsHome {}),
            SidebarItem::Trending => Some(Route::Trending {}),
            SidebarItem::Nips => Some(Route::NipsHome {}),
            SidebarItem::Badges => Some(Route::BadgesHome {}),
            SidebarItem::Citations => Some(Route::CitationsHome {}),
            SidebarItem::Code => Some(Route::CodeHome {}),
            SidebarItem::Lists => Some(Route::Lists {}),
            SidebarItem::Dvm => Some(Route::DVM {}),
            SidebarItem::Wiki => Some(Route::WikiHome {}),
            SidebarItem::Publications => Some(Route::PublicationsHome {}),
            SidebarItem::Shop => Some(Route::ShopHome {}),
            SidebarItem::Blossom => Some(Route::BlossomPage {}),
            SidebarItem::Bible => Some(Route::BibleHome {}),
        }
    }
}

/// NIP-78 data structure for storing sidebar preferences
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidebarPreferencesData {
    /// Ordered list of active (visible) sidebar items
    pub active_items: Vec<SidebarItem>,
    /// Number of items to show in main sidebar (rest go to More menu)
    #[serde(default = "default_main_sidebar_count")]
    pub main_sidebar_count: usize,
    /// Version for future migrations
    #[serde(default)]
    pub version: u32,
}

fn default_main_sidebar_count() -> usize {
    DEFAULT_MAIN_SIDEBAR_SLOTS
}

impl Default for SidebarPreferencesData {
    fn default() -> Self {
        Self {
            active_items: default_sidebar_items(),
            main_sidebar_count: DEFAULT_MAIN_SIDEBAR_SLOTS,
            version: 1,
        }
    }
}

/// NIP-78 kind for arbitrary custom app data
const APP_DATA_KIND: u16 = 30078;

/// D tag identifier for sidebar preferences
const SIDEBAR_D_TAG: &str = "nostr.blue/sidebar";

/// Default main sidebar slot count (items beyond this go to More menu)
pub const DEFAULT_MAIN_SIDEBAR_SLOTS: usize = 12;

/// Maximum items allowed in main sidebar
pub const MAX_MAIN_SIDEBAR_SLOTS: usize = 20;

/// Default sidebar configuration (22 items in standard order)
pub fn default_sidebar_items() -> Vec<SidebarItem> {
    vec![
        // Main sidebar (first 12)
        SidebarItem::Home,
        SidebarItem::Explore,
        SidebarItem::Articles,
        SidebarItem::Music,
        SidebarItem::Podcasts,
        SidebarItem::Photos,
        SidebarItem::Videos,
        SidebarItem::Events,
        SidebarItem::Lists,
        SidebarItem::Notifications,
        SidebarItem::Messages,
        SidebarItem::Profile,
        // More menu (remaining 10)
        SidebarItem::P2PTrading,
        SidebarItem::VoiceMessages,
        SidebarItem::Polls,
        SidebarItem::Communities,
        SidebarItem::Radio,
        SidebarItem::PinBoards,
        SidebarItem::Wiki,
        SidebarItem::Recipes,
        SidebarItem::Shop,
        SidebarItem::Settings,
    ]
}

/// Global state for sidebar items
pub static SIDEBAR_ITEMS: GlobalSignal<Vec<SidebarItem>> = Signal::global(default_sidebar_items);
pub static SIDEBAR_SLOT_COUNT: GlobalSignal<usize> = Signal::global(|| DEFAULT_MAIN_SIDEBAR_SLOTS);
pub static SIDEBAR_LOADED: GlobalSignal<bool> = Signal::global(|| false);
pub static SIDEBAR_LOADING: GlobalSignal<bool> = Signal::global(|| false);

/// Get visible main sidebar items (up to slot count, filtered by auth)
pub fn get_main_sidebar_items(is_authenticated: bool) -> Vec<SidebarItem> {
    let slot_count = *SIDEBAR_SLOT_COUNT.read();
    SIDEBAR_ITEMS.read()
        .iter()
        .take(slot_count)
        .filter(|item| !item.requires_auth() || is_authenticated)
        .cloned()
        .collect()
}

/// Get visible More menu items (beyond slot count, filtered by auth)
pub fn get_more_menu_items(is_authenticated: bool) -> Vec<SidebarItem> {
    let slot_count = *SIDEBAR_SLOT_COUNT.read();
    SIDEBAR_ITEMS.read()
        .iter()
        .skip(slot_count)
        .filter(|item| !item.requires_auth() || is_authenticated)
        .cloned()
        .collect()
}

/// Get hidden/available items (all items not in active list)
pub fn get_available_items(active_items: &[SidebarItem]) -> Vec<SidebarItem> {
    let active_set: std::collections::HashSet<_> = active_items.iter().cloned().collect();
    SidebarItem::iter()
        .filter(|item| !active_set.contains(item))
        .collect()
}

/// Load sidebar preferences from Nostr relays (NIP-78)
pub async fn load_sidebar_preferences() {
    // Atomically check and set loading flag to avoid duplicate loads
    {
        let mut loading = SIDEBAR_LOADING.write();
        if *loading {
            return;
        }
        *loading = true;
    }

    log::info!("Loading sidebar preferences from Nostr (NIP-78)...");

    // Check if authenticated
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, using default sidebar");
        *SIDEBAR_LOADING.write() = false;
        *SIDEBAR_LOADED.write() = true;
        return;
    }

    // Get client and pubkey
    let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
        Some(c) => c.clone(),
        None => {
            log::warn!("Client not initialized");
            *SIDEBAR_LOADING.write() = false;
            *SIDEBAR_LOADED.write() = true;
            return;
        }
    };

    // Extract pubkey immediately and drop the read guard to avoid holding it across await points
    let pubkey_str = auth_store::AUTH_STATE.read().pubkey.clone();
    let pubkey = match pubkey_str.as_ref() {
        Some(pk_str) => {
            match nostr_sdk::PublicKey::from_bech32(pk_str)
                .or_else(|_| nostr_sdk::PublicKey::from_hex(pk_str))
            {
                Ok(pk) => pk,
                Err(e) => {
                    log::error!("Invalid pubkey: {}", e);
                    *SIDEBAR_LOADING.write() = false;
                    *SIDEBAR_LOADED.write() = true;
                    return;
                }
            }
        }
        None => {
            log::warn!("No pubkey available");
            *SIDEBAR_LOADING.write() = false;
            *SIDEBAR_LOADED.write() = true;
            return;
        }
    };

    // Build filter for sidebar preferences event
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(SIDEBAR_D_TAG)
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    // Fetch sidebar preferences event
    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found sidebar preference event: {}", event.id);

                // Parse preferences from content
                match serde_json::from_str::<SidebarPreferencesData>(&event.content) {
                    Ok(data) => {
                        if !data.active_items.is_empty() {
                            log::info!("Loaded {} sidebar items with {} main slots from Nostr",
                                data.active_items.len(), data.main_sidebar_count);
                            *SIDEBAR_ITEMS.write() = data.active_items;
                            *SIDEBAR_SLOT_COUNT.write() = data.main_sidebar_count;
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to parse sidebar data: {}", e);
                    }
                }
            } else {
                log::info!("No sidebar preferences found on Nostr, using defaults");
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch sidebar preferences: {}", e);
        }
    }

    *SIDEBAR_LOADING.write() = false;
    *SIDEBAR_LOADED.write() = true;
}

/// Save sidebar preferences to Nostr relays (NIP-78)
pub async fn save_sidebar_preferences(items: Vec<SidebarItem>, slot_count: usize) -> Result<(), String> {
    log::info!("Saving {} sidebar items with {} main slots to Nostr (NIP-78)...", items.len(), slot_count);

    // Check if authenticated
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    // Get client
    let client = nostr_client::NOSTR_CLIENT.read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Ensure relays are ready before publishing
    nostr_client::ensure_relays_ready(&client).await;

    // Create data structure
    let data = SidebarPreferencesData {
        active_items: items.clone(),
        main_sidebar_count: slot_count,
        version: 1,
    };

    // Serialize to JSON
    let content = serde_json::to_string(&data)
        .map_err(|e| format!("Failed to serialize sidebar data: {}", e))?;

    // Build NIP-78 event (kind 30078 with 'd' tag)
    let builder = EventBuilder::new(Kind::from(APP_DATA_KIND), content)
        .tag(Tag::identifier(SIDEBAR_D_TAG));

    // Publish to relays
    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish sidebar preferences: {}", e))?;

    log::info!("Sidebar preferences saved to Nostr successfully");

    // Update global state
    *SIDEBAR_ITEMS.write() = items;
    *SIDEBAR_SLOT_COUNT.write() = slot_count;

    Ok(())
}
