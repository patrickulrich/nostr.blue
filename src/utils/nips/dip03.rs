//! DIP-03: Private Zaps
//!
//! <https://github.com/damus-io/dips/blob/master/03.md>
//!
//! A private zap hides the sender's identity from everyone except the
//! recipient. The sender's identity and optional private message live in an
//! encrypted kind-9733 event (signed by the sender's real key), which is
//! NIP-04-encrypted to the recipient and carried in the `anon` tag of the
//! public kind-9734 zap request (signed by a random ephemeral key).
//!
//! Wire format of the `anon` tag value:
//! `pzap1<bech32 ciphertext>_iv1<bech32 iv>`
//!
//! The cipher is byte-compatible with NIP-04 (ECDH shared secret +
//! AES-256-CBC/PKCS7); only the encoding differs (bech32 vs base64). This
//! lets remote signers (NIP-07/46/55) decrypt via their standard
//! `nip04_decrypt` implementations by re-encoding the payload as a NIP-04
//! content string.

use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use bech32::{self, Hrp};
use futures::future::{select, Either};
use lru::LruCache;
use nostr::secp256k1::rand::rngs::OsRng;
use nostr::signer::NostrSigner;
use nostr::SecretKey;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::nips::nip57::{self, ZapRequestData};
use nostr_sdk::{
    Event, EventBuilder, EventId, JsonUtil, Keys, Kind, PublicKey, RelayUrl, Tag, TagStandard,
};

const PZAP_HRP: Hrp = Hrp::parse_unchecked("pzap");
const IV_HRP: Hrp = Hrp::parse_unchecked("iv");

/// Remote signers (NIP-07/46/55) can be dismissed or hang forever; bound the
/// waits so flows always resolve.
const REMOTE_SIGN_TIMEOUT_MS: u32 = 45_000;
const REMOTE_DECRYPT_TIMEOUT_MS: u32 = 20_000;

/// Zap visibility modes offered by the zap UIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZapVisibility {
    /// Standard NIP-57 zap: public sender and public message.
    Public,
    /// DIP-03 anonymous zap: sender hidden from everyone (message stays public).
    Anonymous,
    /// DIP-03 private zap: sender and message visible only to the recipient.
    Private,
}

/// How a kind-9734 zap request's `anon` tag classifies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnonKind {
    /// No `anon` tag: standard public zap request.
    None,
    /// `["anon"]` without payload: fully anonymous zap.
    Anonymous,
    /// `["anon", "<payload>"]`: DIP-03 private zap.
    Private(String),
}

/// A successfully decrypted DIP-03 private zap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecryptedPrivateZap {
    /// Real sender pubkey (recovered from the inner kind-9733 signature).
    pub sender_pubkey: PublicKey,
    /// Optional private message (inner event `content`).
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivateZapError {
    /// No signer is connected.
    NoSigner,
    /// Signing the inner kind-9733 event failed.
    Sign(String),
    /// The remote signer failed or refused the operation (dismissed NIP-07
    /// prompt, unconnected NIP-46 signer, transient signer error). The SDK
    /// surfaces all of these as an opaque backend error, so they can't be
    /// told apart from here — they stay retryable (rate-limited by a
    /// cooldown, not cached).
    Signer(String),
    /// Encryption/decryption failure (wrong key, corrupt payload, ...).
    Crypto(String),
    /// Payload structure invalid (bad bech32, wrong HRP, ...).
    Malformed(String),
    /// Remote signer did not answer in time (retryable).
    Timeout,
}

impl std::fmt::Display for PrivateZapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSigner => f.write_str("No signer available"),
            Self::Sign(e) => write!(f, "Signing failed: {e}"),
            Self::Signer(e) => write!(f, "Remote signer error: {e}"),
            Self::Crypto(e) => write!(f, "Private zap crypto error: {e}"),
            Self::Malformed(e) => write!(f, "Malformed private zap payload: {e}"),
            Self::Timeout => f.write_str("Signer timed out"),
        }
    }
}

