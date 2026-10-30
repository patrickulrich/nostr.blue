//! Unified fetch facade — the single entry point for one-shot reads.
//!
//! Every one-shot read should go through [`fetch`] (auto routing) or
//! [`fetch_with_client`] (client-parameterized, testable) instead of raw
//! `client.fetch_events(filter).timeout(...)`. The facade centralizes:
//!
//! - client acquisition (single `peek`-based getter, one error variant)
//! - signer/relay readiness gating (`wait_for_user_relays` when signed in)
//! - EOSE quorum early-exit (see [`super::eose`])
//! - the `max_events` ceiling (`Backfill` raises the SDK's 10k-error default)
//! - client-side re-validation of results (`Filter::match_event`) — relays
//!   are untrusted and occasionally return events that do not match the REQ
//!
//! Kind 0 / 10002 / 10050 (metadata / relay lists / DM relays) MUST NOT go
//! through here — they only exist on DISCOVERY-only indexer relays that plain
//! REQs cannot reach. Use `relay::nip65::fetch_events_from_indexers` (or the
//! batched profile pipeline in `stores::profiles`) instead.
use super::eose;
use super::error::Error;
use super::fetching::{fetch_event_targeted, parse_event_id};
use super::signals::{HAS_SIGNER, NOSTR_CLIENT};
use crate::stores::relay;
use dioxus::prelude::ReadableExt;
use nostr::filter::{Filter, MatchEventOptions};
use nostr::nips::nip01::Coordinate;
use nostr::nips::nip19::Nip19;
use nostr_sdk::client::Client;
use nostr_sdk::prelude::*;
use std::time::Duration;

/// How long [`fetch`] waits for the user's NIP-65 relays to be applied before
/// proceeding anyway (logged-out users skip the gate entirely).
const USER_RELAYS_GATE_TIMEOUT: Duration = Duration::from_secs(8);

/// Named fetch profile: the only place fetch timeouts and event ceilings live.
///
/// Mapped onto the SDK's `FetchEvents` builder knobs (`timeout`,
/// `ReqExitPolicy`, `max_events`) — see the plan's validation annex. The
/// `Backfill` profile exists because the SDK's default `max_events` of 10,000
/// *errors* (`limit_exceeded`) rather than truncating, which silently broke
/// large feed backfills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchProfile {
    /// Point lookups and cheap UI chrome (badges, single counters): 3s.
    Fast,
    /// Card-level interaction fetches (replies/reactions/zaps): 5s.
    Ui,
    /// Default content fetches: 10s.
    Std,
    /// Cross-relay content with sparse coverage: 15s.
    Slow,
    /// Large feed backfills: 30s, raised event ceiling (no 10k error).
    Backfill,
    /// NIP-50 full-text search: 5s; validation relaxes nip50 matching (R1).
    Search,
    /// Sparse discovery kinds (e.g. NIP-87 mint registry 38172/38000): 10s
    /// like `Std`, but with the 3s min-collect floor — few relays hold
    /// these kinds, so fast-EOSE empty relays must not satisfy the quorum
    /// before the content-heavy ones finish streaming. `Backfill`'s floor
    /// without its 30s window.
    #[allow(dead_code)] // constructed in the cashu-gated mint discovery (mint_mgmt.rs)
    Discovery,
}

impl FetchProfile {
    /// Overall fetch deadline.
    pub fn timeout(self) -> Duration {
        match self {
            Self::Fast => Duration::from_secs(3),
            Self::Ui => Duration::from_secs(5),
            Self::Std => Duration::from_secs(10),
            Self::Slow => Duration::from_secs(15),
            Self::Backfill => Duration::from_secs(30),
            Self::Search => Duration::from_secs(5),
            Self::Discovery => Duration::from_secs(10),
        }
    }

    /// Maximum number of unique events to collect.
    pub fn max_events(self) -> usize {
        match self {
            Self::Backfill => 50_000,
            _ => 10_000,
        }
    }

