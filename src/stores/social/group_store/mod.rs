mod fetch;
mod publish;

pub use fetch::*;
pub use publish::*;

use crate::utils::format::truncate_pubkey;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;

pub const KIND_GROUP_METADATA: u16 = 39000;
pub const KIND_GROUP_ADMINS: u16 = 39001;
pub const KIND_GROUP_MEMBERS: u16 = 39002;
pub const KIND_GROUP_ROLES: u16 = 39003;
pub const KIND_PUT_USER: u16 = 9000;
pub const KIND_REMOVE_USER: u16 = 9001;
pub const KIND_EDIT_METADATA: u16 = 9002;
pub const KIND_ADD_PERMISSION: u16 = 9003;
pub const KIND_REMOVE_PERMISSION: u16 = 9004;
pub const KIND_DELETE_EVENT: u16 = 9005;
pub const KIND_EDIT_GROUP_STATUS: u16 = 9006;
pub const KIND_CREATE_GROUP: u16 = 9007;
pub const KIND_DELETE_GROUP: u16 = 9008;
pub const KIND_CREATE_INVITE: u16 = 9009;
pub const KIND_JOIN_REQUEST: u16 = 9021;
pub const KIND_LEAVE_REQUEST: u16 = 9022;
pub const KIND_CHAT_MESSAGE: u16 = 9;
pub const KIND_CHAT_MESSAGE_ALT: u16 = 10;
pub const KIND_GROUP_NOTE: u16 = 11;
pub const KIND_GROUP_NOTE_REPLY: u16 = 12;
pub const KIND_ZAP_RECEIPT: u16 = 9735;
pub const KIND_USER_GROUPS_LIST: u16 = 10009;

pub const RECOMMENDED_GROUP_RELAYS: &[&str] = &[
    "wss://groups.0xchat.com",
    "wss://relay.highlighter.com",
    "wss://relay.groups.nip29.com",
];