/// Whether a decrypt failure is terminal — wrong key, corrupt payload — and
/// should be cached for the process lifetime. Transient failures (no
/// signer, remote-signer errors, timeouts) stay retryable.
fn is_terminal(err: &PrivateZapError) -> bool {
    matches!(
        err,
        PrivateZapError::Crypto(_) | PrivateZapError::Malformed(_)
    )
}

async fn with_timeout<T>(fut: impl Future<Output = T>, ms: u32) -> Result<T, PrivateZapError> {
    let timeout = crate::platform::timer::sleep_ms(ms);
    match select(Box::pin(fut), Box::pin(timeout)).await {
        Either::Left((result, _)) => Ok(result),
        Either::Right(_) => Err(PrivateZapError::Timeout),
    }
}

/// Build a DIP-03 **private** zap request (kind 9734).
///
/// The outer event is signed by a fresh random ephemeral key so the real
/// sender pubkey never appears in public data. The inner kind-9733 event is
/// signed by the *real* user key — this is how the recipient learns the
/// sender identity — and works with every signer backend (local keys,
/// NIP-07, NIP-46, NIP-55).
pub async fn build_private_zap_request(
    recipient_pubkey: PublicKey,
    relays: Vec<RelayUrl>,
    amount_msats: u64,
    message: Option<String>,
    event_id: Option<EventId>,
    event_coordinate: Option<Coordinate>,
) -> Result<Event, PrivateZapError> {
    let signer =
        crate::stores::signer::get_signer().ok_or(PrivateZapError::NoSigner)?;

    let mut inner_tags: Vec<Tag> = vec![Tag::public_key(recipient_pubkey)];
    if let Some(eid) = event_id {
        inner_tags.push(Tag::event(eid));
    }
    if let Some(coord) = event_coordinate.as_ref() {
        inner_tags.push(Tag::from(coord.clone()));
    }
    let inner_builder =
        EventBuilder::new(Kind::ZapPrivateMessage, message.as_deref().unwrap_or(""))
            .tags(inner_tags);
    let inner_event = sign_inner_event(&signer, inner_builder).await?;

    // Random ephemeral keypair (pure Rust keygen; works on WASM too).
    let ephemeral_keys = Keys::generate();

    let mut data = ZapRequestData::new(recipient_pubkey, relays).amount(amount_msats);
    if let Some(eid) = event_id {
        data = data.event_id(eid);
    }
    if let Some(coord) = event_coordinate {
        data = data.event_coordinate(coord);
    }

    compose_private_zap_request(inner_event, &ephemeral_keys, data)
}

/// Build a DIP-03 **anonymous** zap request (kind 9734): a random ephemeral
/// key signs and a bare `["anon"]` tag is attached. No user key involved.
/// The optional message is carried in the public `content` field.
pub fn build_anonymous_zap_request(
    recipient_pubkey: PublicKey,
    relays: Vec<RelayUrl>,
    amount_msats: u64,
    message: Option<String>,
    event_id: Option<EventId>,
    event_coordinate: Option<Coordinate>,
) -> Result<Event, PrivateZapError> {
    let mut data = ZapRequestData::new(recipient_pubkey, relays).amount(amount_msats);
    if let Some(msg) = message {
        data = data.message(msg);
    }
    if let Some(eid) = event_id {
        data = data.event_id(eid);
    }
    if let Some(coord) = event_coordinate {
        data = data.event_coordinate(coord);
    }
    nip57::anonymous_zap_request(data).map_err(|e| PrivateZapError::Sign(e.to_string()))
}

