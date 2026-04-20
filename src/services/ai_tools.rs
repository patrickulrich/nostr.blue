use crate::services::ai_chat::{ToolDefinition, ToolFunction};
use crate::stores::nostr_client::get_client;
use crate::stores::relay::fetch_events_from_relays;
use nostr_sdk::nips::nip19::Nip19;
use nostr_sdk::prelude::*;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

const DEFAULT_TIMEOUT_SECS: u64 = 10;
const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 50;

async fn get_read_relays(client: &Client) -> Vec<String> {
    let pool_relays = client.pool().relays().await;
    let urls: Vec<String> = pool_relays
        .into_iter()
        .filter(|(_, relay)| relay.flags().has_read())
        .map(|(url, _)| url.to_string())
        .collect();
    if urls.is_empty() {
        vec![
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
            "wss://relay.snort.social".to_string(),
            "wss://nostr.wine".to_string(),
        ]
    } else {
        urls
    }
}

async fn tool_fetch_events(client: &Client, filter: Filter) -> Result<Vec<nostr::Event>, String> {
    let relays = get_read_relays(client).await;
    let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
    fetch_events_from_relays(client, filter, relays, timeout).await
}

pub fn nostr_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_profile".to_string(),
                description: "Fetch a Nostr user's profile metadata (name, display_name, about, nip05, picture, lud16, website). Input can be hex pubkey, npub, or nprofile.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pubkey": {
                            "type": "string",
                            "description": "Public key (hex, npub, or nprofile)"
                        }
                    },
                    "required": ["pubkey"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_note".to_string(),
                description: "Fetch a single Nostr event by its ID. Returns the event content, author, kind, tags, and creation date. Input can be hex, note1, or nevent1.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Event ID (hex, note1, or nevent1)"
                        }
                    },
                    "required": ["id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_notes".to_string(),
                description: "Fetch recent text notes (kind 1) from a Nostr user, sorted newest first.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pubkey": {
                            "type": "string",
                            "description": "Author's public key (hex, npub, or nprofile)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Number of notes to fetch (default 10, max 50)",
                            "default": 10
                        }
                    },
                    "required": ["pubkey"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_contact_list".to_string(),
                description: "Fetch the list of pubkeys a Nostr user follows (kind 3 contact list). Returns npub values with optional petnames.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pubkey": {
                            "type": "string",
                            "description": "Public key (hex, npub, or nprofile)"
                        }
                    },
                    "required": ["pubkey"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_relay_list".to_string(),
                description: "Fetch a user's NIP-65 relay list (kind 10002). Returns read/write relay URLs.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pubkey": {
                            "type": "string",
                            "description": "Public key (hex, npub, or nprofile)"
                        }
                    },
                    "required": ["pubkey"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_interaction_counts".to_string(),
                description: "Get the number of replies, likes, reposts, and zaps for a Nostr event.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "event_id": {
                            "type": "string",
                            "description": "Event ID (hex, note1, or nevent1)"
                        }
                    },
                    "required": ["event_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_received_zaps".to_string(),
                description: "Fetch zap receipts (kind 9735) received by a Nostr user. Returns zaps sorted newest first with amounts where available.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pubkey": {
                            "type": "string",
                            "description": "Public key of the zap recipient (hex, npub, or nprofile)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Number of zaps to fetch (default 10, max 50)",
                            "default": 10
                        }
                    },
                    "required": ["pubkey"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "query_events".to_string(),
                description: "Query Nostr events using a generic filter. Supports filtering by kind, author, tags, and search. Use this for advanced queries not covered by specific tools.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "kinds": {
                            "type": "array",
                            "items": { "type": "integer" },
                            "description": "Event kinds to filter (e.g. [1] for text notes, [0] for profiles, [30023] for articles)"
                        },
                        "authors": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Author pubkeys (hex or npub)"
                        },
                        "tags": {
                            "type": "object",
                            "description": "Tag filters. Keys are single-letter tag names (e.g. \"p\", \"e\", \"t\"). Values are arrays of strings.",
                            "additionalProperties": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        },
                        "search": {
                            "type": "string",
                            "description": "NIP-50 full-text search query"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum events to return (default 25, max 100)",
                            "default": 25
                        }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "search_notes".to_string(),
                description: "Search for Nostr text notes using NIP-50 full-text search. Requires relays that support NIP-50.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query text"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default 10, max 50)",
                            "default": 10
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "search_profiles".to_string(),
                description: "Search for Nostr users by name or display name. Searches cached profiles first, then relays.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (name or display name)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default 10, max 50)",
                            "default": 10
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_long_form_notes".to_string(),
                description: "Fetch long-form articles (kind 30023) from a Nostr user. Returns titles, summaries, and publication dates.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pubkey": {
                            "type": "string",
                            "description": "Author's public key (hex, npub, or nprofile)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Number of articles to fetch (default 10, max 50)",
                            "default": 10
                        }
                    },
                    "required": ["pubkey"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_dm_conversation".to_string(),
                description: "Fetch NIP-17 gift-wrapped direct messages exchanged with a peer. Requires the user to be logged in. Returns decrypted message content.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "peer_pubkey": {
                            "type": "string",
                            "description": "Peer's public key (hex, npub, or nprofile)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum messages to return (default 10, max 50)",
                            "default": 10
                        }
                    },
                    "required": ["peer_pubkey"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "convert_nip19".to_string(),
                description: "Convert a Nostr entity between NIP-19 encoding formats. Supported targets: npub, note, hex, nprofile, nevent, naddr.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "NIP-19 entity or hex string to convert"
                        },
                        "target_type": {
                            "type": "string",
                            "enum": ["npub", "note", "hex", "nprofile", "nevent", "naddr"],
                            "description": "Target format"
                        }
                    },
                    "required": ["input", "target_type"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "analyze_nip19".to_string(),
                description: "Decode and identify any NIP-19 entity. Returns the entity type and decoded data (pubkey, event ID, relay hints, etc.).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "NIP-19 entity or hex string to analyze"
                        }
                    },
                    "required": ["input"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_blossom_servers".to_string(),
                description: "Fetch a user's Blossom server list (kind 10063) for blob/media storage.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pubkey": {
                            "type": "string",
                            "description": "Public key (hex, npub, or nprofile)"
                        }
                    },
                    "required": ["pubkey"]
                }),
            },
        },
    ]
}

