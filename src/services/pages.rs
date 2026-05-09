//! Pages Service
//!
//! Handles publishing and discovering NIP-5A static site manifests (nsites).
//! Uploads files to Blossom and publishes kind 35128 manifest events.
#![allow(dead_code)]
use crate::services::git_hosting::git_service::git_service;
use crate::services::git_hosting::repository::fetch_repository;
use crate::stores::media::blossom_store;
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nips::nip5a::{
    build_manifest_tags, content_type_for_path, is_static_file, parse_manifest,
    SiteManifest, KIND_NSITE_NAMED, KIND_NSITE_ROOT,
};
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

pub struct PublishResult {
    pub event_id: EventId,
    pub path_count: usize,
}

pub async fn publish_pages_manifest(
    repo_naddr: &str,
    d_tag: &str,
    title: Option<String>,
    description: Option<String>,
    source_url: Option<String>,
    blossom_server: Option<String>,
    progress_cb: Option<&dyn Fn(f32)>,
) -> Result<PublishResult, String> {
    if !*HAS_SIGNER.read() {
        return Err("Not authenticated. Please sign in to publish.".to_string());
    }

    let repo = fetch_repository(repo_naddr)
        .await
        .map_err(|e| format!("Failed to fetch repo: {}", e))?;

    let gs = git_service();
    let all_paths = gs
        .list_all_files(&repo, Some("HEAD"))
        .await
        .map_err(|e| format!("Failed to list files: {}", e))?;

    let static_paths: Vec<String> = all_paths
        .into_iter()
        .filter(|p| is_static_file(p))
        .collect();

    if static_paths.is_empty() {
        return Err("No static files found in repository.".to_string());
    }

    if !static_paths
        .iter()
        .any(|p| p.trim_start_matches("./").trim_start_matches('/') == "index.html")
    {
        return Err("Repository must have an index.html at root.".to_string());
    }

    let total = static_paths.len();
    let mut path_hashes: Vec<(String, String)> = Vec::with_capacity(total);

    for (i, file_path) in static_paths.iter().enumerate() {
        if let Some(cb) = &progress_cb {
            cb((i as f32 / total as f32) * 80.0);
        }

        let normalized_path = if file_path.starts_with('/') {
            file_path.clone()
        } else {
            format!("/{}", file_path.trim_start_matches("./"))
        };

        let bytes = match gs
            .read_file_bytes(&repo, file_path, Some("HEAD"))
            .await
        {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Skipping file {}: {}", file_path, e);
                continue;
            }
        };

        let sha256 = blossom_store::calculate_sha256(&bytes);
        let content_type = content_type_for_path(file_path);

        let server = blossom_server
            .clone()
            .or_else(|| Some(blossom_store::get_primary_server()));

        match blossom_store::upload_image(bytes, content_type, 100u8, server).await {
            Ok(_) => {}
            Err(e) => {
                log::warn!("Failed to upload {}: {}", file_path, e);
                continue;
            }
        }

        path_hashes.push((normalized_path, sha256));
    }

    if path_hashes.is_empty() {
        return Err("No files could be uploaded.".to_string());
    }

    if let Some(cb) = &progress_cb {
        cb(85.0);
    }

    let servers = blossom_server
        .map(|s| vec![s])
        .unwrap_or_else(|| vec![blossom_store::get_primary_server()]);

    let relays: Vec<String> = match get_client() {
        Some(c) => {
            let relay_map = c.relays().await;
            relay_map
                .iter()
                .take(25)
                .map(|(url, _)| url.to_string())
                .collect()
        }
        None => Vec::new(),
    };

    let tags = build_manifest_tags(
        Some(d_tag),
        &path_hashes,
        &servers,
        title.as_deref(),
        description.as_deref(),
        source_url.as_deref(),
        &relays,
    );

    let builder = EventBuilder::new(Kind::from(KIND_NSITE_NAMED), "").tags(tags);

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;

    let event_id = event.id;
    let path_count = path_hashes.len();

    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Nsite,
        None,
        HashMap::new(),
    )
    .await;

    if let Some(cb) = &progress_cb {
        cb(95.0);
    }

    let _ = blossom_store::publish_user_servers().await;

    if let Some(cb) = &progress_cb {
        cb(100.0);
    }

    Ok(PublishResult {
        event_id,
        path_count,
    })
}

pub async fn fetch_pages_manifest(
    pubkey: &str,
    d_tag: Option<&str>,
) -> Result<Option<SiteManifest>, String> {
    let pk = PublicKey::from_hex(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let mut filter = Filter::new()
        .kind(Kind::from(KIND_NSITE_NAMED))
        .author(pk);

    if let Some(d) = d_tag {
        filter = filter.identifier(d);
    }

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch manifest: {}", e))?;

    let manifest = events
        .into_iter()
        .max_by_key(|e| e.created_at)
        .map(|e| parse_manifest(&e));

    Ok(manifest)
}

pub async fn fetch_root_manifest(pubkey: &str) -> Result<Option<SiteManifest>, String> {
    let pk = PublicKey::from_hex(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::from(KIND_NSITE_ROOT))
        .author(pk);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch root manifest: {}", e))?;

    Ok(events
        .into_iter()
        .max_by_key(|e| e.created_at)
        .map(|e| parse_manifest(&e)))
}

pub async fn fetch_all_user_pages(pubkey: &str) -> Result<Vec<SiteManifest>, String> {
    let pk = PublicKey::from_hex(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::from(KIND_NSITE_NAMED))
        .author(pk)
        .limit(50);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch pages: {}", e))?;

    Ok(events
        .iter()
        .map(parse_manifest)
        .collect())
}

pub async fn fetch_recent_pages(limit: usize) -> Result<Vec<SiteManifest>, String> {
    let filter = Filter::new()
        .kind(Kind::from(KIND_NSITE_NAMED))
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch recent pages: {}", e))?;

    Ok(events
        .iter()
        .map(parse_manifest)
        .collect())
}

pub async fn delete_pages_manifest(
    pubkey: &str,
    d_tag: Option<&str>,
) -> Result<(), String> {
    let manifest = if let Some(d) = d_tag {
        fetch_pages_manifest(pubkey, Some(d)).await?
    } else {
        fetch_root_manifest(pubkey).await?
    };

    let event = manifest
        .and_then(|m| m.event_id)
        .ok_or("No manifest found to delete")?;

    let tags = vec![Tag::custom(
        TagKind::custom("e"),
        vec![event.to_hex()],
    )];

    let builder = EventBuilder::new(Kind::from(5), "Delete nsite manifest").tags(tags);

    let signed = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign deletion: {}", e))?;

    crate::stores::publish_queue::enqueue(
        signed,
        crate::stores::publish_queue::types::QueueEventType::Nsite,
        None,
        HashMap::new(),
    )
    .await;

    Ok(())
}