/// Compose the outer kind-9734 from an already-signed inner kind-9733 event.
///
/// Pure function (no signer access) — also the unit-test entry point.
fn compose_private_zap_request(
    inner_event: Event,
    ephemeral_keys: &Keys,
    data: ZapRequestData,
) -> Result<Event, PrivateZapError> {
    let payload = nip57::encrypt_private_zap_message(
        &mut OsRng,
        ephemeral_keys.secret_key(),
        &data.public_key,
        inner_event.as_json(),
    )
    .map_err(|e| PrivateZapError::Crypto(e.to_string()))?;

    let mut tags: Vec<Tag> = data.into();
    tags.push(Tag::from_standardized_without_cell(TagStandard::Anon {
        msg: Some(payload),
    }));

    EventBuilder::new(Kind::ZapRequest, "")
        .tags(tags)
        .sign_with_keys(ephemeral_keys)
        .map_err(|e| PrivateZapError::Sign(e.to_string()))
}

async fn sign_inner_event(
    signer: &crate::stores::signer::SignerType,
    builder: EventBuilder,
) -> Result<Event, PrivateZapError> {
    use crate::stores::signer::SignerType;
    match signer {
        SignerType::Keys(keys) => builder
            .sign_with_keys(keys)
            .map_err(|e| PrivateZapError::Sign(e.to_string())),
        other => {
            let nostr_signer = other.as_nostr_signer();
            let public_key = with_timeout(nostr_signer.get_public_key(), REMOTE_SIGN_TIMEOUT_MS)
                .await
                .map_err(|_| PrivateZapError::Timeout)?
                .map_err(|e| PrivateZapError::Sign(e.to_string()))?;
            let unsigned = builder.build(public_key);
            with_timeout(nostr_signer.sign_event(unsigned), REMOTE_SIGN_TIMEOUT_MS)
                .await
                .map_err(|_| PrivateZapError::Timeout)?
                .map_err(|e| PrivateZapError::Sign(e.to_string()))
        }
    }
}

/// Parse the kind-9734 zap request embedded in a kind-9735 receipt's
/// `description` tag.
pub fn parse_description_event(receipt: &Event) -> Option<Event> {
    let description: &str = receipt.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|value| value.as_str()) == Some("description") {
            slice.get(1).map(|value| value.as_str())
        } else {
            None
        }
    })?;
    Event::from_json(description).ok()
}

/// Classify a kind-9734 zap request by its `anon` tag.
pub fn classify_anon(zap_request: &Event) -> AnonKind {
    for tag in zap_request.tags.iter() {
        if let Some(TagStandard::Anon { msg }) = tag.as_standardized() {
            return match msg {
                Some(payload) => AnonKind::Private(payload.clone()),
                None => AnonKind::Anonymous,
            };
        }
    }
    AnonKind::None
}

/// Re-encode a DIP-03 `anon` payload as a NIP-04 content string
/// (`<base64 ciphertext>?iv=<base64 iv>`).
///
/// The underlying cipher is identical to NIP-04, so this lets remote signers
/// (NIP-07/46/55) decrypt via `NostrSigner::nip04_decrypt`.
pub fn anon_payload_to_nip04(anon: &str) -> Result<String, PrivateZapError> {
    let malformed = |msg: &str| PrivateZapError::Malformed(msg.to_string());
    let mut segments = anon.split('_');
    let msg_segment = segments
        .next()
        .ok_or_else(|| malformed("empty anon payload"))?;
    let iv_segment = segments
        .next()
        .ok_or_else(|| malformed("anon payload missing iv segment"))?;
    if segments.next().is_some() {
        return Err(malformed("unexpected extra segments in anon payload"));
    }

    let (msg_hrp, ciphertext) = bech32::decode(msg_segment).map_err(|e| malformed(&e.to_string()))?;
    if msg_hrp != PZAP_HRP {
        return Err(malformed(&format!(
            "expected 'pzap' HRP, found '{msg_hrp}'"
        )));
    }
    let (iv_hrp, iv) = bech32::decode(iv_segment).map_err(|e| malformed(&e.to_string()))?;
    if iv_hrp != IV_HRP {
        return Err(malformed(&format!("expected 'iv' HRP, found '{iv_hrp}'")));
    }

    Ok(format!(
        "{}?iv={}",
        BASE64.encode(ciphertext),
        BASE64.encode(iv)
    ))
}