pub async fn execute_nostr_tool(name: &str, arguments: &str) -> String {
    let result = match name {
        "get_profile" => tool_get_profile(arguments).await,
        "get_note" => tool_get_note(arguments).await,
        "get_notes" => tool_get_notes(arguments).await,
        "get_contact_list" => tool_get_contact_list(arguments).await,
        "get_relay_list" => tool_get_relay_list(arguments).await,
        "get_interaction_counts" => tool_get_interaction_counts(arguments).await,
        "get_received_zaps" => tool_get_received_zaps(arguments).await,
        "query_events" => tool_query_events(arguments).await,
        "search_notes" => tool_search_notes(arguments).await,
        "search_profiles" => tool_search_profiles(arguments).await,
        "get_long_form_notes" => tool_get_long_form_notes(arguments).await,
        "get_dm_conversation" => tool_get_dm_conversation(arguments).await,
        "convert_nip19" => tool_convert_nip19(arguments),
        "analyze_nip19" => tool_analyze_nip19(arguments),
        "get_blossom_servers" => tool_get_blossom_servers(arguments).await,
        _ => Err(format!("Unknown tool: {}", name)),
    };
    match result {
        Ok(val) => val.to_string(),
        Err(e) => json!({"error": e}).to_string(),
    }
}

fn parse_pubkey(input: &str) -> Result<PublicKey, String> {
    PublicKey::parse(input.trim()).map_err(|e| format!("Invalid pubkey '{}': {}", input.trim(), e))
}

