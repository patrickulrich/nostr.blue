//! Directory Store
//! Handles NKBIP-04 Directory System - Kinds 30042-30045
//!
//! Directory types:
//! - Kind 30042: Drive event (root container)
//! - Kind 30043: Traceback event (backward linking)
//! - Kind 30044: Symlink event
//! - Kind 30045: Directory event
#![allow(dead_code)]
use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;
type StdResult<T, E> = std::result::Result<T, E>;
use crate::utils::nip54::normalize_wiki_dtag;
use crate::utils::nkbip06::generate_mime_tags;
/// Kind 30042 - Drive Event
pub const KIND_DRIVE: u16 = 30042;
/// Kind 30043 - Traceback Event
pub const KIND_TRACEBACK: u16 = 30043;
/// Kind 30044 - Symlink Event
pub const KIND_SYMLINK: u16 = 30044;
/// Kind 30045 - Directory Event
pub const KIND_DIRECTORY: u16 = 30045;
/// Cache sizes
const DRIVE_CACHE_SIZE: usize = 50;
const DIRECTORY_CACHE_SIZE: usize = 200;
/// Drive entry (root container)
#[derive(Clone, Debug, PartialEq)]
pub struct DriveEvent {
    pub event: NostrEvent,
    pub naddr: String,
    pub a_tag: String,
    pub d_tag: String,
    pub description: Option<String>,
    /// Ordered list of root directory addresses
    pub root_directories: Vec<String>,
}
/// Directory entry (file or subdirectory reference)
#[derive(Clone, Debug, PartialEq)]
pub enum DirectoryEntry {
    /// Reference to another event (file content)
    File {
        event_id: String,
        relay_hint: Option<String>,
    },
    /// Reference to a subdirectory (kind 30045)
    Subdirectory {
        address: String,
        relay_hint: Option<String>,
    },
}
/// Directory event
#[derive(Clone, Debug, PartialEq)]
pub struct DirectoryEvent {
    pub event: NostrEvent,
    pub naddr: String,
    pub a_tag: String,
    pub d_tag: String,
    pub name: Option<String>,
    /// Ordered list of entries
    pub entries: Vec<DirectoryEntry>,
}
/// Traceback event (backward linking for navigation)
#[derive(Clone, Debug, PartialEq)]
pub struct TracebackEvent {
    pub event: NostrEvent,
    pub naddr: String,
    pub a_tag: String,
    pub d_tag: String,
    /// The directory this traceback links to
    pub linked_directory: String,
    /// The parent directory
    pub parent_directory: String,
}
/// Symlink event
#[derive(Clone, Debug, PartialEq)]
pub struct SymlinkEvent {
    pub event: NostrEvent,
    pub naddr: String,
    pub a_tag: String,
    pub d_tag: String,
    /// Target event or address
    pub target: String,
    pub target_is_address: bool,
    /// Context directory
    pub context_directory: Option<String>,
    /// Drive this symlink belongs to
    pub drive: Option<String>,
}
/// Directory tree node for UI
#[derive(Clone, Debug, PartialEq)]
pub struct DirectoryNode {
    pub address: String,
    pub name: String,
    pub is_directory: bool,
    pub children: Vec<DirectoryNode>,
    pub depth: usize,
    pub expanded: bool,
}
/// Complete drive structure
#[derive(Clone, Debug)]
pub struct DriveTree {
    pub drive: DriveEvent,
    pub directories: HashMap<String, DirectoryEvent>,
    pub tracebacks: HashMap<String, TracebackEvent>,
    pub symlinks: HashMap<String, SymlinkEvent>,
}
impl DriveTree {
    pub fn new(drive: DriveEvent) -> Self {
        Self {
            drive,
            directories: HashMap::new(),
            tracebacks: HashMap::new(),
            symlinks: HashMap::new(),
        }
    }
    /// Build a tree representation for UI
    pub fn build_tree(&self) -> Vec<DirectoryNode> {
        self.drive
            .root_directories
            .iter()
            .filter_map(|addr| self.build_node(addr, 0))
            .collect()
    }
    fn build_node(&self, address: &str, depth: usize) -> Option<DirectoryNode> {
        if let Some(dir) = self.directories.get(address) {
            let children: Vec<DirectoryNode> = dir
                .entries
                .iter()
                .filter_map(|entry| match entry {
                    DirectoryEntry::Subdirectory { address, .. } => {
                        self.build_node(address, depth + 1)
                    }
                    DirectoryEntry::File { event_id, .. } => Some(DirectoryNode {
                        address: event_id.clone(),
                        name: event_id[..8].to_string(),
                        is_directory: false,
                        children: vec![],
                        depth: depth + 1,
                        expanded: false,
                    }),
                })
                .collect();
            Some(DirectoryNode {
                address: address.to_string(),
                name: dir.name.clone().unwrap_or_else(|| dir.d_tag.clone()),
                is_directory: true,
                children,
                depth,
                expanded: depth == 0,
            })
        } else {
            None
        }
    }
}
/// Drive cache (keyed by a_tag)
pub static DRIVES_CACHE: GlobalSignal<LruCache<String, DriveEvent>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(DRIVE_CACHE_SIZE).unwrap()));
/// Directory cache (keyed by a_tag)
pub static DIRECTORIES_CACHE: GlobalSignal<LruCache<String, DirectoryEvent>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(DIRECTORY_CACHE_SIZE).unwrap()));
/// Active drive tree
pub static ACTIVE_DRIVE: GlobalSignal<Option<DriveTree>> = GlobalSignal::new(|| None);
/// Current path (for breadcrumb navigation)
pub static CURRENT_PATH: GlobalSignal<Vec<String>> = GlobalSignal::new(Vec::new);
/// Loading states
pub static LOADING_DRIVES: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_DIRECTORY: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Store initialization flag
pub static DIRECTORY_STORE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Get a drive from cache
pub fn get_cached_drive(a_tag: &str) -> Option<DriveEvent> {
    DRIVES_CACHE.read().peek(a_tag).cloned()
}
/// Get a drive by naddr
pub fn get_cached_drive_by_naddr(naddr: &str) -> Option<DriveEvent> {
    let cache = DRIVES_CACHE.read();
    cache
        .iter()
        .find(|(_, d)| d.naddr == naddr)
        .map(|(_, d)| d.clone())
}
/// Cache a drive
pub fn cache_drive(drive: DriveEvent) {
    DRIVES_CACHE.write().put(drive.a_tag.clone(), drive);
}
/// Cache multiple drives
pub fn cache_drives(drives: &[DriveEvent]) {
    let mut cache = DRIVES_CACHE.write();
    for drive in drives {
        cache.put(drive.a_tag.clone(), drive.clone());
    }
}
/// Get a directory from cache
pub fn get_cached_directory(a_tag: &str) -> Option<DirectoryEvent> {
    DIRECTORIES_CACHE.read().peek(a_tag).cloned()
}
/// Cache a directory
pub fn cache_directory(directory: DirectoryEvent) {
    DIRECTORIES_CACHE
        .write()
        .put(directory.a_tag.clone(), directory);
}
/// Cache multiple directories
pub fn cache_directories(directories: &[DirectoryEvent]) {
    let mut cache = DIRECTORIES_CACHE.write();
    for dir in directories {
        cache.put(dir.a_tag.clone(), dir.clone());
    }
}
/// Get all cached drives
pub fn get_all_cached_drives() -> Vec<DriveEvent> {
    let cache = DRIVES_CACHE.read();
    cache.iter().map(|(_, d)| d.clone()).collect()
}
/// Clear all caches
pub fn clear_cache() {
    DRIVES_CACHE.write().clear();
    DIRECTORIES_CACHE.write().clear();
    *ACTIVE_DRIVE.write() = None;
    *CURRENT_PATH.write() = Vec::new();
    *DIRECTORY_STORE_INITIALIZED.write() = false;
}
/// Parse a Kind 30042 event into a DriveEvent
pub fn parse_drive_event(event: &NostrEvent) -> Option<DriveEvent> {
    if event.kind.as_u16() != KIND_DRIVE {
        return None;
    }
    let d_tag = event.tags.identifier()?;
    let a_tag = format!("{}:{}:{}", KIND_DRIVE, event.pubkey.to_hex(), d_tag);
    let naddr = Coordinate::new(Kind::Custom(KIND_DRIVE), event.pubkey)
        .identifier(d_tag)
        .to_bech32()
        .ok()?;
    let mut description = None;
    let mut root_directories = Vec::new();
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("description") => {
                description = slice.get(1).map(|s| s.to_string());
            }
            Some("a") => {
                if let Some(addr) = slice.get(1) {
                    if addr.starts_with("30045:") {
                        root_directories.push(addr.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    Some(DriveEvent {
        event: event.clone(),
        naddr,
        a_tag,
        d_tag: d_tag.to_string(),
        description,
        root_directories,
    })
}
/// Parse a Kind 30045 event into a DirectoryEvent
pub fn parse_directory_event(event: &NostrEvent) -> Option<DirectoryEvent> {
    if event.kind.as_u16() != KIND_DIRECTORY {
        return None;
    }
    let d_tag = event.tags.identifier()?;
    let a_tag = format!("{}:{}:{}", KIND_DIRECTORY, event.pubkey.to_hex(), d_tag);
    let naddr = Coordinate::new(Kind::Custom(KIND_DIRECTORY), event.pubkey)
        .identifier(d_tag)
        .to_bech32()
        .ok()?;
    let mut name = None;
    let mut entries = Vec::new();
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("name") | Some("title") => {
                name = slice.get(1).map(|s| s.to_string());
            }
            Some("a") => {
                if let Some(addr) = slice.get(1) {
                    let relay_hint = slice.get(2).map(|s| s.to_string());
                    entries.push(DirectoryEntry::Subdirectory {
                        address: addr.to_string(),
                        relay_hint,
                    });
                }
            }
            Some("e") => {
                if let Some(event_id) = slice.get(1) {
                    let relay_hint = slice.get(2).map(|s| s.to_string());
                    entries.push(DirectoryEntry::File {
                        event_id: event_id.to_string(),
                        relay_hint,
                    });
                }
            }
            _ => {}
        }
    }
    Some(DirectoryEvent {
        event: event.clone(),
        naddr,
        a_tag,
        d_tag: d_tag.to_string(),
        name,
        entries,
    })
}
/// Parse a Kind 30043 event into a TracebackEvent
pub fn parse_traceback_event(event: &NostrEvent) -> Option<TracebackEvent> {
    if event.kind.as_u16() != KIND_TRACEBACK {
        return None;
    }
    let d_tag = event.tags.identifier()?;
    let a_tag = format!("{}:{}:{}", KIND_TRACEBACK, event.pubkey.to_hex(), d_tag);
    let naddr = Coordinate::new(Kind::Custom(KIND_TRACEBACK), event.pubkey)
        .identifier(d_tag)
        .to_bech32()
        .ok()?;
    let mut linked_directory = None;
    let mut parent_directory = None;
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("a") => {
                linked_directory = slice.get(1).map(|s| s.to_string());
            }
            Some("A") => {
                parent_directory = slice.get(1).map(|s| s.to_string());
            }
            _ => {}
        }
    }
    Some(TracebackEvent {
        event: event.clone(),
        naddr,
        a_tag,
        d_tag: d_tag.to_string(),
        linked_directory: linked_directory?,
        parent_directory: parent_directory?,
    })
}
/// Parse a Kind 30044 event into a SymlinkEvent
pub fn parse_symlink_event(event: &NostrEvent) -> Option<SymlinkEvent> {
    if event.kind.as_u16() != KIND_SYMLINK {
        return None;
    }
    let d_tag = event.tags.identifier()?;
    let a_tag = format!("{}:{}:{}", KIND_SYMLINK, event.pubkey.to_hex(), d_tag);
    let naddr = Coordinate::new(Kind::Custom(KIND_SYMLINK), event.pubkey)
        .identifier(d_tag)
        .to_bech32()
        .ok()?;
    let mut target = None;
    let mut target_is_address = false;
    let mut context_directory = None;
    let mut drive = None;
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("a") => {
                if target.is_none() {
                    target = slice.get(1).map(|s| s.to_string());
                    target_is_address = true;
                }
            }
            Some("e") => {
                if target.is_none() {
                    target = slice.get(1).map(|s| s.to_string());
                    target_is_address = false;
                }
            }
            Some("A") => {
                if context_directory.is_none() {
                    context_directory = slice.get(1).map(|s| s.to_string());
                } else if drive.is_none() {
                    drive = slice.get(1).map(|s| s.to_string());
                }
            }
            _ => {}
        }
    }
    Some(SymlinkEvent {
        event: event.clone(),
        naddr,
        a_tag,
        d_tag: d_tag.to_string(),
        target: target?,
        target_is_address,
        context_directory,
        drive,
    })
}
/// Build filter for drives
pub fn drives_filter(limit: usize) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_DRIVE)).limit(limit)
}
/// Build filter for drives by author
pub fn drives_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_DRIVE))
        .author(pubkey)
        .limit(limit)
}
/// Build filter for a specific drive
pub fn drive_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_DRIVE))
        .author(pubkey)
        .identifier(identifier)
}
/// Build filter for directories
pub fn directories_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_DIRECTORY))
        .limit(limit)
}
/// Build filter for directories by author
pub fn directories_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_DIRECTORY))
        .author(pubkey)
        .limit(limit)
}
/// Build filter for a specific directory
pub fn directory_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_DIRECTORY))
        .author(pubkey)
        .identifier(identifier)
}
/// Build filter for tracebacks pointing to a directory
pub fn tracebacks_filter(directory_address: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_TRACEBACK))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), directory_address)
        .limit(limit)
}
/// Build filter for symlinks
pub fn symlinks_filter(limit: usize) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_SYMLINK)).limit(limit)
}
/// Fetch drives with pagination
pub async fn fetch_drives(limit: usize, until: Option<u64>) -> StdResult<Vec<DriveEvent>, String> {
    *LOADING_DRIVES.write() = true;
    let mut filter = drives_filter(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
    *LOADING_DRIVES.write() = false;
    match result {
        Ok(events) => {
            let drives: Vec<DriveEvent> = events.iter().filter_map(parse_drive_event).collect();
            cache_drives(&drives);
            *DIRECTORY_STORE_INITIALIZED.write() = true;
            log::info!("Fetched {} drives", drives.len());
            Ok(drives)
        }
        Err(e) => {
            log::error!("Failed to fetch drives: {}", e);
            Err(e)
        }
    }
}
/// Fetch a drive by naddr
pub async fn fetch_drive_by_naddr(naddr: &str) -> StdResult<Option<DriveEvent>, String> {
    if let Some(drive) = get_cached_drive_by_naddr(naddr) {
        return Ok(Some(drive));
    }
    let coord = Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let identifier = coord.identifier;
    if identifier.is_empty() {
        return Err("No identifier in naddr".to_string());
    }
    let filter = drive_by_coord_filter(coord.public_key, &identifier);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    match result {
        Ok(events) => {
            if let Some(event) = events.first() {
                if let Some(drive) = parse_drive_event(event) {
                    cache_drive(drive.clone());
                    return Ok(Some(drive));
                }
            }
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
/// Fetch a directory by address
pub async fn fetch_directory_by_address(
    address: &str,
) -> StdResult<Option<DirectoryEvent>, String> {
    if let Some(dir) = get_cached_directory(address) {
        return Ok(Some(dir));
    }
    let parts: Vec<&str> = address.splitn(3, ':').collect();
    if parts.len() < 3 {
        return Err("Invalid directory address format".to_string());
    }
    let kind = parts[0]
        .parse::<u16>()
        .map_err(|_| "Invalid kind in address")?;
    if kind != KIND_DIRECTORY {
        return Err("Address is not a directory".to_string());
    }
    let pubkey = PublicKey::from_hex(parts[1]).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let d_tag = parts[2..].join(":");
    let filter = directory_by_coord_filter(pubkey, &d_tag);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    match result {
        Ok(events) => {
            if let Some(event) = events.first() {
                if let Some(dir) = parse_directory_event(event) {
                    cache_directory(dir.clone());
                    return Ok(Some(dir));
                }
            }
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
/// Fetch drives by author
pub async fn fetch_drives_by_author(
    pubkey_hex: &str,
    limit: usize,
) -> StdResult<Vec<DriveEvent>, String> {
    let pubkey = PublicKey::from_hex(pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = drives_by_author_filter(pubkey, limit);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    match result {
        Ok(events) => {
            let drives: Vec<DriveEvent> = events.iter().filter_map(parse_drive_event).collect();
            cache_drives(&drives);
            Ok(drives)
        }
        Err(e) => Err(e),
    }
}
/// Build and fetch complete drive tree
pub async fn fetch_drive_tree(naddr: &str) -> StdResult<DriveTree, String> {
    let drive = fetch_drive_by_naddr(naddr)
        .await?
        .ok_or("Drive not found")?;
    let mut tree = DriveTree::new(drive.clone());
    for dir_addr in &drive.root_directories {
        if let Ok(Some(dir)) = fetch_directory_by_address(dir_addr).await {
            tree.directories.insert(dir_addr.clone(), dir.clone());
            fetch_subdirectories_recursive(&mut tree, &dir).await;
        }
    }
    *ACTIVE_DRIVE.write() = Some(tree.clone());
    Ok(tree)
}
/// Recursively fetch subdirectories
async fn fetch_subdirectories_recursive(tree: &mut DriveTree, directory: &DirectoryEvent) {
    for entry in &directory.entries {
        if let DirectoryEntry::Subdirectory { address, .. } = entry {
            if !tree.directories.contains_key(address) {
                if let Ok(Some(subdir)) = fetch_directory_by_address(address).await {
                    tree.directories.insert(address.clone(), subdir.clone());
                    Box::pin(fetch_subdirectories_recursive(tree, &subdir)).await;
                }
            }
        }
    }
}
/// Publish a new drive
pub async fn publish_drive(
    name: &str,
    description: Option<&str>,
    root_directories: &[String],
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let d_tag = normalize_wiki_dtag(name);
    let mut tags: Vec<Tag> = vec![Tag::identifier(&d_tag)];
    if let Some(desc) = description {
        tags.push(Tag::custom(
            TagKind::Custom("description".into()),
            vec![desc.to_string()],
        ));
    }
    for dir_addr in root_directories {
        tags.push(Tag::custom(
            TagKind::Custom("a".into()),
            vec![dir_addr.clone()],
        ));
    }
    for mime_tag in generate_mime_tags(KIND_DRIVE) {
        tags.push(mime_tag);
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_DRIVE), "").tags(tags);
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish drive: {}", e))?;
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
    log::info!("Drive published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}
/// Publish a new directory
pub async fn publish_directory(
    name: &str,
    entries: &[DirectoryEntry],
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let d_tag = normalize_wiki_dtag(name);
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::Custom("name".into()), vec![name.to_string()]),
    ];
    for entry in entries {
        match entry {
            DirectoryEntry::File {
                event_id,
                relay_hint,
            } => {
                let mut values = vec![event_id.clone()];
                if let Some(relay) = relay_hint {
                    values.push(relay.clone());
                }
                tags.push(Tag::custom(TagKind::Custom("e".into()), values));
            }
            DirectoryEntry::Subdirectory {
                address,
                relay_hint,
            } => {
                let mut values = vec![address.clone()];
                if let Some(relay) = relay_hint {
                    values.push(relay.clone());
                }
                tags.push(Tag::custom(TagKind::Custom("a".into()), values));
            }
        }
    }
    for mime_tag in generate_mime_tags(KIND_DIRECTORY) {
        tags.push(mime_tag);
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_DIRECTORY), "").tags(tags);
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish directory: {}", e))?;
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
    log::info!("Directory published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}
/// Publish a symlink
pub async fn publish_symlink(
    name: &str,
    target: &str,
    target_is_address: bool,
    context_directory: Option<&str>,
    drive: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let d_tag = normalize_wiki_dtag(name);
    let mut tags: Vec<Tag> = vec![Tag::identifier(&d_tag)];
    if target_is_address {
        tags.push(Tag::custom(
            TagKind::Custom("a".into()),
            vec![target.to_string()],
        ));
    } else {
        tags.push(Tag::custom(
            TagKind::Custom("e".into()),
            vec![target.to_string()],
        ));
    }
    if let Some(ctx) = context_directory {
        tags.push(Tag::custom(
            TagKind::Custom("A".into()),
            vec![ctx.to_string()],
        ));
    }
    if let Some(drv) = drive {
        tags.push(Tag::custom(
            TagKind::Custom("A".into()),
            vec![drv.to_string()],
        ));
    }
    for mime_tag in generate_mime_tags(KIND_SYMLINK) {
        tags.push(mime_tag);
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_SYMLINK), "").tags(tags);
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish symlink: {}", e))?;
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
    log::info!("Symlink published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}
/// Navigate into a directory
pub fn navigate_into(directory_address: &str) {
    let mut path = CURRENT_PATH.write();
    path.push(directory_address.to_string());
}
/// Navigate up one level
pub fn navigate_up() {
    let mut path = CURRENT_PATH.write();
    path.pop();
}
/// Navigate to root
pub fn navigate_to_root() {
    *CURRENT_PATH.write() = Vec::new();
}
/// Get current directory address
pub fn get_current_directory() -> Option<String> {
    CURRENT_PATH.read().last().cloned()
}
/// Get breadcrumb path
pub fn get_breadcrumb() -> Vec<String> {
    CURRENT_PATH.read().clone()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_directory_entry_file() {
        let entry = DirectoryEntry::File {
            event_id: "abc123".to_string(),
            relay_hint: None,
        };
        match entry {
            DirectoryEntry::File { event_id, .. } => assert_eq!(event_id, "abc123"),
            _ => panic!("Expected File entry"),
        }
    }
    #[test]
    fn test_directory_entry_subdirectory() {
        let entry = DirectoryEntry::Subdirectory {
            address: "30045:pubkey:subdir".to_string(),
            relay_hint: Some("wss://relay.example.com".to_string()),
        };
        match entry {
            DirectoryEntry::Subdirectory {
                address,
                relay_hint,
            } => {
                assert_eq!(address, "30045:pubkey:subdir");
                assert_eq!(relay_hint, Some("wss://relay.example.com".to_string()));
            }
            _ => panic!("Expected Subdirectory entry"),
        }
    }
}