/// How long a transient remote-signer failure (dismissed NIP-07 prompt,
/// signer hiccup, timeout) suppresses re-attempts for the same payload.
/// The outcome stays uncached so recovery is possible; this only rate-limits
/// the retry so repeated renders don't re-prompt endlessly.
const TRANSIENT_RETRY_COOLDOWN_MS: u64 = 60_000;

/// Decrypt a DIP-03 private zap.
///
/// `zap_request` is the kind-9734 event parsed from the receipt's
/// `description` tag. Works with every signer backend:
/// - local keys: direct SDK decryption (no prompt)
/// - remote signers: NIP-04 decrypt of the re-encoded payload (may prompt)
///
/// Terminal outcomes are cached per payload so repeated renders never
/// trigger repeated signer prompts; transient outcomes (dismissed prompt,
/// timeout, no signer) stay retryable, rate-limited by
/// [`TRANSIENT_RETRY_COOLDOWN_MS`].
pub async fn decrypt_private_zap(
    zap_request: &Event,
) -> Result<DecryptedPrivateZap, PrivateZapError> {
    let payload = match classify_anon(zap_request) {
        AnonKind::Private(payload) => payload,
        _ => {
            return Err(PrivateZapError::Malformed(
                "no private zap payload in anon tag".to_string(),
            ))
        }
    };

    if let Some(cached) = cached_outcome(&payload) {
        return cached;
    }

    if transient_attempt_pending(&payload) {
        // A transient failure was recorded recently: surface the error
        // without re-prompting the user. Recovery stays possible once the
        // cooldown elapses (the outcome is not terminally cached).
        return Err(PrivateZapError::Signer(
            "private zap decryption recently failed; retry available shortly".to_string(),
        ));
    }

    let signer =
        crate::stores::signer::get_signer().ok_or(PrivateZapError::NoSigner)?;

    let result = match &signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            decrypt_with_keys(keys.secret_key(), zap_request)
        }
        other => {
            let nip04_payload = anon_payload_to_nip04(&payload)?;
            let nostr_signer = other.as_nostr_signer();
            let ephemeral_pubkey = zap_request.pubkey;
            let decrypted = with_timeout(
                nostr_signer.nip04_decrypt(&ephemeral_pubkey, &nip04_payload),
                REMOTE_DECRYPT_TIMEOUT_MS,
            )
            .await
            .map_err(|_| PrivateZapError::Timeout)?
            // The SDK's SignerError is an opaque string: a genuine decrypt
            // failure and a dismissed/unavailable signer are
            // indistinguishable, so classify as transient (retryable).
            .map_err(|e| PrivateZapError::Signer(e.to_string()))?;
            decode_inner_event(&decrypted)
        }
    };

    match result {
        Ok(decrypted) => {
            cache_insert(&payload, Some(decrypted.clone()));
            Ok(decrypted)
        }
        Err(e) => {
            // Only terminal failures are cached for the process lifetime;
            // transient ones are recorded with a timestamp so the cooldown
            // above suppresses prompt storms without blocking recovery.
            if is_terminal(&e) {
                cache_insert(&payload, None);
            } else if !matches!(e, PrivateZapError::NoSigner) {
                record_transient_attempt(&payload);
            }
            Err(e)
        }
    }
}

fn decrypt_with_keys(
    secret_key: &SecretKey,
    zap_request: &Event,
) -> Result<DecryptedPrivateZap, PrivateZapError> {
    let inner = nip57::decrypt_received_private_zap_message(secret_key, zap_request)
        .map_err(|e| PrivateZapError::Crypto(e.to_string()))?;
    validate_inner_event(inner)
}