const GROUP_CACHE_SIZE: usize = 200;
const MESSAGE_CACHE_SIZE: usize = 1000;

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: String,
    pub relay_url: String,
    pub name: Option<String>,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub is_private: bool,
    pub is_restricted: bool,
    pub is_hidden: bool,
    pub is_closed: bool,
    pub created_at: u64,
    pub event: Option<NostrEvent>,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum SystemMessageType {
    UserJoined { pubkey: String },
    UserLeft { pubkey: String },
    StatusChanged { by: String, details: String },
    MessageDeleted { by: String },
    GroupDeleted { by: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupMessage {
    pub id: String,
    pub group_id: String,
    pub author: String,
    pub content: String,
    pub created_at: u64,
    pub reply_to: Option<String>,
    pub reactions: HashMap<String, Vec<String>>,
    pub event: NostrEvent,
    pub is_system: bool,
    pub system_type: Option<SystemMessageType>,
    pub edited: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JoinRequest {
    pub id: String,
    pub group_id: String,
    pub author: String,
    pub content: String,
    pub invite_code: Option<String>,
    pub created_at: u64,
    pub event: NostrEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupNote {
    pub id: String,
    pub group_id: String,
    pub author: String,
    pub content: String,
    pub created_at: u64,
    pub reply_to: Option<String>,
    pub root_id: Option<String>,
    pub reactions: HashMap<String, Vec<String>>,
    pub event: NostrEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupAdmin {
    pub pubkey: String,
    pub role: String,
    pub permissions: Vec<GroupAdminPermission>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupRole {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GroupAdminPermission {
    AddUser,
    RemoveUser,
    EditMetadata,
    AddPermission,
    RemovePermission,
    DeleteEvent,
    EditGroupStatus,
    DeleteGroup,
    CreateInvite,
}

impl GroupAdminPermission {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "add-user" | "put-user" => Some(Self::AddUser),
            "remove-user" => Some(Self::RemoveUser),
            "edit-metadata" => Some(Self::EditMetadata),
            "add-permission" => Some(Self::AddPermission),
            "remove-permission" => Some(Self::RemovePermission),
            "delete-event" => Some(Self::DeleteEvent),
            "edit-group-status" => Some(Self::EditGroupStatus),
            "delete-group" => Some(Self::DeleteGroup),
            "create-invite" => Some(Self::CreateInvite),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AddUser => "add-user",
            Self::RemoveUser => "remove-user",
            Self::EditMetadata => "edit-metadata",
            Self::AddPermission => "add-permission",
            Self::RemovePermission => "remove-permission",
            Self::DeleteEvent => "delete-event",
            Self::EditGroupStatus => "edit-group-status",
            Self::DeleteGroup => "delete-group",
            Self::CreateInvite => "create-invite",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GroupMembershipStatus {
    Admin { role: String },
    Member,
    NotInGroupButKnown,
    Pending,
    NotInGroup,
}

const GROUPS_CACHE: GlobalSignal<LruCache<String, Group>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(GROUP_CACHE_SIZE).unwrap()));

const MESSAGES_CACHE: GlobalSignal<LruCache<String, GroupMessage>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(MESSAGE_CACHE_SIZE).unwrap()));

const GROUP_MEMBERS_CACHE: GlobalSignal<HashMap<String, HashSet<String>>> =
    GlobalSignal::new(HashMap::new);

const GROUP_ADMINS_CACHE: GlobalSignal<HashMap<String, Vec<GroupAdmin>>> =
    GlobalSignal::new(HashMap::new);

const GROUP_MEMBERSHIP_CACHE: GlobalSignal<HashMap<String, GroupMembershipStatus>> =
    GlobalSignal::new(HashMap::new);

pub static GROUPS_LOADING: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static GROUP_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

const PREVIOUS_EVENTS_CACHE: GlobalSignal<HashMap<String, Vec<String>>> =
    GlobalSignal::new(HashMap::new);

const GROUP_ROLES_CACHE: GlobalSignal<HashMap<String, Vec<GroupRole>>> =
    GlobalSignal::new(HashMap::new);

const GROUP_JOIN_REQUESTS_CACHE: GlobalSignal<HashMap<String, Vec<JoinRequest>>> =
    GlobalSignal::new(HashMap::new);

const MUTED_GROUPS: GlobalSignal<HashSet<String>> = GlobalSignal::new(HashSet::new);

const GROUP_PINNED_CACHE: GlobalSignal<HashMap<String, Vec<String>>> =
    GlobalSignal::new(HashMap::new);

const GROUP_NOTES_CACHE: GlobalSignal<LruCache<String, GroupNote>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(200).unwrap()));

pub fn encode_relay_url(url: &str) -> String {
    URL_SAFE_NO_PAD.encode(url.as_bytes())
}

pub fn decode_relay_url(encoded: &str) -> std::result::Result<String, String> {
    URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|e| format!("Invalid relay URL encoding: {}", e))
}

pub fn group_id_from_parts(relay_url: &str, group_id: &str) -> String {
    format!("{}'{}", relay_url, group_id)
}

#[allow(dead_code)]
pub fn parse_group_identifier(id: &str) -> (String, String) {
    if let Some(pos) = id.find('\'') {
        let relay_url = &id[..pos];
        let group_id = &id[pos + 1..];
        let relay_url = if relay_url.starts_with("wss://") || relay_url.starts_with("ws://") {
            relay_url.to_string()
        } else {
            format!("wss://{}", relay_url)
        };
        (relay_url, group_id.to_string())
    } else {
        (id.to_string(), String::new())
    }
}

pub fn cache_group(group: &Group) {
    let key = group_id_from_parts(&group.relay_url, &group.id);
    GROUPS_CACHE.write().put(key, group.clone());
}

pub fn cache_groups(groups: &[Group]) {
    let mut cache = GROUPS_CACHE.write();
    for group in groups {
        let key = group_id_from_parts(&group.relay_url, &group.id);
        cache.put(key, group.clone());
    }
}

pub fn get_cached_group(relay_url: &str, group_id: &str) -> Option<Group> {
    let key = group_id_from_parts(relay_url, group_id);
    GROUPS_CACHE.read().peek(&key).cloned()
}

#[allow(dead_code)]
pub fn get_all_cached_groups() -> Vec<Group> {
    GROUPS_CACHE
        .read()
        .iter()
        .map(|(_, g)| g.clone())
        .collect()
}

#[allow(dead_code)]
pub fn cache_message(msg: &GroupMessage) {
    MESSAGES_CACHE.write().put(msg.id.clone(), msg.clone());
}

pub fn cache_messages(msgs: &[GroupMessage]) {
    let mut cache = MESSAGES_CACHE.write();
    for msg in msgs {
        cache.put(msg.id.clone(), msg.clone());
    }
}

pub fn cache_members(relay_url: &str, group_id: &str, members: HashSet<String>) {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_MEMBERS_CACHE.write().insert(key, members);
}

pub fn get_cached_members(relay_url: &str, group_id: &str) -> HashSet<String> {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_MEMBERS_CACHE
        .read()
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

pub fn cache_admins(relay_url: &str, group_id: &str, admins: Vec<GroupAdmin>) {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_ADMINS_CACHE.write().insert(key, admins);
}

pub fn get_cached_admins(relay_url: &str, group_id: &str) -> Vec<GroupAdmin> {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_ADMINS_CACHE
        .read()
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

pub fn cache_membership(relay_url: &str, group_id: &str, status: GroupMembershipStatus) {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_MEMBERSHIP_CACHE.write().insert(key, status);
}

#[allow(dead_code)]
pub fn get_cached_membership(relay_url: &str, group_id: &str) -> GroupMembershipStatus {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_MEMBERSHIP_CACHE
        .read()
        .get(&key)
        .cloned()
        .unwrap_or(GroupMembershipStatus::NotInGroup)
}

pub fn is_group_admin(relay_url: &str, group_id: &str, user_pubkey: &str) -> bool {
    let admins = get_cached_admins(relay_url, group_id);
    admins.iter().any(|a| a.pubkey == user_pubkey)
}

pub fn get_admin_permissions(
    relay_url: &str,
    group_id: &str,
    user_pubkey: &str,
) -> Vec<GroupAdminPermission> {
    let admins = get_cached_admins(relay_url, group_id);
    admins
        .iter()
        .find(|a| a.pubkey == user_pubkey)
        .map(|a| a.permissions.clone())
        .unwrap_or_default()
}

pub fn track_previous_event(relay_url: &str, group_id: &str, event_id: &str) {
    let key = group_id_from_parts(relay_url, group_id);
    let mut cache = PREVIOUS_EVENTS_CACHE.write();
    let entries = cache.entry(key).or_default();
    if !entries.contains(&event_id.to_string()) {
        entries.push(event_id.to_string());
        if entries.len() > 50 {
            entries.remove(0);
        }
    }
}

pub fn get_previous_refs(relay_url: &str, group_id: &str, count: usize) -> Vec<String> {
    let key = group_id_from_parts(relay_url, group_id);
    let cache = PREVIOUS_EVENTS_CACHE.read();
    cache
        .get(&key)
        .map(|entries| {
            entries
                .iter()
                .rev()
                .take(count)
                .map(|id| id[..8.min(id.len())].to_string())
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_group_metadata(event: &NostrEvent, relay_url: &str) -> Option<Group> {
    if event.kind.as_u16() != KIND_GROUP_METADATA {
        return None;
    }
    let id = event
        .tags
        .iter()
        .find(|t| {
            t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D))
        })
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;
    let name = extract_tag_value(&event.tags, "name");
    let about = extract_tag_value(&event.tags, "about");
    let picture = extract_tag_value(&event.tags, "picture");
    let is_private = has_tag(&event.tags, "private");
    let is_restricted = has_tag(&event.tags, "restricted");
    let is_hidden = has_tag(&event.tags, "hidden");
    let is_closed = has_tag(&event.tags, "closed");
    Some(Group {
        id,
        relay_url: relay_url.to_string(),
        name,
        about,
        picture,
        is_private,
        is_restricted,
        is_hidden,
        is_closed,
        created_at: event.created_at.as_secs(),
        event: Some(event.clone()),
    })
}

pub fn parse_group_message(event: &NostrEvent) -> Option<GroupMessage> {
    let kind = event.kind.as_u16();
    if kind != KIND_CHAT_MESSAGE && kind != KIND_CHAT_MESSAGE_ALT {
        return None;
    }
    let group_id = event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("h")
        })
        .and_then(|t| t.as_slice().get(1).cloned())?;
    let reply_to = event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("q")
        })
        .and_then(|t| t.as_slice().get(1).cloned());
    Some(GroupMessage {
        id: event.id.to_hex(),
        group_id,
        author: event.pubkey.to_hex(),
        content: event.content.clone(),
        created_at: event.created_at.as_secs(),
        reply_to,
        reactions: HashMap::new(),
        event: event.clone(),
        is_system: false,
        system_type: None,
        edited: false,
    })
}

