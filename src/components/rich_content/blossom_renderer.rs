//! BUD-10 `blossom:` URI renderer.
//!
//! Images use a failover `<img>` chain across the candidate servers (the
//! guaranteed path — `<img>` loading is not CORS-restricted), upgraded by
//! best-effort `HEAD` verification when the chain is exhausted. Video/audio
//! probe candidates first, then hand a working URL to the normal players.
//! File blobs resolve with full sha256 verification (vendored
//! `resolve_uri`) before download. Paid servers (402) surface the global
//! payment modal and transparently retry with the proof.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dioxus::prelude::*;
use nostr_blossom::bud10::BlossomUri;
use nostr::types::Url;

use crate::stores::media::blossom_payment::{self, prompt_for_payment};
use crate::stores::media::blossom_uri::{
    self, cached_url, classify_extension, head_probe, initial_candidates, record_hit, record_miss,
    ProbeOutcome, BlossomMediaKind,
};
use crate::stores::media::blossom_store::get_servers;
use crate::stores::media::{self, LightboxImage};

/// Above this size, paid (byte-fetched) video/audio is not inlined as a
/// data URL — large base64 payloads grind the DOM.
const DATA_URL_MEDIA_CAP: u64 = 8 * 1024 * 1024;

#[component]
pub fn BlossomUriRenderer(uri: BlossomUri) -> Element {
    match classify_extension(&uri.extension) {
        BlossomMediaKind::Image => rsx! { BlossomImage { uri } },
        BlossomMediaKind::Video | BlossomMediaKind::Audio => {
            rsx! { BlossomMedia { uri } }
        }
        BlossomMediaKind::File => rsx! { BlossomFileCard { uri } },
    }
}

fn mime_for_extension(ext: &str) -> String {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "opus" => "audio/opus",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn file_name(uri: &BlossomUri) -> String {
    format!("{}.{}", uri.sha256, uri.extension)
}

// ---------------------------------------------------------------------------
// Images: onerror failover chain (guaranteed), HEAD-verified deep resolve
// when it is exhausted, including the author-server phase (BUD-10 order).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum ImagePhase {
    Trying,
    /// All candidates failed; deep resolution running.
    Resolving,
    Done,
}

#[component]
fn BlossomImage(uri: BlossomUri) -> Element {
    let sha_hex = uri.sha256.to_string();
    let mut candidates = use_signal(|| {
        let mut list = Vec::new();
        if let Some(cached) = cached_url(&sha_hex) {
            list.push(cached);
        }
        list.extend(initial_candidates(&uri));
        list
    });
    let mut current = use_signal(|| 0usize);
    let mut phase = use_signal(|| ImagePhase::Trying);
    let mut error_note = use_signal(|| None::<String>);

    let url_now = candidates.read().get(*current.read()).cloned();
    let phase_now = *phase.read();
    let uri_for_error = uri.clone();

    match (url_now, phase_now) {
        (Some(url), _) => {
            let url_for_lightbox = url.clone();
            let sha_for_hit = sha_hex.clone();
            rsx! {
                div {
                    class: "my-2 rounded-lg overflow-hidden border border-border",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        media::open_lightbox(
                            vec![LightboxImage { url: url_for_lightbox.clone(), alt: None }],
                            0,
                        );
                    },
                    img {
                        src: "{url}",
                        alt: "Blossom image",
                        class: "max-w-full h-auto",
                        loading: "lazy",
                        onload: move |_| record_hit(&sha_for_hit, &url),
                        onerror: move |_| {
                            let next = *current.read() + 1;
                            if next < candidates.read().len() {
                                current.set(next);
                            } else if *phase.read() == ImagePhase::Trying {
                                // Exhausted the cheap chain — go deep.
                                phase.set(ImagePhase::Resolving);
                                let uri = uri_for_error.clone();
                                let sha_hex = sha_hex.clone();
                                spawn(async move {
                                    match deep_resolve(&uri, &sha_hex).await {
                                        DeepOutcome::Resolved(url) => {
                                            record_hit(&sha_hex, &url);
                                            candidates.write().clear();
                                            candidates.write().push(url);
                                            current.set(0);
                                            phase.set(ImagePhase::Trying);
                                        }
                                        DeepOutcome::Failed(note) => {
                                            record_miss(&sha_hex);
                                            error_note.set(Some(note));
                                            phase.set(ImagePhase::Done);
                                        }
                                    }
                                });
                            }
                        },
                    }
                }
            }
        }
        (None, ImagePhase::Done) => {
            rsx! {
                FailedCard { uri: uri.clone(), note: error_note.read().clone() }
            }
        }
        (None, _) => {
            rsx! {
                div {
                    class: "my-2 rounded-lg border border-border bg-muted/50 p-6 text-center text-sm text-muted-foreground",
                    "Resolving blossom blob…"
                }
            }
        }
    }
}

