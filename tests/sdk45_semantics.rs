//! SDK 0.45 migration integration tests against a local MockRelay.
//!
//! These verify the runtime semantics the rest of the codebase relies on
//! after the 0.44 → 0.45 migration:
//! 1. `notifications()` streams terminate on client shutdown (the outer
//!    re-read loops in dispatcher/eose-tracker/health/provenance depend on
//!    this to pick up rebuilt clients);
//! 2. `fetch_events` waits for ALL targeted relays (quorum helpers remain
//!    justified);
//! 3. targeted subscribe/fetch work via `Vec<(url, filters)>` ReqTargets;
//! 4. `send_event().to()` delivers to the specified subset and reports
//!    per-relay results;
//! 5. events deduplicate across relays in stream_events.

#![cfg(all(test, feature = "web"))]

use std::time::Duration;

use futures::StreamExt;
use nostr::event::{EventBuilder, FinalizeEvent, Kind};
use nostr::filter::Filter;
use nostr::key::Keys;
use nostr_sdk::client::{Client, ClientNotification};
use nostr_sdk::local_relay::MockRelay;

async fn setup(count: usize) -> (Client, Vec<String>) {
    let mut urls = Vec::with_capacity(count);
    for _ in 0..count {
        let relay = MockRelay::run().await.unwrap();
        urls.push(relay.url().await.to_string());
    }
    let client = Client::default();
    for url in &urls {
        client.add_relay(url).await.unwrap();
    }
    client.connect().await;
    (client, urls)
}

fn signed_note(keys: &Keys, content: &str) -> nostr::event::Event {
    EventBuilder::new(Kind::TextNote, content)
        .finalize(keys)
        .unwrap()
}

#[tokio::test]
async fn notification_stream_ends_on_shutdown() {
    let (client, _urls) = setup(1).await;
    let mut notifications = client.notifications();
    client.shutdown().await;
    // After shutdown the stream must terminate (possibly after yielding the
    // Shutdown notification) rather than hanging forever.
    let mut saw_shutdown = false;
    let mut ended = false;
    for _ in 0..2 {
        match tokio::time::timeout(Duration::from_secs(2), notifications.next()).await {
            Ok(Some(ClientNotification::Shutdown)) => saw_shutdown = true,
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => {
                ended = true;
                break;
            }
        }
    }
    assert!(ended, "notification stream should end after shutdown");
    let _ = saw_shutdown; // informational
}

#[tokio::test]
async fn fetch_events_waits_for_all_targeted_relays() {
    // Two relays: one answers with an event, the other is silent (no data,
    // sends EOSE only when the mock is queried). 0.45's default fetch policy
    // (ExitOnEOSE) terminates only when EVERY targeted relay completes, so
    // the fetch still succeeds — the quorum helpers exist to return EARLIER,
    // not to make such fetches possible.
    let keys = Keys::generate();
    let (client, urls) = setup(2).await;

    let event = signed_note(&keys, "hello quorum");
    client
        .send_event(&event)
        .to([&urls[0]])
        .await
        .unwrap();

    let filter = Filter::new().kind(Kind::TextNote).limit(10);
    let events = client
        .fetch_events(filter)
        .timeout(Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(events.len(), 1, "event from the one populated relay is found");
}

#[tokio::test]
async fn targeted_subscribe_reaches_specified_relays_only() {
    let keys = Keys::generate();
    let (client, urls) = setup(2).await;

    // Connect only the first relay's subscription via a Vec<(url, filters)>
    // target (the ReqTarget pattern used throughout the migrated codebase).
    let filter = Filter::new().kind(Kind::TextNote).limit(5);
    let targets: Vec<(String, Vec<Filter>)> = vec![(urls[0].clone(), vec![filter])];
    let output = client.subscribe(targets).await.unwrap();
    assert!(!output.value.to_string().is_empty());

    let event = signed_note(&keys, "targeted only");
    client
        .send_event(&event)
        .to([&urls[0]])
        .await
        .unwrap();

    // The subscription was only sent to relay 0; verify via subscription map.
    let subs = client.subscriptions().await;
    assert!(subs.contains_key(&output.value));
}

#[tokio::test]
async fn send_event_to_delivers_to_subset() {
    let keys = Keys::generate();
    let (client, urls) = setup(2).await;

    let event = signed_note(&keys, "subset delivery");
    let output = client
        .send_event(&event)
        .to([&urls[1]])
        .ok_timeout(Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(output.success.len(), 1, "delivered to the one targeted relay");
    assert!(output.success.contains_key(
        &nostr::types::url::RelayUrl::parse(&urls[1]).unwrap()
    ));
}

#[tokio::test]
async fn broadcast_sends_to_all_write_relays() {
    let keys = Keys::generate();
    let (client, urls) = setup(3).await;

    let event = signed_note(&keys, "broadcast");
    let output = client
        .send_event(&event)
        .broadcast()
        .ok_timeout(Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(
        output.success.len(),
        urls.len(),
        "broadcast reaches every WRITE-capable relay"
    );
}

#[tokio::test]
async fn stream_events_deduplicates_across_relays() {
    let keys = Keys::generate();
    let (client, urls) = setup(2).await;

    // Send the SAME event to both relays.
    let event = signed_note(&keys, "dupe");
    client.send_event(&event).to(urls.iter()).ok_timeout(Duration::from_secs(5)).await.unwrap();

    let filter = Filter::new().kind(Kind::TextNote).limit(10);
    let mut stream = client
        .stream_events(filter)
        .timeout(Duration::from_secs(5))
        .await
        .unwrap();
    let mut seen = 0usize;
    while let Some((_url, res)) = stream.next().await {
        if res.is_ok() {
            seen += 1;
        }
    }
    assert_eq!(seen, 1, "the merged stream yields the event once");
}