pub fn parse_group_admins(event: &NostrEvent) -> Vec<GroupAdmin> {
    if event.kind.as_u16() != KIND_GROUP_ADMINS {
        return Vec::new();
    }
    event
        .tags
        .iter()
        .filter(|t| t.kind() == TagKind::p())
        .filter_map(|t| {
            let slice = t.as_slice();
            let pubkey = slice.get(1)?.to_string();
            let role = slice.get(3).map(|s| s.to_string()).unwrap_or_default();
            let permissions: Vec<GroupAdminPermission> = slice
                .iter()
                .skip(4)
                .filter_map(|s| GroupAdminPermission::from_str(s))
                .collect();
            Some(GroupAdmin {
                pubkey,
                role,
                permissions,
            })
        })
        .collect()
}

pub fn parse_group_roles(event: &NostrEvent) -> Vec<GroupRole> {
    if event.kind.as_u16() != KIND_GROUP_ROLES {
        return Vec::new();
    }
    event
        .tags
        .iter()
        .filter(|t| {
            let s = t.as_slice();
            s.first().map(|k| k.as_str()) == Some("role")
        })
        .filter_map(|t| {
            let slice = t.as_slice();
            let name = slice.get(1)?.to_string();
            let description = slice.get(2).map(|s| s.to_string());
            Some(GroupRole { name, description })
        })
        .collect()
}