    /// Minimum collection window before the quorum early-exit may fire on
    /// the Auto path. `ZERO` for interactive tiers; `Backfill` sets 3s.
    ///
    /// Rationale: the quorum break now requires quorum AND this floor, so a
    /// single early event from a fast relay plus fast-EOSE empty relays can
    /// no longer close the fetch while content-heavy relays are still
    /// streaming (the marketplace browse regression: 1-of-843 products).
    pub fn min_collect(self) -> Duration {
        match self {
            Self::Backfill | Self::Discovery => Duration::from_secs(3),
            _ => Duration::ZERO,
        }
    }

    /// Re-validation options per R1: all fields, except nip50 search matching
    /// is relaxed for search-tier filters. The SDK's nip50 `match_event`
    /// support is a naive whole-substring content check while real NIP-50
    /// relays match terms independently — enforcing it would falsely drop
    /// legitimate relay results. Structural fields stay enforced everywhere.
    fn validation_opts(filters: &[Filter]) -> MatchEventOptions {
        if filters.iter().any(|f| f.search.is_some()) {
            MatchEventOptions::new().nip50(false)
        } else {
            MatchEventOptions::new()
        }
    }
}

/// Where to run the REQ: the pool's auto routing (READ relays, gossip when
/// configured) or an explicit per-relay filter map.
#[derive(Clone, Debug)]
pub enum FetchTarget {
    /// Automatic relay selection (pool READ capability, gossip-augmented).
    Auto(Vec<Filter>),
    /// Explicit per-relay filters (`Vec<(url, Vec<Filter>)>` ReqTarget shape).
    Targeted(Vec<(RelayUrl, Vec<Filter>)>),
}

impl From<Filter> for FetchTarget {
    fn from(f: Filter) -> Self {
        Self::Auto(vec![f])
    }
}

impl From<Vec<Filter>> for FetchTarget {
    fn from(f: Vec<Filter>) -> Self {
        Self::Auto(f)
    }
}

impl From<Vec<(RelayUrl, Vec<Filter>)>> for FetchTarget {
    fn from(t: Vec<(RelayUrl, Vec<Filter>)>) -> Self {
        Self::Targeted(t)
    }
}

/// Client-side re-validation of fetch results against the originating filters
/// (relays are untrusted). Pure function — unit-testable without a relay.
/// Events arriving from the notification path are already deduplicated by the
/// SDK; this only checks REQ relevance, not validity (that is the admission
/// policy's job, which runs pre-persist).
pub fn validate_against_filters(filters: &[Filter], events: Vec<Event>) -> Vec<Event> {
    if filters.is_empty() {
        return events;
    }
    let opts = FetchProfile::validation_opts(filters);
    let (kept, dropped): (Vec<Event>, Vec<Event>) = events
        .into_iter()
        .partition(|ev| filters.iter().any(|f| f.match_event(ev, opts)));
    if !dropped.is_empty() {
        log::debug!(
            "facade: dropped {}/{} events failing client-side filter re-validation",
            dropped.len(),
            dropped.len() + kept.len()
        );
    }
    kept
}

/// Fetch with an explicit client — the testable core (no global state, no
/// readiness gating). Auto targets use the quorum-EOSE path (early exit once
/// half the reached relays EOSE); targeted targets use the SDK builder path
/// with the profile's timeout and event ceiling.
pub async fn fetch_with_client(
    client: &Client,
    target: FetchTarget,
    profile: FetchProfile,
) -> Result<Vec<Event>, Error> {
    match target {
        FetchTarget::Auto(filters) => {
            let events = super::fetching::fetch_events_quorum_multi(
                client,
                filters.clone(),
                profile.timeout(),
                profile.max_events(),
                profile.min_collect(),
            )
            .await
            .map_err(Error::Relay)?;
            Ok(validate_against_filters(&filters, events))
        }
        FetchTarget::Targeted(targets) => {
            let filters: Vec<Filter> = targets
                .iter()
                .flat_map(|(_, fs)| fs.iter().cloned())
                .collect();
            let events = client
                .fetch_events(targets)
                .timeout(profile.timeout())
                .max_events(profile.max_events())
                .await?;
            Ok(validate_against_filters(&filters, events.into_iter().collect()))
        }
    }
}