fn decode_inner_event(json: &str) -> Result<DecryptedPrivateZap, PrivateZapError> {
    let inner = Event::from_json(json).map_err(|e| PrivateZapError::Malformed(e.to_string()))?;
    validate_inner_event(inner)
}

fn validate_inner_event(inner: Event) -> Result<DecryptedPrivateZap, PrivateZapError> {
    if inner.kind != Kind::ZapPrivateMessage {
        return Err(PrivateZapError::Malformed(format!(
            "expected kind 9733, found {}",
            inner.kind.as_u16()
        )));
    }
    if inner.verify().is_err() {
        return Err(PrivateZapError::Crypto(
            "inner event signature invalid".to_string(),
        ));
    }
    let message = {
        let trimmed = inner.content.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };
    Ok(DecryptedPrivateZap {
        sender_pubkey: inner.pubkey,
        message,
    })
}

fn private_zap_cache() -> &'static Mutex<LruCache<String, Option<DecryptedPrivateZap>>> {
    static CACHE: OnceLock<Mutex<LruCache<String, Option<DecryptedPrivateZap>>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(256).expect("non-zero cache capacity"),
        ))
    })
}

fn cached_outcome(payload: &str) -> Option<Result<DecryptedPrivateZap, PrivateZapError>> {
    private_zap_cache()
        .lock()
        .ok()?
        .get(payload)
        .map(|opt| match opt.clone() {
            Some(decrypted) => Ok(decrypted),
            None => Err(PrivateZapError::Crypto(
                "private zap decryption previously failed".to_string(),
            )),
        })
}

fn cache_insert(payload: &str, outcome: Option<DecryptedPrivateZap>) {
    if let Ok(mut cache) = private_zap_cache().lock() {
        cache.put(payload.to_string(), outcome);
    }
}

/// Last-attempt timestamps for transient (retryable) failures, keyed by
/// payload — bounds re-prompt frequency without caching the outcome.
fn transient_attempts() -> &'static Mutex<LruCache<String, u64>> {
    static CACHE: OnceLock<Mutex<LruCache<String, u64>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(256).expect("non-zero cache capacity"),
        ))
    })
}

fn transient_attempt_pending_at(payload: &str, now: u64) -> bool {
    match transient_attempts().lock() {
        Ok(mut cache) => match cache.get(payload) {
            Some(last) => now.saturating_sub(*last) < TRANSIENT_RETRY_COOLDOWN_MS,
            None => false,
        },
        Err(_) => false,
    }
}

fn transient_attempt_pending(payload: &str) -> bool {
    transient_attempt_pending_at(payload, crate::platform::timestamp::now_millis())
}

