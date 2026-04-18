# nostr.blue Publish Queue — Research Findings & Implementation Plan

## Table of Contents
1. [Primal Reference Architecture](#1-primal-reference-architecture)
2. [nostr.blue Current Publishing System](#2-nostrblue-current-publishing-system)
3. [Framework & SDK Validation](#3-framework--sdk-validation)
4. [Complete Call Site Inventory](#4-complete-call-site-inventory)
5. [Revised Implementation Plan](#5-revised-implementation-plan)
6. [Risk Assessment](#6-risk-assessment)

---

## 1. Primal Reference Architecture

### Overview
Primal uses a **two-tier queue system**:
1. **Signing queue** — serial `Queue` class ensuring one signing op at a time
2. **Relay publish queue** — persisted in localStorage, 16-second auto-retry countdown, sequential processing

### Key Behavior
`sendEvent()` returns `{ success: true }` **immediately** before signing or relay publish even begins.

### Core Data Structures (Primal)

**AccountStore queue fields** (`src/stores/accountStore.ts`):
```typescript
eventQueue: NostrRelaySignedEvent[],     // The persisted publish queue
eventQueueRetry: number,                  // Countdown timer (0-16) for auto-retry
sendErrors: Record<string, string>,       // Error messages keyed by event ID
```

**Signing Queue** (`src/lib/nostrAPI.ts`):
```typescript
type QueueItem = {
  action: () => Promise<any>,
  resolve: (result: any) => void,
  reject: (reason: any) => void,
};

export class Queue {
  #items: QueueItem[];
  #pendingPromise: boolean;
  // Serial promise queue - one signing op at a time
}
```

### Primal Flow (Direct Relay Path)
```
User Action → sendEvent(event) → returns { success: true } IMMEDIATELY (optimistic)
  → signEvent(event) [serial Queue]
  → sendSignedEvent(signedNote)
    → relayWorker.postMessage('SEND_EVENT')
    → relayWorker: enqueEvent (notify main thread)
    → relayWorker: Promise.any(pool.publish(relays, event))
      → First relay OK → EVENT_SENT + DEQUE_EVENT
      → All relays fail → EVENT_NOT_SENT (stays in queue)
```

### Primal Retry Mechanism
- **16-second countdown timer** — auto-retries all queued events when countdown hits 0
- **Sequential processing** — `processArrayUntilFailure()` stops on first failure
- **Persistence** — localStorage per-user, loaded on login
- **Deduplication** — by event `id`; for Settings kind, by `kind + content + d-tag`

### Primal UI
- **EventQueueWidget** — nav bar badge showing "Publish pending (N)" after 12-second delay
- **EventQueue page** (`/pending`) — full management with retry/abort controls

---

## 2. nostr.blue Current Publishing System

### Three Tiers of Sophistication

| Tier | Pattern | Retry | Queue | Persistence | Examples |
|------|---------|-------|-------|-------------|----------|
| **Tier 1: Fire-and-forget** | `client.send_event_builder(tag_event_builder(builder))` | None | None | None | Notes, reactions, reposts, articles, contacts, mute lists, profile (26+ functions) |
| **Tier 2: Retry with backoff** | Recursive async with generation counter | Exponential backoff (3 retries) | In-memory only | Rollback state | Pinned notes, pinned communities |
| **Tier 3: Durable queue** | Background processor with adaptive polling | Adaptive backoff (5 retries) | IndexedDB | Full event serialization | Cashu wallet events only |

### Core Publish Flow (Tier 1 — Most Events)
**File**: `src/stores/nostr_client/notes.rs:73-133`
```rust
pub async fn publish_note_tracked(content: String, tags: Vec<Vec<String>>) -> Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let builder = nostr::EventBuilder::text_note(&content).tags(mention_tags);
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let result = PublishResult::from_output(output);
    Ok(result)
}
```

### PublishResult Type
**File**: `src/stores/nostr_client/types.rs:1-85`
```rust
pub struct PublishResult {
    pub event_id: String,
    pub successful_relays: Vec<String>,
    pub failed_relays: Vec<(String, String)>,
}
// Methods: from_output(), is_success(), has_failures(), success_count(), total_attempted()
```

### NIP-89 Client Tagging
**File**: `src/utils/nips/nip89.rs`
Every published event passes through `tag_event_builder()` which adds `["client", "nostr.blue"]`.

### Tier 2: Retry with Exponential Backoff (Pinned Notes/Communities)
**Files**: `src/stores/social/pinned_notes.rs`, `src/stores/social/pinned_communities.rs`
- Optimistic UI with local state updates
- Debounced publishing (1s delay)
- Generation counter (`AtomicU64`) prevents stale publishes
- Exponential backoff: `1000ms * 2^retry_count`, max 3 retries
- Rollback on failure
- Platform-specific: `#[cfg(feature = "web")]` vs `#[cfg(feature = "native")]`

### Tier 3: Cashu Durable Event Queue (Most Sophisticated)
**File**: `src/stores/cashu/events.rs`

**PendingNostrEvent type** (`src/stores/cashu/types.rs:722-761`):
```rust
pub struct PendingNostrEvent {
    pub id: String,              // UUID
    pub builder_json: String,    // MISLEADING NAME — always contains signed Event JSON, never EventBuilder
    pub event_type: PendingEventType,
    pub created_at: u64,
    pub retry_count: u32,
    pub last_retry_at: Option<u64>,
    pub pending_token_id: Option<String>,
    pub mint_url: Option<String>,
    // ... more fields
}
```

**Global signal**: `PENDING_NOSTR_EVENTS: GlobalSignal<Vec<PendingNostrEvent>>` (`src/stores/cashu/signals.rs:413`)

**Background processor** (`src/stores/cashu/events.rs:1188-1256`):
- Singleton guard via `AtomicBool`
- Adaptive interval: 30s active, 60s idle
- Max 5 retries
- Backoff: `BASE_DELAY * (1 + retry_count/2)`, capped 60s
- Persistence to IndexedDB via `SHARED_LOCALSTORE`

**Platform variants**:
- WASM: `dioxus::prelude::spawn` + `gloo_timers::future::TimeoutFuture`
- Native: `dioxus_core::spawn_forever` + `tokio::time::sleep`

**IMPORTANT**: The WASM loop sleeps first then processes; native processes first then sleeps.

### Signing Helper (Cashu — Should Be Promoted)
**File**: `src/stores/cashu/events.rs:146`
```rust
pub async fn sign_event_builder_with_signer(
    builder: nostr_sdk::EventBuilder,
    signer: crate::stores::signer::SignerType,
) -> Result<nostr_sdk::Event, String> {
    let builder = crate::utils::nips::nip89::tag_event_builder(builder);
    match signer {
        SignerType::Keys(keys) => builder.sign_with_keys(&keys).map_err(|e| e.to_string()),
        #[cfg(target_family = "wasm")]
        SignerType::BrowserExtension(s) => builder.sign(&*s).await.map_err(|e| e.to_string()),
        SignerType::NostrConnect(s) => builder.sign(&*s).await.map_err(|e| e.to_string()),
        #[cfg(feature = "mobile_platform")]
        SignerType::AndroidSigner(s) => builder.sign(&*s).await.map_err(|e| e.to_string()),
    }
}
```

**DUPLICATED** in at least 4 other files:
- `stores/cashu/mint_mgmt.rs:64-82`
- `stores/cashu/payment_request.rs:53-71`
- `stores/media/blossom_store.rs:440-450, 625-640, 717-730`
- `stores/media/gif_store.rs:419`

### IndexedDB Storage Layer
**File**: `src/stores/indexeddb_database.rs`

Database: `cashu_wallet_db`, Version: `6`

Existing stores:
| Store Name | Purpose |
|-----------|---------|
| `mints` | Mint URLs and info |
| `keysets` | Keysets per mint |
| `keyset_by_id` | Keyset lookup |
| `keys` | Cryptographic keys |
| `mint_quotes` | Pending mint quotes |
| `melt_quotes` | Pending melt quotes |
| `proofs` | Ecash proofs |
| `transactions` | Transaction history |
| `keyset_counters` | Derivation counters |
| `pending_events` | **Cashu pending events** (to be migrated) |
| `sync_state` | Fetch timestamps |
| `pending_secrets` | Pending proof secrets |
| `in_flight_melts` | Crash recovery |
| `nutzap_settings` | Nutzap config |
| `pending_nutzaps` | Pending nutzaps |

Generic helpers (private):
```rust
async fn put_value<T>(&self, store_name: &str, key: &str, value: &T) -> Result<(), database::Error>
where T: Serialize + ?Sized

async fn get_value<T>(&self, store_name: &str, key: &str) -> Result<Option<T>, database::Error>
where T: for<'de> Deserialize<'de>

async fn get_all_values<T>(&self, store_name: &str) -> Result<Vec<T>, database::Error>
where T: for<'de> Deserialize<'de>

async fn delete_value(&self, store_name: &str, key: &str) -> Result<(), database::Error>
```

### Existing Store Pattern (dioxus-stores)
All Vec-like reactive data uses `#[derive(Store)]` with single `data` field:
```rust
#[derive(Clone, Debug, Default, Store)]
pub struct WalletTokensStore { pub data: Vec<TokenData> }
// Usage: WALLET_TOKENS.read().data().read() / .write()
```
20+ examples throughout the codebase. The `#[derive(Store)]` macro generates accessor methods for each field with independent reactivity.

---

## 3. Framework & SDK Validation

### Dioxus Framework Findings

**Source**: `/home/patrick/dioxus`

#### GlobalSignal Constraints
- `GlobalSignal<T>` requires only `T: 'static`
- No `Clone`, `PartialEq`, or `Send`/`Sync` required for `Signal<T>`
- `Clone` needed only for `signal()` syntax (calling `QUEUE()` to get clone)
- Defined as `pub type GlobalSignal<T> = Global<Signal<T>, T>` (`packages/signals/src/global/signal.rs`)
- Lives on `ScopeId::ROOT` — persists for entire app lifetime

#### Vec Mutation API
`WritableVecExt<T>` blanket-implemented for all `Writable<Target = Vec<T>>`:
```rust
fn push(&mut self, value: T)
fn pop(&mut self) -> Option<T>
fn remove(&mut self, index: usize) -> T
fn retain(&mut self, f: impl FnMut(&T) -> bool)
fn clear(&mut self)
fn extend(&mut self, iter: impl IntoIterator<Item = T>)
// etc.
```
So `PUBLISH_QUEUE.write().push(item)` works on `GlobalSignal<Vec<T>>`.

#### Background Tasks
- `spawn(fut)` — tied to current component's scope; canceled when component unmounts
- `spawn_forever(fut)` — tied to `ScopeId::ROOT`; lives until VirtualDom dropped
- **For persistent background tasks, use `spawn_forever`**

#### Reactivity
- ANY write via `.write()` or `.with_mut()` triggers ALL subscribers
- No granular Vec change detection on plain `Signal<Vec<T>>`
- Use `Store<T>` (dioxus-stores) for per-field/per-index reactivity
- `#[derive(Store)]` generates accessor methods for each struct field with independent reactivity

### nostr-sdk Findings

**Source**: `/home/patrick/nostr`

#### EventBuilder — NOT Serializable
```rust
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct EventBuilder { ... }
// Does NOT implement Serialize or Deserialize
```
**Critical**: Cannot persist EventBuilder. Must sign first, persist the signed Event.

#### Event (Signed) — Fully Serializable
```rust
// Implements Serialize/Deserialize via EventIntermediate
// Also implements JsonUtil: from_json(), as_json(), as_pretty_json()
```

#### Client::send_event_builder
```rust
pub async fn send_event_builder(&self, builder: EventBuilder) -> Result<Output<EventId>, Error> {
    let event: Event = self.sign_event_builder(builder).await?;  // Signs internally
    self.send_event(&event).await                                 // Sends with default AckPolicy::all()
}
```

#### Client::send_event (returns builder)
```rust
pub fn send_event<'event, 'url>(&self, event: &'event Event) -> SendEvent<'_, 'event, 'url>
```
Returns a `SendEvent` builder with:
- `.broadcast()` — send to all WRITE relays
- `.to(urls)` — send to specific relays
- `.ack_policy(AckPolicy)` — `all()` (wait for OK) or `none()` (fire-and-forget)
- `.ok_timeout(Duration)` — default 10s
- `.save_into_database(bool)` — default true

#### AckPolicy — The Key to Optimistic Publishing
```rust
pub struct AckPolicy(InnerAckPolicy);
impl AckPolicy {
    pub const fn all() -> Self   // Wait for relay OK (default)
    pub const fn none() -> Self  // Fire-and-forget: dispatch EVENT msg, don't wait for OK
}
```
With `AckPolicy::none()`: EVENT message is sent to relay WebSocket, returns immediately. `output.success` contains all relays where the WebSocket write succeeded (but NOT relay acceptance).

#### Output<EventId>
```rust
pub struct Output<T: Debug> {
    pub val: T,                                // The EventId
    pub success: HashSet<RelayUrl>,            // Relays that succeeded
    pub failed: HashMap<RelayUrl, String>,     // Relays that failed + error messages
}
```
Even on partial relay failure, returns `Ok(Output)`. Only returns `Err` for fundamental failures (no relays, not connected, etc.).

#### Relay Pool Publishing — ALL Relays, Not Promise.any
The SDK sends to ALL relays concurrently and waits for ALL to respond. Not Promise.any (first-success-wins).

#### No Built-in Retry/Queue
The SDK has **no** built-in event publishing retry or queue mechanism. Must be implemented at application level.

#### Signer Types
```rust
pub trait NostrSigner: Any + Debug + Send + Sync {
    fn backend(&self) -> SignerBackend<'_>;
    fn get_public_key(&self) -> BoxedFuture<'_, Result<PublicKey, SignerError>>;
    fn sign_event(&self, unsigned: UnsignedEvent) -> BoxedFuture<'_, Result<Event, SignerError>>;
    // nip04/nip44 encrypt/decrypt
}
```
Backends: `Keys`, `BrowserExtension` (NIP-07), `NostrConnect` (NIP-46), `Custom`

#### Manual Signing
```rust
let signer = client.signer().ok_or(Error::SignerNotConfigured)?;
let event: Event = client.sign_event_builder(builder).await?;
// OR
let event: Event = builder.sign(&*signer).await?;
```

---

## 4. Complete Call Site Inventory

### Category 1: publish_note_tracked → Result<PublishResult, String>

| # | File:Line | What it does with result | Needs update? |
|---|-----------|--------------------------|---------------|
| 1 | `components/photo_card.rs:639` | Reads `result.event_id`, `result.success_count()`, `result.total_attempted()`, iterates `result.failed_relays` | **YES** — relay detail |
| 2 | `components/photo_card.rs:681` | Same as above | **YES** — relay detail |
| 3 | `components/note_composer.rs:40` | Reads `result.success_count()`, `result.total_attempted()`, `result.event_id`, `result.has_failures()` for UI feedback | **YES** — relay detail |

Wrapper `publish_note` → `Result<String, String>` (extracts event_id only): no callers need relay detail.

### Category 2: publish_reaction_tracked → Result<PublishResult, String>

| # | File:Line | What it does with result | Needs update? |
|---|-----------|--------------------------|---------------|
| 1 | `hooks/use_reaction.rs:283` | Reads `result.event_id`, `result.success_count()`, `result.total_attempted()`, iterates `result.failed_relays`. Sets `ReactionState::Success` | **YES** — relay detail |
| 2 | `hooks/use_reaction.rs:362` | Same pattern (emoji-specific) | **YES** — relay detail |

### Category 3: publish_repost_tracked / publish_repost

`publish_repost` wrapper → `Result<String, String>`, called from:
| # | File:Line | Needs update? |
|---|-----------|---------------|
| 1 | `components/threaded_comment.rs:504` | No (just event_id) |
| 2 | `components/note_card.rs:755` | No |
| 3 | `components/photo_card.rs:490` | No |
| 4 | `components/poll/card.rs:470` | No |
| 5 | `components/voice/message_card.rs:327` | No |

### Category 4: publish_article / publish_article_tracked

No external callers of `_tracked` variant. Wrapper `publish_article` → `Result<String, String>`.

### Category 5: publish_metadata / publish_metadata_tracked

| # | File:Line | What it does | Needs update? |
|---|-----------|--------------|---------------|
| 1 | `components/profile_editor_modal.rs:112` | Match on result, shows success/error | No (just String) |
| 2 | `stores/nostr_client/profile.rs:101` | `update_profile_field` propagates error | No |

### Category 6: publish_enriched_contacts (private)

Internal callers in `contacts.rs` (lines 305, 325, 347, 401). External callers of `follow_user`/`unfollow_user` → `Result<(), String>`:
| # | File:Line | Needs update? |
|---|-----------|---------------|
| 1 | `components/note_menu.rs:189` | No |
| 2 | `routes/profile.rs:647` | No |
| 3 | `components/content_menu.rs:176` | No |
| 4 | `routes/packs/pack_detail.rs:613` | No |
| 5 | `routes/code/code_user_profile.rs:236,254` | No |
| 6 | `stores/social/packs_store.rs:460` | No |

### Category 7: publish_picture/video/voice_message_tracked

No external callers of `_tracked` variants. Wrappers → `Result<String, String>`.

### Category 8: publish_edit → Result<EditPublishResult, String>

| # | File:Line | What it does | Needs update? |
|---|-----------|--------------|---------------|
| 1 | `components/edit_post.rs:183` | Checks `result.publish.is_success()`, reads `result.publish.event_id`, `result.publish.success_count()`, `result.publish.total_attempted()`. Calls `edit_cache::process_edit_event(&result.event)` | **YES** — relay detail + needs Event object |

### Category 9: publish_nutzap_info → Result<String, String>

| # | File:Line | What it does | Needs update? |
|---|-----------|--------------|---------------|
| 1 | `components/cashu/nutzap_settings_modal.rs:95` | Uses event_id for display | No (just String) |

### Category 10: relay_publishing.rs functions

| # | Function | File:Line | What it does | Needs update? |
|---|----------|-----------|--------------|---------------|
| 1 | `broadcast_presigned_event` | `components/note_menu.rs:426` | Reads `result.is_success()`, `result.has_failures()`, `result.success_count()`, `result.total_attempted()` | **YES** — relay detail |
| 2 | `send_presigned_event_to_relays` | `components/reply_composer.rs:259` | Reads `result.success_count() > 0`, `result.event_id`, `result.success_count()`, `result.total_attempted()` | **YES** — relay detail |
| 3 | `send_presigned_event_to_relays` | `stores/content/draft_store.rs:279` | Reads `result.event_id`, `result.success_count()`, `result.total_attempted()` for logging | **YES** — relay detail |
| 4 | `send_presigned_event_to_relays` | `stores/content/draft_store.rs:409` | Same pattern | **YES** — relay detail |
| 5 | `publish_vanish_request_to_relays` | `routes/settings/home.rs:1447` | Match on result, reads relay counts | **YES** — relay detail |

### Category 11: Calendar publish functions

All → `Result<String, String>`:
| # | Function | File:Line | Needs update? |
|---|----------|-----------|---------------|
| 1 | `publish_rsvp` | `routes/events/event_detail.rs:299` | No (ignores return value) |
| 2 | `publish_event_comment` | `routes/events/event_detail.rs:329` | No |
| 3 | `publish_date_event` | `routes/events/calendar_event_new.rs:342` | No |
| 4 | `publish_time_event` | `routes/events/calendar_event_new.rs:370` | No |

### Category 12: NIP-65 publish functions

All → `Result<String, String>`:
| # | Function | File:Line | Needs update? |
|---|----------|-----------|---------------|
| 1 | `publish_relay_list` | `routes/settings/settings_relays.rs:367` | No (error only) |
| 2 | `publish_dm_relay_list` | `settings_relays.rs:372` | No |
| 3 | `publish_search_relays` | `settings_relays.rs:377` | No |
| 4 | `publish_blocked_relays` | `settings_relays.rs:382` | No |

### Category 13: Shop publish functions

All → `Result<String, String>`:
| # | Function | File:Line | Needs update? |
|---|----------|-----------|---------------|
| 1 | `publish_product` | `routes/shop/shop_product_new.rs:247` | No |
| 2 | `publish_review` | `components/shop/review_form.rs:135` | No |
| 3 | `publish_collection` | `routes/shop/shop_collection_new.rs:188` | No |
| 4 | `delete_collection` | `routes/shop/shop_merchant.rs:515` | No |
| 5 | `delete_product` | `routes/shop/shop_merchant.rs:460` | No |
| 6 | `update_product` | `routes/shop/shop_product_edit.rs:287` | No |

### Category 14: Git Hosting publish functions

All use `send_event_builder` directly → `Result<String, String>`:
- `services/git_hosting/ssh_keys.rs` (lines 95, 131)
- `services/git_hosting/stars.rs` (lines 32, 71)
- `services/git_hosting/issues.rs` (lines 169, 191, 219)
- `services/git_hosting/discussions.rs` (lines 69, 102)
- `services/git_hosting/snippets.rs` (lines 146, 168)
- `services/git_hosting/bounties.rs` (lines 59, 109, 165)
- `services/git_hosting/releases.rs` (lines 61, 83)
- `services/git_hosting/repository.rs` (lines 155, 207, 230)
- `services/git_hosting/pull_requests.rs` (line 21)
All only extract `output.id().to_hex()`. **No relay detail needed.**

### Category 15: Cashu publish/queue functions

**Files with `send_event_builder`/`send_event` calls to migrate:**
| # | File | Lines | Context |
|---|------|-------|---------|
| 1 | `stores/cashu/events.rs` | 177, 343, 726, 1023, 1134 | Queue processing |
| 2 | `stores/cashu/init.rs` | 223 | Wallet snapshot |
| 3 | `stores/cashu/internal.rs` | 482, 552 | Internal helpers |
| 4 | `stores/cashu/mint_mgmt.rs` | 90, 770, 1237, 1356 | Mint management |
| 5 | `stores/cashu/mpp.rs` | 319, 438 | Multi-part payments |
| 6 | `stores/cashu/nutzap.rs` | 160, 457, 464, 1110 | Nutzap send/receive |
| 7 | `stores/cashu/payment_request.rs` | 90 | Payment request |
| 8 | `stores/cashu/receive.rs` | 374 | Receive tokens |
| 9 | `stores/cashu/send.rs` | 620, 675 | Send tokens |
| 10 | `stores/cashu/swap.rs` | 570, 682 | Swap tokens |
| 11 | `stores/cashu/transfer.rs` | 259, 315, 366 | Transfer tokens |

### Category 16: Pinned Notes/Communities

| # | Function | File:Line | Returns | External callers need update? |
|---|----------|-----------|---------|-------------------------------|
| 1 | `pin_event` | `pinned_notes.rs:129` | `Result<(), String>` | No (`note_menu.rs:266`) |
| 2 | `unpin_event` | `pinned_notes.rs:162` | `Result<(), String>` | No |
| 3 | `pin_community` | `pinned_communities.rs:161` | `Result<(), String>` | No |
| 4 | `unpin_community` | `pinned_communities.rs:175` | `Result<(), String>` | No |

### Category 17: Direct send_event_builder/send_event callers (outside tracked functions)

**Social stores** (`stores/social/`):
| # | File | Lines |
|---|------|-------|
| 1-8 | `community_store/publish.rs` | 41, 66, 97, 132, 168, 200, 246, 332 |
| 9-11 | `dms.rs` | 339, 361, 467 |
| 12-15 | `topic_store/publish.rs` | 21, 58, 95, 164 |
| 16-21 | `pin_boards_store/publish.rs` | 42, 136, 158, 184, 235, 256 |
| 22 | `pinned_communities.rs` | 334 |
| 23 | `pinned_notes.rs` | 348 |
| 24-25 | `packs_store.rs` | 366, 408 |
| 26-27 | `channel_store.rs` | 352, 379 |
| 28 | `reactions_store.rs` | 289 |

**Other stores**:
| # | File | Lines |
|---|------|-------|
| 29-30 | `nostr_music.rs` | 710, 764 |
| 31 | `podcast_subscription.rs` | 338 |
| 32 | `bookmarks.rs` | 453 |
| 33 | `citation_store.rs` | 535, 587, 644, 710 |
| 34-35 | `draft_store.rs` | 214 |
| 36-37 | `publication_store.rs` | 947, 987 |
| 38-42 | `recipe_store.rs` | 562, 615, 670, 695, 720, 771 |
| 43-46 | `wiki_store.rs` | 513, 555, 581, 618 |
| 47 | `zap_goals_store.rs` | 726 |
| 48-50 | `directory_store.rs` | 610, 662, 713 |
| 51 | `dvm_store.rs` | 169 |
| 52 | `embedding_store.rs` | 386 |
| 53 | `media/blossom_store.rs` | 647 |
| 54 | `media/gif_store.rs` | 435 |
| 55-58 | `muting.rs` | 148, 185, 248, 286, 346 |
| 59-64 | `polls.rs` | 80, 88, 96, 195, 200, 208 |

**UI stores**:
| # | File | Lines |
|---|------|-------|
| 65 | `emoji_store.rs` | 391 |
| 66 | `notifications.rs` | 94 |
| 67 | `settings_store.rs` | 228 |
| 68 | `sidebar_store.rs` | 572 |

**Components**:
| # | File | Lines |
|---|------|-------|
| 69-70 | `blobbi/actions/care_actions.rs` | 102, 144 |
| 71-72 | `blobbi/core/builders.rs` | 206, 317 |
| 73 | `blobbi/onboarding/onboarding_flow.rs` | 276 |
| 74 | `blobbi/social/breeding_modal.rs` | 129 |
| 75 | `blobbi/social/photo_modal.rs` | 96 |
| 76 | `blobbi/social/records_modal.rs` | 171 |
| 77 | `code/review_section.rs` | 41 |
| 78 | `comment_composer.rs` | 190 |
| 79 | `content_share_modal.rs` | 261 |
| 80-81 | `list/add_to_list_modal.rs` | 508, 552 |
| 82 | `list/create_list_modal.rs` | 315 |
| 83 | `live/chat.rs` | 182 |
| 84 | `live/share_modal.rs` | 236 |
| 85 | `share_modal.rs` | 253 |
| 86-87 | `hooks/blobbi/use_blobbi_sleep.rs` | 32, 74 |
| 88 | `hooks/use_lists.rs` | 232 |

**Routes**:
| # | File | Lines |
|---|------|-------|
| 89 | `routes/radio/radio_station_new.rs` | 480 |
| 90 | `routes/video/live_stream_new.rs` | 265 |
| 91 | `routes/code/code_repo_new_file.rs` | 519 |
| 92 | `routes/code/code_repo_edit_file.rs` | 493 |

**Utils**:
| # | File | Lines |
|---|------|-------|
| 93-95 | `utils/list_encryption.rs` | 116, 165, 290 |
| 96-99 | `utils/nips/nip58.rs` | 355, 388, 433, 465 |
| 100 | `utils/nips/nip84.rs` | 318 |

**Web bookmarks**:
| # | File | Lines |
|---|------|-------|
| 101-102 | `stores/webbookmarks.rs` | 62, 104 |

### Summary Statistics

| Metric | Count |
|--------|-------|
| Total SDK publish call sites | ~200+ |
| Callers using PublishResult relay details | **10** (need targeted UI update) |
| Callers using only event_id or error propagation | ~190+ (no change needed) |
| Cashu-specific queue functions | ~10 functions, ~20+ internal call sites |
| Duplicated signing helper copies | 4+ files |

---

## 5. Revised Implementation Plan

### Design Decisions
- **Queue type**: New universal queue, replace Cashu queue entirely
- **Persistence**: IndexedDB via existing `SHARED_LOCALSTORE` / `IndexedDbDatabase`
- **Optimism level**: Return after signing, before relay send
- **Scope**: Full — core queue + migrate all publishers + UI indicator + management page

### Approach: Internal-Change-Only (Zero Signature Changes)

Instead of changing function signatures across 200+ call sites, change the **internal implementation** of publish functions while keeping return types backward-compatible.

```
BEFORE (current):
  publish_note_tracked(builder)
    → client.send_event_builder(builder).await  [blocks until relays respond]
    → PublishResult { event_id, successful_relays, failed_relays }

AFTER (new):
  publish_note_tracked(builder)
    → sign_event_builder(builder).await          [signs synchronously]
    → enqueue(signed_event)                       [queues for background relay send]
    → PublishResult { event_id, queued: true, successful_relays: [], failed_relays: [] }
```

### File Structure

```
src/stores/publish_queue/
├── mod.rs           # Queue operations + public API + module init
├── types.rs         # QueuedEvent, QueueEventStatus, PublishQueueStore
├── processor.rs     # Background processor (WASM + native variants)
├── persistence.rs   # IndexedDB save/load via SHARED_LOCALSTORE
└── signing.rs       # Shared signing helper (promoted from cashu/events.rs)

src/components/
└── publish_queue_indicator.rs  # Nav bar widget

src/routes/
└── publish_queue.rs            # Management page (/pending)
```

### Phase 1: Core Queue Infrastructure

#### Types (`src/stores/publish_queue/types.rs`)

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum QueueEventStatus {
    Pending,
    Publishing,
    Success,
    PartialFailure,
    Failed { error: String },
    MaxRetriesExceeded { error: String },
    Aborted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuedEvent {
    pub id: String,
    pub event_json: String,
    pub event_type: String,
    pub event_id: String,
    pub status: QueueEventStatus,
    pub target_relays: Option<Vec<String>>,
    pub created_at: u64,
    pub retry_count: u32,
    pub max_retries: u32,
    pub last_retry_at: Option<u64>,
    pub metadata: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Store)]
pub struct PublishQueueStore {
    pub events: Vec<QueuedEvent>,
}
```

#### Global Signals
```rust
pub static PUBLISH_QUEUE: GlobalSignal<Store<PublishQueueStore>> =
    Signal::global(|| Store::new(PublishQueueStore::default()));
```

#### Signing Helper (`src/stores/publish_queue/signing.rs`)
Promote from `cashu/events.rs:146`. Handles all 4 signer types with proper cfg gates.

#### Queue Operations (`src/stores/publish_queue/mod.rs`)
- `enqueue(event, event_type, target_relays, metadata) -> String` — Returns queue UUID
- `deque(id)` — Remove + persist
- `update_status(id, status)` — Update + persist
- `get_pending() -> Vec<QueuedEvent>` — Filter helper
- `clear_completed()` — Remove Success/Aborted

#### Background Processor (`src/stores/publish_queue/processor.rs`)
- WASM variant: `spawn` + `gloo_timers::future::TimeoutFuture`
- Native variant: `spawn_forever` + `tokio::time::sleep`
- Adaptive polling: 10s active, 60s idle
- Per-event:
  1. Deserialize `Event` from `event_json`
  2. `client.send_event(&event).ack_policy(AckPolicy::all()).await` (or `.to(urls)` if target_relays set)
  3. Success → `Success` → auto-remove after 30s
  4. Partial failure → increment retry, backoff
  5. Total failure → `Failed` → retry with backoff
  6. Max retries (5) → `MaxRetriesExceeded`
- Singleton guard via `AtomicBool`

#### Persistence (`src/stores/publish_queue/persistence.rs`)
- Add `STORE_PUBLISH_QUEUE = "publish_queue"` constant
- Bump `DB_VERSION` in `indexeddb_database.rs`
- Add new object store in `on_upgrade_needed`
- Public methods: `add_queued_event`, `get_all_queued_events`, `remove_queued_event`, `update_queued_event`

### Phase 2: Modify PublishResult (Additive Only)

```rust
pub struct PublishResult {
    pub event_id: String,
    pub queue_id: Option<String>,      // NEW
    pub queued: bool,                   // NEW: true if queued (not yet relay-confirmed)
    pub successful_relays: Vec<String>, // Empty when queued=true
    pub failed_relays: Vec<(String, String)>, // Empty when queued=true
}

impl PublishResult {
    pub fn queued(queue_id: String, event_id: String) -> Self {
        Self {
            event_id,
            queue_id: Some(queue_id),
            queued: true,
            successful_relays: vec![],
            failed_relays: vec![],
        }
    }
    // Keep existing from_output() unchanged
    pub fn is_success(&self) -> bool { true } // Always true — optimistic
    pub fn has_failures(&self) -> bool { !self.queued && !self.failed_relays.is_empty() }
}
```

### Phase 3: Modify Publish Functions (Internal Only)

Each `publish_*_tracked` function changes from:
```rust
let output = client.send_event_builder(tag_event_builder(builder)).await?;
Ok(PublishResult::from_output(output))
```
To:
```rust
let event = sign_event_builder(builder).await?;
let event_id = event.id.to_hex();
let queue_id = enqueue(event, "note", None, HashMap::new());
Ok(PublishResult::queued(queue_id, event_id))
```

**Files to modify** (internal implementation only, no signature changes):

| File | Functions |
|------|-----------|
| `stores/nostr_client/notes.rs` | `publish_note_tracked` |
| `stores/nostr_client/reactions.rs` | `publish_reaction_tracked` |
| `stores/nostr_client/reposts.rs` | `publish_repost_tracked` |
| `stores/nostr_client/articles.rs` | `publish_article_tracked` |
| `stores/nostr_client/contacts.rs` | `publish_enriched_contacts` |
| `stores/nostr_client/profile.rs` | `publish_metadata_tracked` |
| `stores/nostr_client/media.rs` | `publish_picture_tracked`, `publish_video_tracked`, `publish_voice_message_tracked` |
| `stores/nostr_client/muting.rs` | all mute/unmute/block/report |
| `stores/nostr_client/custom_nips.rs` | `publish_custom_nip_tracked` |
| `stores/nostr_client/edits.rs` | `publish_edit` |
| `stores/nostr_client/relay_publishing.rs` | all 5 functions |
| `stores/calendar_store/publish.rs` | 6 functions |
| `stores/relay/nip65.rs` | 4 functions |
| `stores/shop_store/mod.rs` | 6 functions |
| `services/git_hosting/*` | ~15 functions |
| All ~100+ direct `send_event_builder` callers in social stores, content stores, components, routes, utils |

### Phase 4: Update ~10 UI Callers That Use Relay Details

| File | Component | Change needed |
|------|-----------|---------------|
| `components/photo_card.rs:639,681` | Reply publish | Remove relay count display; show "Published" |
| `components/note_composer.rs:40` | Note composer | Show "Publishing..." → "Published" instead of relay counts |
| `hooks/use_reaction.rs:283,362` | Reaction toggle | Set Success immediately; remove relay logging |
| `components/edit_post.rs:183` | Edit publish | Remove relay detail checks; keep Event object usage |
| `components/note_menu.rs:426` | Broadcast | Show "Broadcast queued" instead of relay counts |
| `components/reply_composer.rs:259` | Reply publish | Show "Published" instead of relay counts |
| `stores/content/draft_store.rs:279,409` | Draft publish | Log "queued" instead of relay counts |
| `routes/settings/home.rs:1447` | Vanish request | Show "Request queued" |

### Phase 5: Replace Cashu Queue

1. Add migration in IndexedDB `on_upgrade_needed` (bump `DB_VERSION` to 7)
2. Load old `PendingNostrEvent` entries, convert to `QueuedEvent`, save to new `publish_queue` store
3. Redirect all Cashu callers to use `publish_queue::enqueue()`
4. Remove Cashu-specific queue code:
   - `cashu/events.rs`: Remove `queue_nostr_event`, `queue_signed_event_for_retry*`, `queue_event_for_retry`, `queue_token_event_for_retry*`, `process_pending_events`, `start_pending_events_processor`, `PENDING_NOSTR_EVENTS` references
   - `cashu/types.rs`: Remove `PendingNostrEvent`, `PendingEventType`
   - `cashu/signals.rs`: Remove `PENDING_NOSTR_EVENTS`
   - `cashu/init.rs`: Replace `load_pending_events` + `start_pending_events_processor` with universal queue load + processor start
5. Remove duplicated signing helpers in `mint_mgmt.rs`, `payment_request.rs`, `blossom_store.rs`, `gif_store.rs` — redirect to `publish_queue::sign_event_builder`

### Phase 6: Migrate Pinned Notes/Communities

- Remove ad-hoc retry logic from `pinned_notes.rs` and `pinned_communities.rs`
- Their publish calls now go through universal queue
- Keep generation counter / rollback state for local optimistic updates
- Keep `SyncStatus` signals reflecting queue status

### Phase 7: UI Components

#### Queue Indicator (`src/components/publish_queue_indicator.rs`)
- Badge in nav bar
- Shows count of `Pending` + `Failed` events
- Only visible when count > 0
- Pulsing animation when `Publishing`
- Click → navigate to `/pending`

#### Queue Management Page (`src/routes/publish_queue.rs`)
- Route: `/pending`
- List grouped by status (Failed, Pending, Publishing, Success)
- Per-event: type icon, event preview (truncated content from event_json), status badge, timestamp, retry count
- Actions: Retry, Abort
- Bulk: Retry All Failed, Clear Completed
- Reactive via `Store<PublishQueueStore>` per-item reactivity

### Phase 8: Startup Integration

- On login / client init: `load_publish_queue()` from IndexedDB → populate `PUBLISH_QUEUE` → `start_publish_queue_processor()`
- On logout: stop processor, clear queue from memory
- In `App` component: call init in `use_hook`

---

## 6. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Signing fails but UI showed success | Low | Low | Signing errors still return `Err`; signing is pre-queue |
| ~10 UI callers show stale relay data | Medium | Low | Targeted updates in Phase 4; graceful degradation |
| IndexedDB version bump breaks wallets | Low | High | Additive migration only; keep old store for rollback |
| Android signer blocks processor (45s) | Medium | Medium | Process events sequentially; Android events wait their turn |
| Events lost on crash before IndexedDB write | Low | High | Write to IndexedDB immediately on enqueue |
| `PublishResult` backward compat breakage | Low | High | Additive changes only; `is_success()` always true |
| Cashu migration data loss | Low | High | Convert old entries before removing old store |
| `send_event_builder_to` deprecation | Low | Low | Use `send_event(&event).to(urls)` for targeted sends |
| WASM ordering: sleep-then-process | Low | Low | Match existing Cashu pattern; initial delay acceptable |

### What Stays Unchanged
- All function signatures across the entire codebase
- `PublishResult` existing fields and methods (additive only)
- All callers that just use `event_id` or `?` error propagation (~190+ sites)
- Cashu wallet business logic (only queue infrastructure changes)
- Relay connection management, event subscriptions
- No changes to any non-publish code paths

### Dependencies Verified
- [x] `dioxus-stores` `0.7.3` supports `#[derive(Store)]` for multi-field structs
- [x] `nostr_sdk::AckPolicy` re-exported and accessible
- [x] `indexed_db_futures` supports store creation in `on_upgrade_needed`
- [x] `gloo-timers` futures feature available
- [x] `dioxus_core::spawn_forever` available on native (13 existing uses)