/// Public facade: acquire the client, gate on signer/relay readiness, then
/// delegate to [`fetch_with_client`]. Use this everywhere instead of raw
/// `client.fetch_events(...)`.
pub async fn fetch(
    target: impl Into<FetchTarget>,
    profile: FetchProfile,
) -> Result<Vec<Event>, Error> {
    let client: std::sync::Arc<Client> = NOSTR_CLIENT.peek().clone().ok_or(Error::NotInitialized)?;
    // Readiness gate: subscriptions/fetches issued before the user's NIP-65
    // relays are applied fire against an incomplete pool. No-op when logged
    // out. (See AGENTS.md "Signer & Relay Readiness Gating".)
    if *HAS_SIGNER.peek() {
        relay::wait_for_user_relays(USER_RELAYS_GATE_TIMEOUT, "nostr_client::facade::fetch").await;
    }
    fetch_with_client(&client, target.into(), profile).await
}

/// A parsed `hex` / `note1` / `nevent1` / `naddr1` (optionally `nostr:`-prefixed)
/// event reference, ready to be located.
#[derive(Clone, Debug)]
pub enum EventRef {
    /// By event id, with optional author + relay hints (from `nevent1`).
    Id(super::fetching::ParsedEventId),
    /// By coordinate (`naddr1`), with relay hints.
    Coordinate {
        kind: u16,
        pubkey: String,
        identifier: String,
        relay_hints: Vec<String>,
    },
}

/// Parse an event reference in any common addressing format.
/// Pure function — unit-testable.
pub fn parse_event_ref(input: &str) -> Option<EventRef> {
    if let Some(parsed) = parse_event_id(input) {
        return Some(EventRef::Id(parsed));
    }
    let trimmed = input.trim();
    let normalized = trimmed
        .strip_prefix("nostr:")
        .or_else(|| trimmed.strip_prefix("NOSTR:"))
        .unwrap_or(trimmed);
    if let Ok(Nip19::Coordinate(coord)) = Nip19::from_bech32(normalized) {
        let Coordinate {
            kind,
            public_key,
            identifier,
        } = (*coord).clone();
        return Some(EventRef::Coordinate {
            kind: kind.as_u16(),
            pubkey: public_key.to_hex(),
            identifier,
            relay_hints: coord.relays.iter().map(|r| r.to_string()).collect(),
        });
    }
    None
}

/// An event located through the facade, with best-effort provenance.
#[derive(Clone, Debug)]
pub struct LocatedEvent {
    pub event: Event,
    /// Relays known to have served the event. Currently best-effort (empty
    /// until the underlying fetch ladder grows provenance plumbing).
    #[allow(dead_code)] // consumed by Phase 2 provenance/observability work
    pub seen_on: Vec<String>,
    /// `"ladder"` (store-first multi-phase id lookup) or `"coordinate"`.
    #[allow(dead_code)] // consumed by Phase 2 provenance/observability work
    pub source: &'static str,
}