fn parse_event_id(input: &str) -> Result<EventId, String> {
    let trimmed = input.trim();
    if let Ok(id) = EventId::parse(trimmed) {
        return Ok(id);
    }
    match Nip19::from_bech32(trimmed) {
        Ok(Nip19::Event(nevent)) => Ok(nevent.event_id),
        Ok(Nip19::EventId(id)) => Ok(id),
        _ => Err(format!(
            "Invalid event ID '{}'. Expected hex, note1..., or nevent1...",
            trimmed
        )),
    }
}

fn format_event_summary(event: &Event) -> serde_json::Value {
    let author_npub = event
        .pubkey
        .to_bech32()
        .unwrap_or_else(|_| event.pubkey.to_string());
    let event_id_note = event
        .id
        .to_bech32()
        .unwrap_or_else(|_| event.id.to_string());
    json!({
        "id": event_id_note,
        "author": author_npub,
        "kind": event.kind.as_u16(),
        "created_at": event.created_at.to_human_datetime(),
        "content": event.content,
        "tags": event.tags.iter().map(|t| t.as_slice()).collect::<Vec<_>>(),
    })
}

#[derive(Deserialize)]
struct PubkeyArg {
    pubkey: String,
}

#[derive(Deserialize)]
struct PubkeyLimitArg {
    pubkey: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

#[derive(Deserialize)]
struct EventIdArg {
    event_id: String,
}

#[derive(Deserialize)]
struct IdArg {
    id: String,
}

#[derive(Deserialize)]
struct SearchArg {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Deserialize)]
struct PeerPubkeyArg {
    peer_pubkey: String,
    #[serde(default = "default_limit")]
    #[allow(dead_code)]
    limit: usize,
}

#[derive(Deserialize)]
struct ConvertNip19Arg {
    input: String,
    target_type: String,
}

#[derive(Deserialize)]
struct AnalyzeNip19Arg {
    input: String,
}

#[derive(Deserialize)]
struct QueryEventsArg {
    kinds: Option<Vec<u16>>,
    authors: Option<Vec<String>>,
    tags: Option<serde_json::Map<String, serde_json::Value>>,
    search: Option<String>,
    #[serde(default = "default_query_limit")]
    limit: usize,
}

fn default_query_limit() -> usize {
    25
}