enum DeepOutcome {
    Resolved(String),
    Failed(String),
}

/// Full BUD-10 resolution order once the cheap chain is exhausted: probe
/// remaining candidates (author-list phase first), fall back to trusting an
/// unprobeable URL, and surface payment challenges via the global modal.
async fn deep_resolve(uri: &BlossomUri, sha_hex: &str) -> DeepOutcome {
    // Author server lists (kind 10063) — only fetched now, on demand.
    let mut candidates = if uri.authors.is_empty() {
        initial_candidates(uri)
    } else {
        let mut servers: Vec<Url> = Vec::new();
        for author in &uri.authors {
            servers.extend(blossom_uri::fetch_author_servers(author).await);
        }
        blossom_uri::candidates_with_authors(uri, &servers)
    };
    // Skip URLs already tried by the img chain (the first N).
    let tried = initial_candidates(uri);
    candidates.retain(|c| !tried.contains(c));
    if let Some(cached) = cached_url(sha_hex) {
        candidates.insert(0, cached);
    }

    let mut last_error = String::new();
    for url in &candidates {
        match head_probe(url, uri.size).await {
            ProbeOutcome::Available { size_matches: true } => {
                return DeepOutcome::Resolved(url.clone());
            }
            // Wrong Content-Length = a mirror serving different bytes.
            ProbeOutcome::Available { size_matches: false } => {
                last_error = "mirror served a blob of a different size".to_string();
            }
            ProbeOutcome::NotFound => {}
            ProbeOutcome::PaymentRequired { requests, reason } => {
                let server = origin_of(url).unwrap_or_else(|| url.clone());
                let prompt = blossom_payment::PaymentPrompt {
                    server,
                    sha256: sha_hex.to_string(),
                    requests,
                    reason,
                };
                if prompt_for_payment(prompt) {
                    if let Some(proof) =
                        blossom_payment::wait_for_proof(&origin_of(url).unwrap_or_default(), sha_hex)
                            .await
                    {
                        if let Ok(data) = fetch_with_proof(url, sha_hex, &proof).await {
                            return DeepOutcome::Resolved(data_url(&data, &uri.extension));
                        }
                    }
                    return DeepOutcome::Failed(
                        "payment did not complete — try another mirror".to_string(),
                    );
                }
            }
            ProbeOutcome::Unknown(note) => {
                // CORS-opaque or 5xx: might still work in a plain <img>.
                log::debug!("probe inconclusive for {url}: {note}");
                return DeepOutcome::Resolved(url.clone());
            }
        }
    }
    let _ = &mut last_error;
    DeepOutcome::Failed(format!(
        "not found on any of {} candidate server(s) {last_error}",
        candidates.len()
    ))
}

// ---------------------------------------------------------------------------
// Video / audio: probe a working URL, then use the standard players.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum MediaState {
    Resolving,
    Ready { url: String },
    Paywall,
    Failed(String),
}

#[component]
fn BlossomMedia(uri: BlossomUri) -> Element {
    let sha_hex = uri.sha256.to_string();
    let mut state = use_signal(|| MediaState::Resolving);

    // Runs once after mount (no reactive reads inside).
    let uri_once = uri.clone();
    let sha_once = sha_hex;
    use_effect(move || {
        let uri = uri_once.clone();
        let sha_hex = sha_once.clone();
        spawn(async move {
            match resolve_streamable_url(&uri, &sha_hex).await {
                MediaOutcome::Ready(url) => state.set(MediaState::Ready { url }),
                MediaOutcome::Paywall => state.set(MediaState::Paywall),
                MediaOutcome::Failed(note) => state.set(MediaState::Failed(note)),
            }
        });
    });

    let state_now = state.read().clone();
    match state_now {
        MediaState::Resolving => rsx! {
            div {
                class: "my-2 rounded-lg border border-border bg-muted/50 p-6 text-center text-sm text-muted-foreground",
                "Resolving blossom media…"
            }
        },
        MediaState::Ready { url } => match classify_extension(&uri.extension) {
            BlossomMediaKind::Video => rsx! {
                super::InlineVideoPlayer { url, imeta_width: None, imeta_height: None }
            },
            _ => rsx! {
                audio { class: "w-full my-2", controls: true, src: "{url}" }
            },
        },
        MediaState::Paywall => rsx! {
            div {
                class: "my-2 rounded-lg border border-border bg-muted/50 p-4 text-sm text-muted-foreground",
                "Paid server — complete the payment to load this media."
            }
        },
        MediaState::Failed(note) => rsx! { FailedCard { uri, note: Some(note) } },
    }
}