/// Locate a single event by any NIP-19 reference (`hex`/`note1`/`nevent1`/
/// `naddr1`). Store-first, then the existing multi-phase fetch ladder
/// (broadcast → author outbox → relay hints → gossip). Newest-wins and
/// deletion semantics are handled by the underlying ladder / DB layer;
/// results are re-validated against the derived filter (R1).
pub async fn fetch_event_by_id(
    input: &str,
    profile: FetchProfile,
) -> Result<Option<LocatedEvent>, Error> {
    if *HAS_SIGNER.peek() {
        relay::wait_for_user_relays(
            USER_RELAYS_GATE_TIMEOUT,
            "nostr_client::facade::fetch_event_by_id",
        )
        .await;
    }
    match parse_event_ref(input) {
        Some(EventRef::Id(parsed)) => {
            let filter = Filter::new().id(parsed.event_id).limit(1);
            let event = fetch_event_targeted(parsed, profile.timeout())
                .await
                .map_err(Error::Relay)?;
            Ok(event
                .filter(|ev| {
                    let opts = FetchProfile::validation_opts(std::slice::from_ref(&filter));
                    filter.match_event(ev, opts)
                })
                .map(|event| LocatedEvent {
                    event,
                    seen_on: Vec::new(),
                    source: "ladder",
                }))
        }
        Some(EventRef::Coordinate {
            kind,
            pubkey,
            identifier,
            relay_hints,
        }) => {
            // Parse the author before it is moved into the ladder call;
            // an unparseable author matches nothing (fail-closed).
            let author = PublicKey::parse(pubkey.as_str());
            let event = super::articles::fetch_event_by_coordinate_with_relays(
                kind,
                pubkey,
                identifier.clone(),
                relay_hints,
            )
            .await
            .map_err(Error::Relay)?;
            // Re-validate against the coordinate the caller asked for. The
            // fetch ladder's hint phases (naddr relays, author outbox) apply
            // no validation of their own and relays are untrusted: kind,
            // author AND `d` must all match, else a relay can satisfy any
            // lookup with an arbitrary event carrying the same `d` value.
            let event = event.filter(|ev| {
                ev.kind.as_u16() == kind
                    && author.is_ok_and(|a| ev.pubkey == a)
                    && match identifier.as_str() {
                        // Replaceable kinds carry no `d`: an absent
                        // identifier still matches an empty request.
                        "" => true,
                        d => ev.tags.identifier().map(|v| v.as_str() == d).unwrap_or(false),
                    }
            });
            Ok(event.map(|event| LocatedEvent {
                event,
                seen_on: Vec::new(),
                source: "coordinate",
            }))
        }
        None => Ok(None),
    }
}