async fn tool_get_profile(args_str: &str) -> Result<serde_json::Value, String> {
    let args: PubkeyArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let pk = parse_pubkey(&args.pubkey)?;
    let client = get_client().ok_or("Nostr client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .author(pk)
        .limit(1);
    let events = tool_fetch_events(&client, filter).await?;
    let Some(event) = events.first() else {
        return Ok(json!({"error": "Profile not found"}));
    };
    let metadata = Metadata::from_json(&event.content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;
    let npub = pk.to_bech32().unwrap_or_else(|_| pk.to_string());
    Ok(json!({
        "pubkey": npub,
        "name": metadata.name,
        "display_name": metadata.display_name,
        "about": metadata.about,
        "picture": metadata.picture.map(|u| u.to_string()),
        "nip05": metadata.nip05,
        "lud16": metadata.lud16,
        "lud06": metadata.lud06,
        "website": metadata.website.map(|u| u.to_string()),
    }))
}

async fn tool_get_note(args_str: &str) -> Result<serde_json::Value, String> {
    let args: IdArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let id = parse_event_id(&args.id)?;
    let client = get_client().ok_or("Nostr client not initialized")?;
    let filter = Filter::new().id(id).limit(1);
    let events = tool_fetch_events(&client, filter).await?;
    let Some(event) = events.first() else {
        return Ok(json!({"error": "Event not found"}));
    };
    Ok(format_event_summary(event))
}

async fn tool_get_notes(args_str: &str) -> Result<serde_json::Value, String> {
    let args: PubkeyLimitArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let pk = parse_pubkey(&args.pubkey)?;
    let limit = args.limit.clamp(1, MAX_LIMIT);
    let client = get_client().ok_or("Nostr client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::TextNote)
        .author(pk)
        .limit(limit);
    let events = tool_fetch_events(&client, filter).await?;
    let notes: Vec<serde_json::Value> = events
        .iter()
        .map(|e| {
            let author_npub = e
                .pubkey
                .to_bech32()
                .unwrap_or_else(|_| e.pubkey.to_string());
            let event_id_note = e
                .id
                .to_bech32()
                .unwrap_or_else(|_| e.id.to_string());
            json!({
                "id": event_id_note,
                "author": author_npub,
                "created_at": e.created_at.to_human_datetime(),
                "content": e.content,
            })
        })
        .collect();
    Ok(json!({"notes": notes, "count": notes.len()}))
}

async fn tool_get_contact_list(args_str: &str) -> Result<serde_json::Value, String> {
    let args: PubkeyArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let pk = parse_pubkey(&args.pubkey)?;
    let client = get_client().ok_or("Nostr client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::ContactList)
        .author(pk)
        .limit(1);
    let events = tool_fetch_events(&client, filter).await?;
    let Some(event) = events.first() else {
        return Ok(json!({"contacts": [], "count": 0}));
    };
    let contacts: Vec<serde_json::Value> = event
        .tags
        .public_keys()
        .map(|pk| {
            let npub = pk.to_bech32().unwrap_or_else(|_| pk.to_string());
            json!({"pubkey": npub})
        })
        .collect();
    Ok(json!({"contacts": contacts, "count": contacts.len()}))
}

async fn tool_get_relay_list(args_str: &str) -> Result<serde_json::Value, String> {
    let args: PubkeyArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let pk = parse_pubkey(&args.pubkey)?;
    let client = get_client().ok_or("Nostr client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::RelayList)
        .author(pk)
        .limit(1);
    let events = tool_fetch_events(&client, filter).await?;
    let Some(event) = events.first() else {
        return Ok(json!({"relays": [], "count": 0}));
    };
    let relays: Vec<serde_json::Value> = nostr_sdk::nips::nip65::extract_relay_list(event)
        .map(|(url, metadata)| {
            let marker = match metadata {
                Some(nostr_sdk::nips::nip65::RelayMetadata::Read) => "read",
                Some(nostr_sdk::nips::nip65::RelayMetadata::Write) => "write",
                None => "read+write",
            };
            json!({"url": url.to_string(), "mode": marker})
        })
        .collect();
    Ok(json!({"relays": relays, "count": relays.len()}))
}

async fn tool_get_interaction_counts(args_str: &str) -> Result<serde_json::Value, String> {
    let args: EventIdArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let event_id = parse_event_id(&args.event_id)?;
    let counts_map = crate::services::aggregation::fetch_interaction_counts_batch(
        vec![event_id],
        Duration::from_secs(DEFAULT_TIMEOUT_SECS),
    )
    .await
    .map_err(|e| e.to_string())?;
    let hex_id = event_id.to_hex();
    let Some(counts) = counts_map.get(&hex_id) else {
        return Ok(json!({"replies": 0, "likes": 0, "reposts": 0, "zaps": 0, "zap_amount_sats": 0}));
    };
    Ok(json!({
        "replies": counts.replies,
        "likes": counts.likes,
        "reposts": counts.reposts,
        "zaps": counts.zaps,
        "zap_amount_sats": counts.zap_amount_sats,
    }))
}

async fn tool_get_received_zaps(args_str: &str) -> Result<serde_json::Value, String> {
    let args: PubkeyLimitArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let pk = parse_pubkey(&args.pubkey)?;
    let limit = args.limit.clamp(1, MAX_LIMIT);
    let client = get_client().ok_or("Nostr client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::ZapReceipt)
        .pubkey(pk)
        .limit(limit);
    let events = tool_fetch_events(&client, filter).await?;
    let zaps: Vec<serde_json::Value> = events
        .iter()
        .map(|e| {
            let zap_from = e
                .tags
                .iter()
                .find(|t| t.kind() == TagKind::p())
                .and_then(|t| t.content())
                .unwrap_or("unknown");
            let comment = e
                .tags
                .iter()
                .find(|t| t.kind() == TagKind::custom("z"))
                .and_then(|t| t.content())
                .unwrap_or("");
            let event_id_note = e
                .id
                .to_bech32()
                .unwrap_or_else(|_| e.id.to_string());
            json!({
                "id": event_id_note,
                "from": zap_from,
                "created_at": e.created_at.to_human_datetime(),
                "comment": comment,
            })
        })
        .collect();
    Ok(json!({"zaps": zaps, "count": zaps.len()}))
}

async fn tool_query_events(args_str: &str) -> Result<serde_json::Value, String> {
    let args: QueryEventsArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let limit = args.limit.clamp(1, 100);
    let client = get_client().ok_or("Nostr client not initialized")?;
    let mut filter = Filter::new().limit(limit);
    if let Some(kinds) = args.kinds {
        filter = filter.kinds(kinds.into_iter().map(Kind::from));
    }
    if let Some(authors) = args.authors {
        let parsed: Vec<PublicKey> = authors
            .iter()
            .filter_map(|a| PublicKey::parse(a).ok())
            .collect();
        if !parsed.is_empty() {
            filter = filter.authors(parsed);
        }
    }
    if let Some(search) = &args.search {
        filter = filter.search(search);
    }
    if let Some(tags_map) = args.tags {
        for (tag_key, tag_values) in tags_map {
            if tag_key.len() != 1 {
                continue;
            }
            let letter = tag_key.chars().next().unwrap();
            let alphabet = match letter {
                'a' => Alphabet::A,
                'b' => Alphabet::B,
                'c' => Alphabet::C,
                'd' => Alphabet::D,
                'e' => Alphabet::E,
                'f' => Alphabet::F,
                'g' => Alphabet::G,
                'h' => Alphabet::H,
                'i' => Alphabet::I,
                'j' => Alphabet::J,
                'k' => Alphabet::K,
                'l' => Alphabet::L,
                'm' => Alphabet::M,
                'n' => Alphabet::N,
                'o' => Alphabet::O,
                'p' => Alphabet::P,
                'q' => Alphabet::Q,
                'r' => Alphabet::R,
                's' => Alphabet::S,
                't' => Alphabet::T,
                'u' => Alphabet::U,
                'v' => Alphabet::V,
                'w' => Alphabet::W,
                'x' => Alphabet::X,
                'y' => Alphabet::Y,
                'z' => Alphabet::Z,
                _ => continue,
            };
            let slt = nostr_sdk::SingleLetterTag::lowercase(alphabet);
            if let Some(vals) = tag_values.as_array() {
                let strings: Vec<String> = vals
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                if !strings.is_empty() {
                    filter = filter.custom_tags(slt, strings);
                }
            }
        }
    }
    let events = tool_fetch_events(&client, filter).await?;
    let results: Vec<serde_json::Value> =
        events.iter().map(format_event_summary).collect();
    Ok(json!({"events": results, "count": results.len()}))
}

async fn tool_search_notes(args_str: &str) -> Result<serde_json::Value, String> {
    let args: SearchArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let limit = args.limit.clamp(1, MAX_LIMIT);
    let results = crate::services::search::content_search::search_text_notes(
        &args.query,
        limit,
        &[],
    )
    .await?;
    let notes: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let e = &r.event;
            let author_npub = e
                .pubkey
                .to_bech32()
                .unwrap_or_else(|_| e.pubkey.to_string());
            let event_id_note = e
                .id
                .to_bech32()
                .unwrap_or_else(|_| e.id.to_string());
            json!({
                "id": event_id_note,
                "author": author_npub,
                "created_at": e.created_at.to_human_datetime(),
                "content": e.content,
            })
        })
        .collect();
    Ok(json!({"notes": notes, "count": notes.len()}))
}

async fn tool_search_profiles(args_str: &str) -> Result<serde_json::Value, String> {
    let args: SearchArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let limit = args.limit.clamp(1, MAX_LIMIT);
    let results = crate::services::search::profile_search::search_profiles(
        &args.query,
        limit,
        true,
    )
    .await?;
    let profiles: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let npub = r
                .pubkey
                .to_bech32()
                .unwrap_or_else(|_| r.pubkey.to_string());
            json!({
                "pubkey": npub,
                "name": r.name,
                "display_name": r.display_name,
                "picture": r.picture,
                "nip05": r.nip05,
            })
        })
        .collect();
    Ok(json!({"profiles": profiles, "count": profiles.len()}))
}

async fn tool_get_long_form_notes(args_str: &str) -> Result<serde_json::Value, String> {
    let args: PubkeyLimitArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let pk = parse_pubkey(&args.pubkey)?;
    let limit = args.limit.clamp(1, MAX_LIMIT);
    let client = get_client().ok_or("Nostr client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::LongFormTextNote)
        .author(pk)
        .limit(limit);
    let events = tool_fetch_events(&client, filter).await?;
    let articles: Vec<serde_json::Value> = events
        .iter()
        .map(|e| {
            let title = e.tags.identifier().map(String::from);
            let summary_tag = e
                .tags
                .iter()
                .find(|t| t.kind() == TagKind::custom("s"))
                .and_then(|t| t.content());
            let image_tag = e
                .tags
                .iter()
                .find(|t| t.kind() == TagKind::custom("i"))
                .and_then(|t| t.content());
            let event_id_note = e
                .id
                .to_bech32()
                .unwrap_or_else(|_| e.id.to_string());
            json!({
                "id": event_id_note,
                "title": title,
                "summary": summary_tag,
                "image": image_tag,
                "created_at": e.created_at.to_human_datetime(),
                "content_length": e.content.len(),
            })
        })
        .collect();
    Ok(json!({"articles": articles, "count": articles.len()}))
}

async fn tool_get_dm_conversation(args_str: &str) -> Result<serde_json::Value, String> {
    let args: PeerPubkeyArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let _peer_pk = parse_pubkey(&args.peer_pubkey)?;
    Ok(json!({
        "error": "DM fetching is not yet supported in AI tools. This feature requires additional implementation for NIP-17 gift wrap unwrapping."
    }))
}

fn tool_convert_nip19(args_str: &str) -> Result<serde_json::Value, String> {
    let args: ConvertNip19Arg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let input = args.input.trim();
    let target = args.target_type.trim().to_lowercase();
    match Nip19::from_bech32(input) {
        Ok(nip19) => match nip19 {
            Nip19::Pubkey(pk) => convert_pubkey(&pk, &target),
            Nip19::Profile(profile) => convert_pubkey(&profile.public_key, &target),
            Nip19::EventId(id) => convert_event_id(&id, &target),
            Nip19::Event(nevent) => convert_event_id(&nevent.event_id, &target),
            Nip19::Coordinate(_) => {
                Err("Coordinate conversion not yet supported".to_string())
            }
            Nip19::Secret(_) => Err("Secret key conversion not supported".to_string()),
            Nip19::EncryptedSecret(_) => Err("Encrypted secret key conversion not supported".to_string()),
        },
        Err(_) => {
            if input.len() == 64 {
                if let Ok(pk) = PublicKey::from_hex(input) {
                    return convert_pubkey(&pk, &target);
                }
                if let Ok(id) = EventId::from_hex(input) {
                    return convert_event_id(&id, &target);
                }
            }
            Err(format!("Cannot decode '{}' as NIP-19 entity", input))
        }
    }
}

fn convert_pubkey(pk: &PublicKey, target: &str) -> Result<serde_json::Value, String> {
    match target {
        "npub" => Ok(json!({"result": pk.to_bech32().map_err(|e| e.to_string())?})),
        "hex" => Ok(json!({"result": pk.to_hex()})),
        "nprofile" => {
            let nprofile =
                nostr_sdk::nips::nip19::Nip19Profile::new(*pk, vec![]);
            Ok(json!({"result": nprofile.to_bech32().map_err(|e| e.to_string())?}))
        }
        _ => Err(format!(
            "Cannot convert pubkey to '{}'. Supported: npub, hex, nprofile",
            target
        )),
    }
}

fn convert_event_id(id: &EventId, target: &str) -> Result<serde_json::Value, String> {
    match target {
        "note" => Ok(json!({"result": id.to_bech32().map_err(|e| e.to_string())?})),
        "hex" => Ok(json!({"result": id.to_hex()})),
        "nevent" => {
            let nevent = nostr_sdk::nips::nip19::Nip19Event::new(*id);
            Ok(json!({"result": nevent.to_bech32().map_err(|e| e.to_string())?}))
        }
        _ => Err(format!(
            "Cannot convert event ID to '{}'. Supported: note, hex, nevent",
            target
        )),
    }
}

fn tool_analyze_nip19(args_str: &str) -> Result<serde_json::Value, String> {
    let args: AnalyzeNip19Arg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let input = args.input.trim();
    if let Ok(nip19) = Nip19::from_bech32(input) {
        return match nip19 {
            Nip19::Pubkey(pk) => Ok(json!({
                "type": "npub",
                "pubkey": pk.to_bech32().unwrap_or_else(|_| pk.to_hex()),
                "pubkey_hex": pk.to_hex(),
            })),
            Nip19::Profile(profile) => Ok(json!({
                "type": "nprofile",
                "pubkey": profile.public_key.to_bech32().unwrap_or_else(|_| profile.public_key.to_hex()),
                "pubkey_hex": profile.public_key.to_hex(),
                "relays": profile.relays.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
            })),
            Nip19::EventId(id) => Ok(json!({
                "type": "note",
                "event_id": id.to_bech32().unwrap_or_else(|_| id.to_hex()),
                "event_id_hex": id.to_hex(),
            })),
            Nip19::Event(nevent) => Ok(json!({
                "type": "nevent",
                "event_id": nevent.event_id.to_bech32().unwrap_or_else(|_| nevent.event_id.to_hex()),
                "event_id_hex": nevent.event_id.to_hex(),
                "author": nevent.author.map(|a| a.to_bech32().unwrap_or_else(|_| a.to_hex())),
                "kind": nevent.kind.map(|k| k.as_u16()),
                "relays": nevent.relays.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
            })),
            Nip19::Coordinate(coord) => Ok(json!({
                "type": "naddr",
                "kind": coord.coordinate.kind.as_u16(),
                "pubkey": coord.coordinate.public_key.to_bech32().unwrap_or_else(|_| coord.coordinate.public_key.to_hex()),
                "identifier": coord.coordinate.identifier,
                "relays": coord.relays.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
            })),
            Nip19::Secret(_) => Ok(json!({"type": "nsec", "description": "Secret key (contents hidden)"})),
            Nip19::EncryptedSecret(_) => Ok(json!({"type": "ncryptsec", "description": "Encrypted secret key (contents hidden)"})),
        };
    }
    if input.len() == 64 {
        if let Ok(pk) = PublicKey::from_hex(input) {
            return Ok(json!({
                "type": "hex_pubkey",
                "pubkey": pk.to_bech32().unwrap_or_else(|_| pk.to_hex()),
                "pubkey_hex": pk.to_hex(),
            }));
        }
        if let Ok(id) = EventId::from_hex(input) {
            return Ok(json!({
                "type": "hex_event_id",
                "event_id": id.to_bech32().unwrap_or_else(|_| id.to_hex()),
                "event_id_hex": id.to_hex(),
            }));
        }
    }
    Err(format!(
        "Cannot decode '{}' as any known Nostr entity",
        input
    ))
}

async fn tool_get_blossom_servers(args_str: &str) -> Result<serde_json::Value, String> {
    let args: PubkeyArg = serde_json::from_str(args_str)
        .map_err(|e| format!("Invalid arguments: {}", e))?;
    let pk = parse_pubkey(&args.pubkey)?;
    let client = get_client().ok_or("Nostr client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::Custom(10063))
        .author(pk)
        .limit(1);
    let events = tool_fetch_events(&client, filter).await?;
    let Some(event) = events.first() else {
        return Ok(json!({"servers": [], "count": 0}));
    };
    let servers: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.kind() == TagKind::custom("server"))
        .filter_map(|t| t.content().map(String::from))
        .collect();
    Ok(json!({"servers": servers, "count": servers.len()}))
}