fn record_transient_attempt(payload: &str) {
    if let Ok(mut cache) = transient_attempts().lock() {
        cache.put(
            payload.to_string(),
            crate::platform::timestamp::now_millis(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_relays() -> Vec<RelayUrl> {
        vec![RelayUrl::parse("wss://relay.damus.io").unwrap()]
    }

    fn signed_inner(sender: &Keys, recipient: &PublicKey, message: &str) -> Event {
        EventBuilder::new(Kind::ZapPrivateMessage, message)
            .tags(vec![Tag::public_key(*recipient)])
            .sign_with_keys(sender)
            .unwrap()
    }

    #[test]
    fn private_zap_roundtrip_via_local_keys() {
        let alice = Keys::generate(); // real sender
        let bob = Keys::generate(); // recipient
        let ephemeral = Keys::generate();

        let inner = signed_inner(&alice, &bob.public_key(), "secret message");
        let data = ZapRequestData::new(bob.public_key(), test_relays())
            .amount(21_000)
            .event_id(EventId::all_zeros());
        let outer = compose_private_zap_request(inner, &ephemeral, data).unwrap();

        // Outer shape: ephemeral signer, anon tag, valid signature.
        assert_eq!(outer.kind, Kind::ZapRequest);
        assert_eq!(outer.pubkey, ephemeral.public_key());
        assert_ne!(outer.pubkey, alice.public_key());
        assert!(outer.verify().is_ok());
        assert!(matches!(classify_anon(&outer), AnonKind::Private(_)));
        // The real sender never appears in the public event.
        assert!(!outer.as_json().contains(&alice.public_key().to_string()));

        // Recipient decrypts (local-keys SDK path).
        let decrypted = decrypt_with_keys(bob.secret_key(), &outer).unwrap();
        assert_eq!(decrypted.sender_pubkey, alice.public_key());
        assert_eq!(decrypted.message.as_deref(), Some("secret message"));

        // A different key cannot decrypt.
        let mallory = Keys::generate();
        assert!(decrypt_with_keys(mallory.secret_key(), &outer).is_err());
    }

    #[test]
    fn anon_payload_reconstruction_preserves_bytes() {
        let bob = Keys::generate();
        let ephemeral = Keys::generate();
        let payload = nip57::encrypt_private_zap_message(
            &mut OsRng,
            ephemeral.secret_key(),
            &bob.public_key(),
            "inner event json",
        )
        .unwrap();

        let nip04 = anon_payload_to_nip04(&payload).unwrap();
        let (ct_b64, iv_b64) = nip04.split_once("?iv=").unwrap();

        let mut segments = payload.split('_');
        let (_, ct) = bech32::decode(segments.next().unwrap()).unwrap();
        let (_, iv) = bech32::decode(segments.next().unwrap()).unwrap();

        assert_eq!(BASE64.decode(ct_b64).unwrap(), ct);
        assert_eq!(BASE64.decode(iv_b64).unwrap(), iv);
    }

    #[tokio::test]
    async fn remote_signer_path_roundtrip() {
        // The reconstructed NIP-04 payload decrypts via the NostrSigner trait
        // — the exact path NIP-07/46/55 signers take. `Keys` implements the
        // trait, so it stands in for a remote signer.
        let alice = Keys::generate();
        let bob = Keys::generate();
        let ephemeral = Keys::generate();

        let inner = signed_inner(&alice, &bob.public_key(), "hi bob");
        let data = ZapRequestData::new(bob.public_key(), test_relays()).amount(1_000);
        let outer = compose_private_zap_request(inner, &ephemeral, data).unwrap();

        let AnonKind::Private(payload) = classify_anon(&outer) else {
            panic!("expected private anon tag");
        };
        let nip04_payload = anon_payload_to_nip04(&payload).unwrap();
        let decrypted_json = bob.nip04_decrypt(&outer.pubkey, &nip04_payload).await.unwrap();
        let decrypted = decode_inner_event(&decrypted_json).unwrap();

        assert_eq!(decrypted.sender_pubkey, alice.public_key());
        assert_eq!(decrypted.message.as_deref(), Some("hi bob"));
    }

    #[test]
    fn anonymous_zap_request_classifies() {
        let bob = Keys::generate();
        let request =
            build_anonymous_zap_request(bob.public_key(), test_relays(), 1_000, None, None, None)
                .unwrap();
        assert_eq!(request.kind, Kind::ZapRequest);
        assert!(request.verify().is_ok());
        assert!(matches!(classify_anon(&request), AnonKind::Anonymous));
    }

    #[test]
    fn dip03_spec_example_payload_parses() {
        // Exact `anon` payload from the DIP-03 spec example — cross-client
        // interop check for the bech32 encoding.
        const SPEC_PAYLOAD: &str = "pzap1n0pkup9fxc9w3yd2tvfr03shffrapfz8rtzu9kkq6v222jw5rgtp9myyh378gdtwptpnls8f0rv0v2dyapgt7sssu4263puepgshsj9g4u9y5lvfv9fsujlgvywsuejvftlfzcanu5fmnf2a3grlelwe8v0z4mdkyhr9mddxpswtvp7mtlc4acdys7740t0x5ej36qs5amfzwz5dpwlaf4gsl69lzhqdgc3hgt62xw4y8384a6zvsnf96l3ardkd2vkk6cm77p6v7ul3gwgjr7tra7uzpkvf4hncxp5qd75h6cdadf6n2d7edhc3dyyy7qpdka2mgqhvckhzhd2gcaux34jyw6qfk3nxhaaqs6pqkuy6z34wu2p2fvqqvg55eyqlrndjlgekm7xu08lqc3g0nje59uqu0adqerv2puypez3eck9xzupg4vxyfclk37qfqxra8nt4tk9ydc2tzhpnl4wpf7jf2nrkchknfnfgmezfyqe074dexe5mkxgw67j7zn8s24tae8tml747qnq0edw5jxsx6xfc4qhshf3man0s5duw6wm63ue8fese8c7hanqzphjna3g0ee4jgpwceqzk9jgrvf9rnkt89tkvh75qm65nvtqpud30vecwlqzdlu9fhcaj7jv89gpy32y2k828vsj7x8hmlq55rleeq23e062apenymv96tkvltv266ww6kly2q2t7k6z_iv189a0s9afn7ehz4gpeanueh56cv6t79qk";
        let nip04 = anon_payload_to_nip04(SPEC_PAYLOAD).unwrap();
        let (ct_b64, iv_b64) = nip04.split_once("?iv=").unwrap();
        // AES-256-CBC shape checks: 16-byte IV, block-aligned ciphertext.
        assert_eq!(BASE64.decode(iv_b64).unwrap().len(), 16);
        assert_eq!(BASE64.decode(ct_b64).unwrap().len() % 16, 0);
    }

    #[test]
    fn malformed_payloads_rejected() {
        assert!(anon_payload_to_nip04("pzap1abc").is_err()); // missing iv
        assert!(anon_payload_to_nip04("pzap1abc_iv1def_extra").is_err()); // extra segment
        assert!(anon_payload_to_nip04("lnurl1abc_iv1def").is_err()); // wrong msg HRP
        assert!(anon_payload_to_nip04("pzap1abc_npub1def").is_err()); // wrong iv HRP
    }

    #[test]
    fn only_crypto_and_malformed_are_terminal() {
        assert!(is_terminal(&PrivateZapError::Crypto("x".to_string())));
        assert!(is_terminal(&PrivateZapError::Malformed("x".to_string())));
        assert!(!is_terminal(&PrivateZapError::NoSigner));
        assert!(!is_terminal(&PrivateZapError::Signer("x".to_string())));
        assert!(!is_terminal(&PrivateZapError::Timeout));
        assert!(!is_terminal(&PrivateZapError::Sign("x".to_string())));
    }

    #[test]
    fn transient_cooldown_window_enforced() {
        const PAYLOAD: &str = "transient-cooldown-test-payload";
        transient_attempts()
            .lock()
            .unwrap()
            .put(PAYLOAD.to_string(), 1_000_000);
        assert!(transient_attempt_pending_at(
            PAYLOAD,
            1_000_000 + TRANSIENT_RETRY_COOLDOWN_MS - 1
        ));
        assert!(!transient_attempt_pending_at(
            PAYLOAD,
            1_000_000 + TRANSIENT_RETRY_COOLDOWN_MS
        ));
        assert!(!transient_attempt_pending_at("never-attempted", 1_000_000));
    }

    #[test]
    fn terminal_failures_cache_as_anonymous_dead_ends() {
        const PAYLOAD: &str = "terminal-cache-test-payload";
        cache_insert(PAYLOAD, None);
        // A cached None surfaces as an error (renders as anonymous), so
        // repeated renders never re-attempt decryption.
        assert!(cached_outcome(PAYLOAD).is_some_and(|outcome| outcome.is_err()));
    }
}