enum MediaOutcome {
    Ready(String),
    Paywall,
    Failed(String),
}

/// Probe the candidate chain for a URL the media element can stream.
/// Unprobeable candidates (CORS-opaque HEAD) are trusted as a last resort —
/// the media element itself is not CORS-restricted for playback.
async fn resolve_streamable_url(uri: &BlossomUri, sha_hex: &str) -> MediaOutcome {
    if let Some(cached) = cached_url(sha_hex) {
        return MediaOutcome::Ready(cached);
    }
    let candidates = initial_candidates(uri);
    let mut first_unknown: Option<String> = None;
    for url in &candidates {
        match head_probe(url, uri.size).await {
            ProbeOutcome::Available { size_matches: true } => {
                record_hit(sha_hex, url);
                return MediaOutcome::Ready(url.clone());
            }
            ProbeOutcome::Available { size_matches: false }
            | ProbeOutcome::NotFound => {}
            ProbeOutcome::PaymentRequired { requests, reason } => {
                let server = origin_of(url).unwrap_or_else(|| url.clone());
                let prompt = blossom_payment::PaymentPrompt {
                    server: server.clone(),
                    sha256: sha_hex.to_string(),
                    requests,
                    reason,
                };
                if prompt_for_payment(prompt) {
                    if let Some(proof) = blossom_payment::wait_for_proof(&server, sha_hex).await {
                        if let Ok(data) = fetch_with_proof(url, sha_hex, &proof).await {
                            if data.len() as u64 <= DATA_URL_MEDIA_CAP {
                                return MediaOutcome::Ready(data_url(&data, &uri.extension));
                            }
                        }
                    }
                    return MediaOutcome::Paywall;
                }
            }
            ProbeOutcome::Unknown(_) => {
                first_unknown.get_or_insert_with(|| url.clone());
            }
        }
    }
    if let Some(url) = first_unknown {
        return MediaOutcome::Ready(url);
    }
    MediaOutcome::Failed(format!(
        "not found on any of {} candidate server(s)",
        candidates.len()
    ))
}

// ---------------------------------------------------------------------------
// Generic files: verified byte resolution + download.
// ---------------------------------------------------------------------------

#[component]
fn BlossomFileCard(uri: BlossomUri) -> Element {
    let mut status = use_signal(|| None::<String>);
    let uri_for_dl = uri.clone();
    let name = file_name(&uri);
    let size_label = uri
        .size
        .map(crate::stores::media::blossom_store::format_bytes)
        .unwrap_or_else(|| "unknown size".to_string());

    rsx! {
        div {
            class: "my-2 rounded-lg border border-border bg-card p-4 flex items-center justify-between gap-3",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "min-w-0",
                p { class: "text-sm font-medium truncate", "{name}" }
                p { class: "text-xs text-muted-foreground", "Blossom blob · {size_label}" }
                if let Some(s) = status.read().as_ref() {
                    p { class: "text-xs text-muted-foreground mt-1", "{s}" }
                }
            }
            button {
                class: "px-3 py-2 text-sm rounded-lg bg-accent hover:bg-muted transition shrink-0",
                onclick: move |_| {
                    let uri = uri_for_dl.clone();
                    status.set(Some("resolving…".to_string()));
                    spawn(async move {
                        match resolve_and_download(&uri).await {
                            Ok(()) => status.set(Some("downloaded".to_string())),
                            Err(e) => status.set(Some(e)),
                        }
                    });
                },
                "Download"
            }
        }
    }
}