pub fn cache_roles(relay_url: &str, group_id: &str, roles: Vec<GroupRole>) {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_ROLES_CACHE.write().insert(key, roles);
}

#[allow(dead_code)]
pub fn get_cached_roles(relay_url: &str, group_id: &str) -> Vec<GroupRole> {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_ROLES_CACHE.read().get(&key).cloned().unwrap_or_default()
}

pub fn cache_join_requests(relay_url: &str, group_id: &str, requests: Vec<JoinRequest>) {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_JOIN_REQUESTS_CACHE.write().insert(key, requests);
}

pub fn get_cached_join_requests(relay_url: &str, group_id: &str) -> Vec<JoinRequest> {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_JOIN_REQUESTS_CACHE.read().get(&key).cloned().unwrap_or_default()
}

pub fn ignore_join_request(relay_url: &str, group_id: &str, request_id: &str) {
    let key = group_id_from_parts(relay_url, group_id);
    if let Some(requests) = GROUP_JOIN_REQUESTS_CACHE.write().get_mut(&key) {
        requests.retain(|r| r.id != request_id);
    }
}

pub fn is_group_muted(relay_url: &str, group_id: &str) -> bool {
    let key = group_id_from_parts(relay_url, group_id);
    MUTED_GROUPS.read().contains(&key)
}

pub fn toggle_group_mute(relay_url: &str, group_id: &str) {
    let key = group_id_from_parts(relay_url, group_id);
    let mut muted = MUTED_GROUPS.write();
    if muted.contains(&key) {
        muted.remove(&key);
    } else {
        muted.insert(key);
    }
}

pub fn cache_pinned(relay_url: &str, group_id: &str, pinned: Vec<String>) {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_PINNED_CACHE.write().insert(key, pinned);
}

pub fn get_cached_pinned(relay_url: &str, group_id: &str) -> Vec<String> {
    let key = group_id_from_parts(relay_url, group_id);
    GROUP_PINNED_CACHE.read().get(&key).cloned().unwrap_or_default()
}

pub fn toggle_pin_message(relay_url: &str, group_id: &str, event_id: &str) {
    let mut pinned = get_cached_pinned(relay_url, group_id);
    if pinned.contains(&event_id.to_string()) {
        pinned.retain(|id| id != event_id);
    } else {
        pinned.push(event_id.to_string());
    }
    cache_pinned(relay_url, group_id, pinned);
}

pub fn is_pinned(relay_url: &str, group_id: &str, event_id: &str) -> bool {
    get_cached_pinned(relay_url, group_id).contains(&event_id.to_string())
}

pub fn parse_group_members(event: &NostrEvent) -> Vec<String> {
    if event.kind.as_u16() != KIND_GROUP_MEMBERS {
        return Vec::new();
    }
    event
        .tags
        .iter()
        .filter(|t| t.kind() == TagKind::p())
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect()
}

