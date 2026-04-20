use crate::platform::storage;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client};
/// NIP-78: Sidebar Preferences Storage
/// Stores user's sidebar layout preferences on Nostr relays using kind 30078 events
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, FromBech32, Kind, Tag};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use strum::{EnumIter, IntoEnumIterator};
/// State for NIP-78 data loading operations
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Nip78LoadState {
    /// Initial state, not yet attempted
    #[default]
    Pending,
    /// Currently loading from relays
    Loading,
    /// Successfully loaded from relays
    Loaded,
    /// Using defaults (not authenticated, no data found, or parse error)
    LoadedDefaults,
    /// Network/fetch error - candidate for retry when relay connects
    Failed(String),
}
impl Nip78LoadState {
    /// Returns true if data is ready (loaded, defaults, or pending with defaults)
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Loaded | Self::LoadedDefaults | Self::Pending)
    }
    /// Returns true if load failed and should be retried
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
    /// Returns true if currently loading (prevents duplicate loads)
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}
/// All customizable sidebar navigation items
/// Auth-required items will be filtered at render time when user is logged out
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum SidebarItem {
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
    VoiceMessages,
    Polls,
    WebBookmarks,
    Podcasts,
    Radio,
    Wallet,
    P2PTrading,
    Communities,
    Topics,
    Events,
    Calendar,
    Recipes,
    PinBoards,
    Trending,
    Nips,
    Badges,
    Citations,
    Code,
    Lists,
    Packs,
    Chats,
    Dvm,
    Wiki,
    Publications,
    Shop,
    Blossom,
    Bible,
    Highlights,
    AIChat,
    Blobbi,
}
impl SidebarItem {
    /// Returns true if this item requires authentication
    pub fn requires_auth(&self) -> bool {
        match self {
            SidebarItem::Photos
                | SidebarItem::Videos
                | SidebarItem::Live
                | SidebarItem::Profile
                | SidebarItem::Notifications
                | SidebarItem::Messages
                | SidebarItem::Bookmarks
                | SidebarItem::Settings
                | SidebarItem::VoiceMessages
                | SidebarItem::Polls
                | SidebarItem::WebBookmarks
                | SidebarItem::Lists
                | SidebarItem::Badges
                | SidebarItem::Citations
            | SidebarItem::Blossom
            | SidebarItem::AIChat
            | SidebarItem::Blobbi => true,
            #[cfg(feature = "cashu")]
            SidebarItem::Wallet => true,
            _ => false,
        }
    }
    /// Items temporarily hidden from sidebar and customizer.
    /// Routes remain accessible via direct navigation.
    pub fn is_hidden(&self) -> bool {
        matches!(
            self,
            SidebarItem::Citations
                | SidebarItem::WebBookmarks
                | SidebarItem::Trending
                | SidebarItem::Nips
                | SidebarItem::Dvm
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
            #[cfg(feature = "cashu")]
            SidebarItem::Wallet => "Wallet",
            #[cfg(not(feature = "cashu"))]
            SidebarItem::Wallet => "",
            SidebarItem::P2PTrading => "P2P Trading",
            SidebarItem::Communities => "Communities",
            SidebarItem::Topics => "Topics",
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
            SidebarItem::Packs => "Packs",
            SidebarItem::Chats => "Chats",
            SidebarItem::Dvm => "DVM",
            SidebarItem::Wiki => "Wiki",
            SidebarItem::Publications => "Publications",
            SidebarItem::Shop => "Marketplace",
            SidebarItem::Blossom => "Blossom",
            SidebarItem::Bible => "Bible",
            SidebarItem::Highlights => "Highlights",
            SidebarItem::AIChat => "AI Chat",
            SidebarItem::Blobbi => "Blobbi",
        }
    }
    /// Returns the Route for this sidebar item
    /// Note: Profile requires pubkey parameter, returns None if not available
    pub fn as_route(&self, pubkey: Option<&str>) -> Option<Route> {
        match self {
            SidebarItem::Home => Some(Route::Home {
                list: String::new(),
            }),
            SidebarItem::Explore => Some(Route::Explore {}),
            SidebarItem::Articles => Some(Route::Articles {}),
            SidebarItem::Music => Some(Route::MusicHome {}),
            SidebarItem::Photos => Some(Route::Photos {}),
            SidebarItem::Videos => Some(Route::Videos {}),
            SidebarItem::Live => Some(Route::VideosLive {}),
            SidebarItem::Notifications => Some(Route::Notifications {}),
            SidebarItem::Messages => Some(Route::DMs {}),
            SidebarItem::Bookmarks => Some(Route::Bookmarks {}),
            SidebarItem::Profile => pubkey.map(|pk| Route::Profile {
                pubkey: pk.to_string(),
            }),
            SidebarItem::Settings => Some(Route::Settings {}),
            SidebarItem::VoiceMessages => Some(Route::VoiceMessages {}),
            SidebarItem::Polls => Some(Route::Polls {}),
            SidebarItem::WebBookmarks => Some(Route::WebBookmarks {}),
            SidebarItem::Podcasts => Some(Route::PodcastHome {}),
            SidebarItem::Radio => Some(Route::RadioHome {}),
            #[cfg(feature = "cashu")]
            SidebarItem::Wallet => Some(Route::CashuWallet {}),
            #[cfg(not(feature = "cashu"))]
            SidebarItem::Wallet => None,
            SidebarItem::P2PTrading => Some(Route::P2PHome {}),
            SidebarItem::Communities => Some(Route::Communities {}),
            SidebarItem::Topics => Some(Route::TopicsHome {}),
            SidebarItem::Events => Some(Route::Events {}),
            SidebarItem::Calendar => Some(Route::Calendar {}),
            SidebarItem::Recipes => Some(Route::RecipesHome {}),
            SidebarItem::PinBoards => Some(Route::PinBoardsHome {}),
            SidebarItem::Trending => Some(Route::Trending { source: None }),
            SidebarItem::Nips => Some(Route::NipsHome {}),
            SidebarItem::Badges => Some(Route::BadgesHome {}),
            SidebarItem::Citations => Some(Route::CitationsHome {}),
            SidebarItem::Code => Some(Route::CodeHome {}),
            SidebarItem::Lists => Some(Route::Lists {}),
            SidebarItem::Packs => Some(Route::PacksHome {}),
            SidebarItem::Chats => Some(Route::Chats {}),
            SidebarItem::Dvm => Some(Route::DVM {}),
            SidebarItem::Wiki => Some(Route::WikiHome {}),
            SidebarItem::Publications => Some(Route::PublicationsHome {}),
            SidebarItem::Shop => Some(Route::ShopHome {}),
            SidebarItem::Blossom => Some(Route::BlossomPage {}),
            SidebarItem::Bible => Some(Route::BibleHome {}),
            SidebarItem::Highlights => Some(Route::Highlights {}),
            SidebarItem::AIChat => Some(Route::AIChat {}),
            SidebarItem::Blobbi => Some(Route::BlobbiHome {}),
        }
    }
}
/// NIP-78 data structure for storing sidebar preferences (v2)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidebarPreferencesData {
    /// Ordered list of all sidebar items (v2: all variants included)
    #[serde(alias = "active_items")]
    pub items_order: Vec<SidebarItem>,
    /// Number of items to show per page
    #[serde(alias = "main_sidebar_count", default = "default_items_per_page")]
    pub items_per_page: usize,
    /// Version for future migrations
    #[serde(default)]
    pub version: u32,
}
fn default_items_per_page() -> usize {
    DEFAULT_MAIN_SIDEBAR_SLOTS
}
impl Default for SidebarPreferencesData {
    fn default() -> Self {
        Self {
            items_order: default_sidebar_items(),
            items_per_page: DEFAULT_MAIN_SIDEBAR_SLOTS,
            version: 2,
        }
    }
}
impl SidebarPreferencesData {
    /// Migrate v1 format to v2 by appending any missing variants
    pub fn migrate_to_v2(mut self) -> Self {
        if self.version < 2 {
            self.version = 2;
        }
        // Always ensure all non-hidden variants present (handles new variants added post-v2)
        for variant in SidebarItem::iter() {
            if variant.is_hidden() {
                continue;
            }
            if !self.items_order.contains(&variant) {
                self.items_order.push(variant);
            }
        }
        // Strip any hidden items from stored preferences
        self.items_order.retain(|item| !item.is_hidden());
        // Always validate items_per_page regardless of version
        if self.items_per_page == 0 || self.items_per_page > MAX_MAIN_SIDEBAR_SLOTS {
            self.items_per_page = DEFAULT_MAIN_SIDEBAR_SLOTS;
        }
        self
    }
}
/// NIP-78 kind for arbitrary custom app data
const APP_DATA_KIND: u16 = 30078;
/// D tag identifier for sidebar preferences
const SIDEBAR_D_TAG: &str = "nostr.blue/sidebar";
/// localStorage key for caching sidebar preferences
const SIDEBAR_LOCAL_STORAGE_KEY: &str = "nostr_blue_sidebar_prefs";
/// Default main sidebar slot count (items beyond this go to More menu)
pub const DEFAULT_MAIN_SIDEBAR_SLOTS: usize = 12;
/// Maximum items allowed in main sidebar
pub const MAX_MAIN_SIDEBAR_SLOTS: usize = 20;
/// Default sidebar configuration — all variants in a sensible default order
pub fn default_sidebar_items() -> Vec<SidebarItem> {
    vec![
        SidebarItem::Home,
        SidebarItem::Explore,
        SidebarItem::Articles,
        SidebarItem::Music,
        SidebarItem::Podcasts,
        SidebarItem::Photos,
        SidebarItem::Videos,
        SidebarItem::Events,
        SidebarItem::Lists,
        SidebarItem::Packs,
        SidebarItem::Notifications,
        SidebarItem::Messages,
        SidebarItem::Profile,
        SidebarItem::P2PTrading,
        SidebarItem::VoiceMessages,
        SidebarItem::Polls,
        SidebarItem::Chats,
        SidebarItem::Communities,
        SidebarItem::Topics,
        SidebarItem::Radio,
        SidebarItem::PinBoards,
        SidebarItem::Wiki,
        SidebarItem::Code,
        SidebarItem::Recipes,
        SidebarItem::Shop,
        SidebarItem::Settings,
        SidebarItem::Live,
        SidebarItem::Bookmarks,
        #[cfg(feature = "cashu")]
        SidebarItem::Wallet,
        SidebarItem::Calendar,
        SidebarItem::Badges,
        SidebarItem::Publications,
        SidebarItem::Blossom,
        SidebarItem::Bible,
        SidebarItem::Highlights,
        SidebarItem::AIChat,
    ]
}
/// Global state for sidebar items
pub static SIDEBAR_ITEMS: GlobalSignal<Vec<SidebarItem>> = Signal::global(default_sidebar_items);
pub static SIDEBAR_SLOT_COUNT: GlobalSignal<usize> = Signal::global(|| DEFAULT_MAIN_SIDEBAR_SLOTS);
/// NIP-78 load state for sidebar preferences
pub static SIDEBAR_STATE: GlobalSignal<Nip78LoadState> = Signal::global(Nip78LoadState::default);
/// Get items for a specific sidebar page, filtered by auth
pub fn get_sidebar_page_items(page: usize, is_authenticated: bool) -> Vec<SidebarItem> {
    let slot_count = *SIDEBAR_SLOT_COUNT.read();
    let visible: Vec<SidebarItem> = SIDEBAR_ITEMS
        .read()
        .iter()
        .filter(|item| !item.is_hidden() && (!item.requires_auth() || is_authenticated))
        .cloned()
        .collect();
    visible
        .into_iter()
        .skip(page * slot_count)
        .take(slot_count)
        .collect()
}
/// Get total number of sidebar pages based on visible items
pub fn get_total_pages(is_authenticated: bool) -> usize {
    let slot_count = *SIDEBAR_SLOT_COUNT.read();
    if slot_count == 0 {
        return 1;
    }
    let visible_count = SIDEBAR_ITEMS
        .read()
        .iter()
        .filter(|item| !item.is_hidden() && (!item.requires_auth() || is_authenticated))
        .count();
    visible_count.div_ceil(slot_count).max(1)
}
/// Load cached sidebar preferences from localStorage
fn load_cached_sidebar() -> Option<SidebarPreferencesData> {
    storage::get::<String>(SIDEBAR_LOCAL_STORAGE_KEY)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
}
/// Save sidebar preferences to localStorage
fn cache_sidebar(data: &SidebarPreferencesData) {
    if let Ok(json) = serde_json::to_string(data) {
        let _ = storage::set(SIDEBAR_LOCAL_STORAGE_KEY, &json);
    }
}
/// Initialize sidebar from localStorage cache (synchronous, for instant UI)
/// Call this during app init BEFORE async client initialization
pub fn init_sidebar_from_cache() {
    if let Some(cached) = load_cached_sidebar() {
        let cached = cached.migrate_to_v2();
        if !cached.items_order.is_empty() {
            log::info!(
                "Initialized {} sidebar items from localStorage",
                cached.items_order.len()
            );
            *SIDEBAR_ITEMS.write() = cached.items_order;
            *SIDEBAR_SLOT_COUNT.write() = cached.items_per_page;
        }
    }
}
/// Load sidebar preferences from Nostr relays (NIP-78)
/// Uses a 3-step loading strategy for reliability:
/// 1. Load from localStorage first for instant UI
/// 2. Query local database (nostr-sdk caches events)
/// 3. Fetch from relays to sync any updates
pub async fn load_sidebar_preferences() {
    {
        let state = SIDEBAR_STATE.read().clone();
        if state.is_loading() {
            return;
        }
        *SIDEBAR_STATE.write() = Nip78LoadState::Loading;
    }
    log::info!("Loading sidebar preferences...");
    let mut loaded_from_cache = false;
    if let Some(cached) = load_cached_sidebar() {
        let cached = cached.migrate_to_v2();
        log::info!(
            "Loaded {} sidebar items from localStorage",
            cached.items_order.len()
        );
        *SIDEBAR_ITEMS.write() = cached.items_order;
        *SIDEBAR_SLOT_COUNT.write() = cached.items_per_page;
        loaded_from_cache = true;
    }
    if !auth_store::is_authenticated() {
        log::info!(
            "Not authenticated, using {} sidebar",
            if loaded_from_cache {
                "cached"
            } else {
                "default"
            }
        );
        *SIDEBAR_STATE.write() = Nip78LoadState::LoadedDefaults;
        return;
    }
    let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
        Some(c) => c.clone(),
        None => {
            log::warn!("Client not initialized");
            *SIDEBAR_STATE.write() = if loaded_from_cache {
                Nip78LoadState::Loaded
            } else {
                Nip78LoadState::Failed("Client not initialized".into())
            };
            return;
        }
    };
    let pubkey_str = auth_store::AUTH_STATE.read().pubkey.clone();
    let pubkey = match pubkey_str.as_ref() {
        Some(pk_str) => {
            match nostr_sdk::PublicKey::from_bech32(pk_str)
                .or_else(|_| nostr_sdk::PublicKey::from_hex(pk_str))
            {
                Ok(pk) => pk,
                Err(e) => {
                    log::error!("Invalid pubkey: {}", e);
                    *SIDEBAR_STATE.write() = if loaded_from_cache {
                        Nip78LoadState::Loaded
                    } else {
                        Nip78LoadState::LoadedDefaults
                    };
                    return;
                }
            }
        }
        None => {
            log::warn!("No pubkey available");
            *SIDEBAR_STATE.write() = if loaded_from_cache {
                Nip78LoadState::Loaded
            } else {
                Nip78LoadState::LoadedDefaults
            };
            return;
        }
    };
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(SIDEBAR_D_TAG)
        .limit(1);
    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if let Some(event) = db_events.into_iter().next() {
            log::info!("Found sidebar preference in local database: {}", event.id);
            if let Ok(data) = serde_json::from_str::<SidebarPreferencesData>(&event.content) {
                let data = data.migrate_to_v2();
                if !data.items_order.is_empty() {
                    *SIDEBAR_ITEMS.write() = data.items_order.clone();
                    *SIDEBAR_SLOT_COUNT.write() = data.items_per_page;
                    cache_sidebar(&data);
                    loaded_from_cache = true;
                }
            }
        }
    }
    nostr_client::ensure_relays_ready(&client).await;
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found sidebar preference event from relays: {}", event.id);
                match serde_json::from_str::<SidebarPreferencesData>(&event.content) {
                    Ok(data) => {
                        let data = data.migrate_to_v2();
                        if !data.items_order.is_empty() {
                            log::info!(
                                "Loaded {} sidebar items from Nostr relays",
                                data.items_order.len()
                            );
                            *SIDEBAR_ITEMS.write() = data.items_order.clone();
                            *SIDEBAR_SLOT_COUNT.write() = data.items_per_page;
                            cache_sidebar(&data);
                        }
                        *SIDEBAR_STATE.write() = Nip78LoadState::Loaded;
                    }
                    Err(e) => {
                        log::warn!("Failed to parse sidebar data: {}", e);
                        *SIDEBAR_STATE.write() = if loaded_from_cache {
                            Nip78LoadState::Loaded
                        } else {
                            Nip78LoadState::LoadedDefaults
                        };
                    }
                }
            } else {
                log::info!("No sidebar preferences found on relays");
                *SIDEBAR_STATE.write() = if loaded_from_cache {
                    Nip78LoadState::Loaded
                } else {
                    Nip78LoadState::LoadedDefaults
                };
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch sidebar preferences: {}", e);
            *SIDEBAR_STATE.write() = if loaded_from_cache {
                Nip78LoadState::Loaded
            } else {
                Nip78LoadState::Failed(e.to_string())
            };
        }
    }
}
/// Save sidebar preferences to Nostr relays (NIP-78)
pub async fn save_sidebar_preferences(
    items: Vec<SidebarItem>,
    items_per_page: usize,
) -> Result<(), String> {
    let items_per_page = items_per_page.clamp(1, MAX_MAIN_SIDEBAR_SLOTS);
    let mut items = items;
    for variant in SidebarItem::iter() {
        if !variant.is_hidden() && !items.contains(&variant) {
            items.push(variant);
        }
    }
    log::info!(
        "Saving {} sidebar items with {} per page to Nostr (NIP-78)...",
        items.len(),
        items_per_page
    );
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    nostr_client::ensure_relays_ready(&client).await;
    let data = SidebarPreferencesData {
        items_order: items.clone(),
        items_per_page,
        version: 2,
    };
    let content = serde_json::to_string(&data)
        .map_err(|e| format!("Failed to serialize sidebar data: {}", e))?;
    let builder =
        EventBuilder::new(Kind::from(APP_DATA_KIND), content).tag(Tag::identifier(SIDEBAR_D_TAG));
    client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish sidebar preferences: {}", e))?;
    log::info!("Sidebar preferences saved to Nostr successfully");
    cache_sidebar(&data);
    *SIDEBAR_ITEMS.write() = items;
    *SIDEBAR_SLOT_COUNT.write() = items_per_page;
    Ok(())
}