/// Verified byte resolution (BUD-10 order, sha256-checked by the vendored
/// `resolve_uri`) followed by a platform save. Paid servers go through the
/// payment modal + proof retry first.
async fn resolve_and_download(uri: &BlossomUri) -> Result<(), String> {
    let sha_hex = uri.sha256.to_string();
    let candidates = initial_candidates(uri);
    // Detect paywalls up front so the user is not blocked by a silent miss.
    for url in &candidates {
        if let ProbeOutcome::PaymentRequired { requests, reason } = head_probe(url, uri.size).await
        {
            let server = origin_of(url).unwrap_or_else(|| url.clone());
            let prompt = blossom_payment::PaymentPrompt {
                server: server.clone(),
                sha256: sha_hex.clone(),
                requests,
                reason,
            };
            if prompt_for_payment(prompt) {
                if let Some(proof) = blossom_payment::wait_for_proof(&server, &sha_hex).await {
                    let data = fetch_with_proof(url, sha_hex.as_str(), &proof).await?;
                    return save_blob(&data, uri).await;
                }
                return Err("payment did not complete".to_string());
            }
        }
    }

    // Unpaid path: full verification via the vendored resolver.
    let author_servers = fetch_all_author_servers(uri).await;
    let fallback: Vec<Url> = get_servers()
        .into_iter()
        .filter_map(|s| Url::parse(&s).ok())
        .collect();
    let bytes = resolve_verified(uri, author_servers, fallback).await?;
    save_blob(&bytes, uri).await
}

async fn fetch_all_author_servers(uri: &BlossomUri) -> Vec<Url> {
    let mut servers = Vec::new();
    for author in &uri.authors {
        servers.extend(blossom_uri::fetch_author_servers(author).await);
    }
    servers
}

async fn resolve_verified(
    uri: &BlossomUri,
    author_servers: Vec<Url>,
    fallback: Vec<Url>,
) -> Result<Vec<u8>, String> {
    let client = nostr_blossom::client::BlossomClient::new(
        Url::parse("https://blossom.primal.net").map_err(|e| e.to_string())?,
    );
    client
        .resolve_uri(uri, author_servers, fallback)
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_with_proof(
    url: &str,
    sha_hex: &str,
    proof: &nostr_blossom::client::BlossomPaymentProof,
) -> Result<Vec<u8>, String> {
    use nostr_blossom::client::BlossomClient;
    let parsed = Url::parse(url).map_err(|e| e.to_string())?;
    let sha_bytes = hex::decode(sha_hex).map_err(|e| format!("bad hash: {e}"))?;
    let sha_arr: [u8; 32] = sha_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "hash must be 32 bytes".to_string())?;
    // Same bitcoin_hashes 1.x instance the vendored crate and nostr use.
    let sha = bitcoin_hashes_1::sha256::Hash::from_byte_array(sha_arr);
    // Anonymous GET (no signer) + BUD-07 proof; verifies the sha256 of the
    // returned bytes before handing them back.
    let client = BlossomClient::new(parsed);
    client
        .get_blob_with_payment(sha, None, None, None::<&nostr::key::Keys>, Some(proof))
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

#[component]
fn FailedCard(uri: BlossomUri, note: Option<String>) -> Element {
    let name = file_name(&uri);
    rsx! {
        div {
            class: "my-2 rounded-lg border border-border bg-muted/50 p-4 text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            p { class: "text-muted-foreground break-all", "Blossom blob unavailable: {name}" }
            if let Some(note) = note {
                p { class: "text-xs text-muted-foreground mt-1", "{note}" }
            }
        }
    }
}

fn origin_of(url: &str) -> Option<String> {
    Url::parse(url).ok().and_then(|u| {
        u.host_str().map(|host| match u.port() {
            Some(port) => format!("{}://{host}:{port}", u.scheme()),
            None => format!("{}://{host}", u.scheme()),
        })
    })
}

fn data_url(bytes: &[u8], ext: &str) -> String {
    format!(
        "data:{};base64,{}",
        mime_for_extension(ext),
        BASE64.encode(bytes)
    )
}

async fn save_blob(bytes: &[u8], uri: &BlossomUri) -> Result<(), String> {
    crate::platform::download::save_bytes(
        &file_name(uri),
        bytes,
        &mime_for_extension(&uri.extension),
    )
}