// Re-export for callers that want the threshold policies next to the facade.
pub use eose::{fetch_quorum_threshold, realtime_liveness_threshold};

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::FinalizeEvent;
    use nostr::nips::nip19::ToBech32;

    #[test]
    fn profiles_map_to_expected_knobs() {
        assert_eq!(FetchProfile::Fast.timeout(), Duration::from_secs(3));
        assert_eq!(FetchProfile::Ui.timeout(), Duration::from_secs(5));
        assert_eq!(FetchProfile::Std.timeout(), Duration::from_secs(10));
        assert_eq!(FetchProfile::Slow.timeout(), Duration::from_secs(15));
        assert_eq!(FetchProfile::Backfill.timeout(), Duration::from_secs(30));
        assert_eq!(FetchProfile::Search.timeout(), Duration::from_secs(5));
        // Backfill raises the SDK's 10k error ceiling.
        assert_eq!(FetchProfile::Backfill.max_events(), 50_000);
        assert_eq!(FetchProfile::Std.max_events(), 10_000);
        // Backfill floors the quorum early-exit; interactive tiers are un-floored.
        assert_eq!(FetchProfile::Backfill.min_collect(), Duration::from_secs(3));
        assert_eq!(FetchProfile::Std.min_collect(), Duration::ZERO);
        assert_eq!(FetchProfile::Fast.min_collect(), Duration::ZERO);
        // Discovery: Std's window with Backfill's floor.
        assert_eq!(FetchProfile::Discovery.timeout(), Duration::from_secs(10));
        assert_eq!(
            FetchProfile::Discovery.min_collect(),
            Duration::from_secs(3)
        );
        assert_eq!(FetchProfile::Discovery.max_events(), 10_000);
    }

    #[test]
    fn validation_drops_non_matching_events() {
        let keys = nostr::key::Keys::generate();
        let event = nostr::event::EventBuilder::new(nostr::event::Kind::TextNote, "hello")
            .finalize(&keys)
            .unwrap();
        // Matching kind filter keeps the event.
        let filter = Filter::new().kind(nostr::event::Kind::TextNote);
        let out = validate_against_filters(std::slice::from_ref(&filter), vec![event.clone()]);
        assert_eq!(out.len(), 1);
        // Non-matching kind filter drops it.
        let other = Filter::new().kind(nostr::event::Kind::Metadata);
        let out = validate_against_filters(&[other], vec![event]);
        assert!(out.is_empty());
    }

    #[test]
    fn validation_relaxes_nip50_for_search_filters() {
        let keys = nostr::key::Keys::generate();
        // Relay-side search matches terms independently; content has the terms
        // non-adjacent, which the SDK's substring nip50 match would reject.
        let event = nostr::event::EventBuilder::new(nostr::event::Kind::TextNote, "foo … bar")
            .finalize(&keys)
            .unwrap();
        let filter = Filter::new().search("foo bar".to_string());
        // R1: search-tier filters skip nip50 enforcement instead of dropping
        // the legitimate relay result.
        let out = validate_against_filters(std::slice::from_ref(&filter), vec![event.clone()]);
        assert_eq!(out.len(), 1, "search-tier results must not be nip50-filtered");
        // Sanity: the SDK's own nip50 matcher would have dropped it, proving
        // the relaxation is load-bearing.
        let opts = MatchEventOptions::new();
        assert!(!filter.match_event(&event, opts));
    }

    #[test]
    fn parse_event_ref_supports_all_formats() {
        let hex = "aa".repeat(32);
        assert!(matches!(
            parse_event_ref(&hex),
            Some(EventRef::Id(_))
        ));
        let keys = nostr::key::Keys::generate();
        let npub = keys.public_key().to_hex();
        // note1 of a fake id: build via Nip19::EventId bech32
        let id = nostr::event::EventId::from_byte_array([0u8; 32]);
        let note = nostr::nips::nip19::Nip19::EventId(id).to_bech32().unwrap();
        assert!(matches!(parse_event_ref(&note), Some(EventRef::Id(_))));
        // naddr1
        let coord = Coordinate::new(nostr::event::Kind::LongFormTextNote, keys.public_key());
        let naddr = nostr::nips::nip19::Nip19::Coordinate(nostr::nips::nip19::Nip19Coordinate {
            coordinate: coord,
            relays: Vec::new(),
        })
        .to_bech32()
        .unwrap();
        match parse_event_ref(&naddr) {
            Some(EventRef::Coordinate { kind, pubkey, identifier, .. }) => {
                assert_eq!(kind, 30_023);
                assert_eq!(pubkey, npub);
                assert!(identifier.is_empty());
            }
            other => panic!("expected Coordinate, got {other:?}"),
        }
        // Garbage is None, not a panic.
        assert!(parse_event_ref("not-a-ref").is_none());
    }

    #[test]
    fn backfill_profile_raises_event_ceiling() {
        // The SDK's default ceiling errors at 10,000; Backfill must raise it.
        assert!(FetchProfile::Backfill.max_events() > 10_000);
        assert_eq!(FetchProfile::Backfill.max_events(), 50_000);
    }

    // ---- MockRelay contract tests (plan item #10; dev-dep `local-relay`) ----
    // These live in-crate (not tests/) because the crate is binary-only —
    // there is no lib target for integration tests to link against.
    // Native-only: the `local-relay` dev-dep (tokio/net) cannot build for
    // wasm32; wasm test builds compile these modules but skip these cases.

    #[cfg(not(target_arch = "wasm32"))]
    async fn mock_setup(count: usize) -> (Client, Vec<String>) {
        let mut urls = Vec::with_capacity(count);
        for _ in 0..count {
            let relay = nostr_sdk::local_relay::MockRelay::run().await.unwrap();
            urls.push(relay.url().await.to_string());
        }
        let client = Client::default();
        for url in &urls {
            client.add_relay(url).await.unwrap();
        }
        client.connect().await;
        (client, urls)
    }

    fn mock_note(keys: &nostr::key::Keys, content: &str) -> Event {
        use nostr::event::FinalizeEvent;
        nostr::event::EventBuilder::new(nostr::event::Kind::TextNote, content)
            .finalize(keys)
            .unwrap()
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn facade_auto_path_uses_quorum_and_returns_events() {
        let keys = nostr::key::Keys::generate();
        let (client, urls) = mock_setup(3).await;

        // send_event auto-persists the event to the client DB, so the relay
        // responses arrive as raw `Message(Event)` — NOT first-seen
        // `ClientNotification::Event`. This exercises exactly the
        // already-cached-event refresh path the quorum collector must handle.
        let event = mock_note(&keys, "facade auto");
        client
            .send_event(&event)
            .to(urls.iter())
            .ok_timeout(Duration::from_secs(5))
            .await
            .unwrap();

        let filter = Filter::new().kind(nostr::event::Kind::TextNote).limit(10);
        let events = fetch_with_client(&client, FetchTarget::Auto(vec![filter]), FetchProfile::Std)
            .await
            .unwrap();
        assert_eq!(events.len(), 1, "auto (quorum) path finds the stored event");
        assert_eq!(events[0].id, event.id);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn quorum_dedups_the_same_event_across_relays() {
        // The same event returned by every relay must be collected once:
        // `Message(Event)` repeats per relay while first-seen
        // `ClientNotification::Event` fires at most once.
        let keys = nostr::key::Keys::generate();
        let (client, urls) = mock_setup(3).await;

        let event = mock_note(&keys, "same event everywhere");
        client
            .send_event(&event)
            .to(urls.iter())
            .ok_timeout(Duration::from_secs(5))
            .await
            .unwrap();

        let filter = Filter::new().kind(nostr::event::Kind::TextNote).limit(10);
        let events = fetch_with_client(&client, FetchTarget::Auto(vec![filter]), FetchProfile::Std)
            .await
            .unwrap();
        assert_eq!(
            events.len(),
            1,
            "event present on all 3 relays is collected exactly once"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn facade_targeted_path_reaches_specified_relays() {
        let keys = nostr::key::Keys::generate();
        let (client, urls) = mock_setup(2).await;

        let event = mock_note(&keys, "facade targeted");
        client
            .send_event(&event)
            .to([&urls[1]])
            .ok_timeout(Duration::from_secs(5))
            .await
            .unwrap();

        let filter = Filter::new().kind(nostr::event::Kind::TextNote).limit(10);
        let targets = vec![(RelayUrl::parse(&urls[1]).unwrap(), vec![filter])];
        let events = fetch_with_client(&client, FetchTarget::Targeted(targets), FetchProfile::Ui)
            .await
            .unwrap();
        assert_eq!(events.len(), 1, "targeted path finds the event on relay 1");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn quorum_floor_blocks_break_until_min_collect_elapses() {
        // Regression shape of the marketplace browse bug: relays EOSE
        // instantly with an event already collected, so the quorum is met
        // milliseconds into the fetch. With a floor, the break must wait for
        // the floor timer (not fire on the early EOSE), and the timer branch
        // must end the fetch at the floor instead of running to the timeout.
        let keys = nostr::key::Keys::generate();
        let (client, urls) = mock_setup(3).await;

        let event = mock_note(&keys, "floored");
        client
            .send_event(&event)
            .to(urls.iter())
            .ok_timeout(Duration::from_secs(5))
            .await
            .unwrap();

        let filter = Filter::new().kind(nostr::event::Kind::TextNote).limit(10);
        let floor = Duration::from_millis(250);
        let started = std::time::Instant::now();
        let events = super::super::fetching::fetch_events_quorum_multi(
            &client,
            vec![filter],
            Duration::from_secs(10),
            usize::MAX,
            floor,
        )
        .await
        .unwrap();
        let elapsed = started.elapsed();

        assert_eq!(events.len(), 1, "floored fetch still collects the event");
        assert!(
            elapsed >= floor,
            "quorum+event must NOT break before the floor (broke at {elapsed:?})"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "floor timer must end the fetch at the floor, not the timeout ({elapsed:?})"
        );
    }

    #[tokio::test]
    async fn facade_search_tier_does_not_nip50_filter_results() {
        // R1: search filters relax nip50 re-validation because the SDK's
        // substring matcher would falsely drop legitimate relay search results.
        let keys = nostr::key::Keys::generate();
        // Terms present but non-adjacent — the SDK's own nip50 match rejects it.
        let event = mock_note(&keys, "foo … bar");
        let filter = Filter::new().search("foo bar".to_string());
        let out = validate_against_filters(std::slice::from_ref(&filter), vec![event.clone()]);
        assert_eq!(out.len(), 1, "search-tier results survive validation");
    }
}