pub fn parse_join_request(event: &NostrEvent) -> Option<JoinRequest> {
    if event.kind.as_u16() != KIND_JOIN_REQUEST {
        return None;
    }
    let group_id = event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("h")
        })
        .and_then(|t| t.as_slice().get(1).cloned())?;
    let invite_code = event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("code")
        })
        .and_then(|t| t.as_slice().get(1).cloned());
    Some(JoinRequest {
        id: event.id.to_hex(),
        group_id,
        author: event.pubkey.to_hex(),
        content: event.content.clone(),
        invite_code,
        created_at: event.created_at.as_secs(),
        event: event.clone(),
    })
}

pub fn parse_group_note(event: &NostrEvent) -> Option<GroupNote> {
    let kind = event.kind.as_u16();
    if kind != KIND_GROUP_NOTE && kind != KIND_GROUP_NOTE_REPLY {
        return None;
    }
    let group_id = event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("h")
        })
        .and_then(|t| t.as_slice().get(1).cloned())?;
    let reply_to = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::e() && {
            t.as_slice().get(3).map(|s| s == "reply").unwrap_or(false)
        })
        .and_then(|t| t.as_slice().get(1).cloned());
    let root_id = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::e() && {
            t.as_slice().get(3).map(|s| s == "root").unwrap_or(false)
        })
        .and_then(|t| t.as_slice().get(1).cloned());
    Some(GroupNote {
        id: event.id.to_hex(),
        group_id,
        author: event.pubkey.to_hex(),
        content: event.content.clone(),
        created_at: event.created_at.as_secs(),
        reply_to,
        root_id,
        reactions: HashMap::new(),
        event: event.clone(),
    })
}

fn extract_tag_value(tags: &Tags, tag_name: &str) -> Option<String> {
    tags.iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some(tag_name)
        })
        .and_then(|t| t.as_slice().get(1).cloned())
}

fn has_tag(tags: &Tags, tag_name: &str) -> bool {
    tags.iter().any(|t| {
        let slice = t.as_slice();
        slice.first().map(|s| s.as_str()) == Some(tag_name)
    })
}

pub fn all_groups_filter() -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_GROUP_METADATA))
        .limit(100)
}

pub fn group_metadata_filter(group_id: &str) -> Filter {
    Filter::new()
        .kinds(vec![
            Kind::Custom(KIND_GROUP_METADATA),
            Kind::Custom(KIND_GROUP_ADMINS),
            Kind::Custom(KIND_GROUP_MEMBERS),
            Kind::Custom(KIND_GROUP_ROLES),
        ])
        .identifier(group_id)
}

pub fn group_messages_filter(group_id: &str, limit: usize, until: Option<u64>) -> Filter {
    let mut filter = Filter::new()
        .kinds(vec![
            Kind::Custom(KIND_CHAT_MESSAGE),
            Kind::Custom(KIND_CHAT_MESSAGE_ALT),
        ])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), group_id)
        .limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    filter
}

pub fn group_subscription_filter(group_id: &str) -> Filter {
    Filter::new()
        .kinds(vec![
            Kind::Custom(7),
            Kind::Custom(KIND_CHAT_MESSAGE),
            Kind::Custom(KIND_CHAT_MESSAGE_ALT),
            Kind::Custom(KIND_GROUP_NOTE),
            Kind::Custom(KIND_GROUP_NOTE_REPLY),
            Kind::Custom(KIND_PUT_USER),
            Kind::Custom(KIND_REMOVE_USER),
            Kind::Custom(KIND_EDIT_METADATA),
            Kind::Custom(KIND_DELETE_EVENT),
            Kind::Custom(KIND_JOIN_REQUEST),
            Kind::Custom(KIND_LEAVE_REQUEST),
            Kind::Custom(KIND_GROUP_METADATA),
            Kind::Custom(KIND_GROUP_ADMINS),
            Kind::Custom(KIND_GROUP_MEMBERS),
            Kind::Custom(KIND_ZAP_RECEIPT),
        ])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), group_id)
        .limit(0)
}

#[allow(dead_code)]
pub fn membership_check_filter(group_id: &str, user_pubkey: &str) -> Filter {
    Filter::new()
        .kinds(vec![
            Kind::Custom(KIND_PUT_USER),
            Kind::Custom(KIND_REMOVE_USER),
        ])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), group_id)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::P), user_pubkey)
        .limit(2)
}
