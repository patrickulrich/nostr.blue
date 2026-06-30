# nostr.blue Agent Guidelines

## Git

**Use `git add -A`** before committing (not `git commit -a` which misses new files).

## Build & CI

### Pre-commit Checks

Run these before committing (all must pass):

```bash
# Dioxus
dx check

# Web (WASM) - default feature
cargo check --target wasm32-unknown-unknown
cargo clippy --target wasm32-unknown-unknown -- -D warnings

# Desktop
cargo check --no-default-features --features desktop
cargo clippy --no-default-features --features desktop -- -D warnings

# Mobile
cargo check --no-default-features --features mobile
cargo clippy --no-default-features --features mobile -- -D warnings
cargo check --no-default-features --features playstore
cargo clippy --no-default-features --features playstore -- -D warnings

# Tests (104 inline #[cfg(test)] blocks across the codebase)
cargo test
```

**Validation priority order:** a task is not finished until the code type-checks and builds. Treat the order as: `dx check` / `cargo check` (compile) → `cargo clippy -D warnings` (lint) → `cargo test` (correctness). A failure at any level blocks the next. Code that compiles but fails clippy is not done.

### Local Dev

```bash
npm install                   # Install TailwindCSS 4 + esbuild
npm run dev                   # Build assets + dx serve (hot reload)
npm run build                 # Production build (assets + dx build --release)
```

### npm Build Pipeline

`npm run build:assets` runs two steps:
1. **TailwindCSS 4**: `tailwindcss -i tailwind.css -o public/tailwind.css --minify`
2. **esbuild**: Bundles `src/js/git-worker.js` → `public/git-worker.js` (ESM, es2020, minified)

### Desktop Build Dependencies

Desktop builds require `cmake` (for vendored libopus) and `libasound2-dev` (for cpal ALSA backend):

```bash
sudo apt-get install -y cmake libasound2-dev
```

### CI/CD

- **CI** (`ci.yml`): Runs on PRs to `main`. Three parallel jobs: WASM check+lint, native checks+tests, Android APK build.
- **Deploy** (`deploy.yml`): Runs on push to `main`. Builds web (GitHub Pages), Linux AppImage, signed Android APK + AAB. Creates GitHub Release with all artifacts + changelog from `public/docs/changelogs/{version}.md`.
- Version is read from `Cargo.toml` (currently `0.8.17`).

## Engineering Principles

### Verify, Don't Guess

Don't assert a diagnosis you haven't reproduced. This repo gives you cheap verification tools — `cargo test` (838 tests), `dx check`, and `dx serve` for live reload. If a claim is checkable in under a minute, check it before stating it: reproduce the failure first, then explain it. Read the actual SDK source under `/home/patrick/nostr` and Dioxus under `/home/patrick/dioxus` rather than assuming an API's behavior — relay flag semantics and connection lifecycle in particular are easy to get wrong from memory.

### Dependency Licensing

**MANDATORY whenever you introduce a new crate** to `Cargo.toml`. This project ships under **MIT**. Determine the dependency's license from its actual `LICENSE`/`COPYING` file or crates.io — **not from memory** — then classify:

- **Permissive** (MIT, Apache-2.0, BSD, ISC, MPL-2.0, zlib) → OK, proceed.
- **LGPL, or GPL/EPL with a linking/Classpath exception** → WARN. Acceptable to link, but call it out in your summary. Confirm the exception actually exists in the license text.
- **Stricter than LGPL** — GPL/AGPL without a linking exception, SSPL, proprietary → STRONGLY WARN and STOP. Surface it prominently and require an explicit maintainer call-out. Prefer a permissive alternative.

## Project Overview

**nostr.blue** is a comprehensive Nostr social client built with Rust + Dioxus, compiled to WebAssembly for browsers, Android via WebView, and Linux Desktop. It is a full-featured social platform with **150+ routes** and **450+ component files**.

**Stack**: Dioxus 0.7 (reactive UI), dioxus-stores (global state), rust-nostr SDK 0.44, CDK 0.16 (Cashu ecash), TailwindCSS 4

**Platforms**: Web (WASM), Android (WebView), Linux Desktop (AppImage)

**Key Dependencies**: `nostr-sdk`, `nostr-database`, `nostr-blossom`, `cdk`, `bitcoin`, `lightning-invoice`, `pulldown-cmark`, `ammonia`, `quick-xml`, `reqwest`, `tokio`, `image`, `rschess`, `geohash`, `zeroize`, `dioxus-primitives`

### Feature Areas

| Feature | Route Prefix | Key Paths |
|---------|-------------|-----------|
| Home Feed | `/` | `routes/home/`, `components/note_card.rs` |
| Explore / Search / Trending | `/explore`, `/search`, `/trending` | `services/search/` |
| Notifications | `/notifications` | `stores/ui/notifications.rs` |
| DMs | `/dms` | `stores/social/dms.rs` |
| Channels | `/chats` | `stores/social/channel_store.rs` |
| Communities (NIP-72) | `/communities` | `stores/social/community_store/` |
| Groups (NIP-29) | `/groups` | `stores/social/group_store/`, `components/groups/` |
| Topics (Reddit-like) | `/topics` | `stores/social/topic_store/`, `components/topic/` |
| Notes | `/note/:id`, `/notes/new` | `components/note_card.rs`, `components/note_composer.rs` |
| Articles (NIP-23) | `/articles` | `routes/articles/`, `components/article_card.rs` |
| Publications | `/publications` | `routes/articles/`, `stores/content/publication_store.rs` |
| Videos / Live Streams | `/videos` | `routes/video/`, `components/video_card.rs`, `components/live/` |
| Music | `/music` | `routes/music/`, `components/music/`, `stores/audio/`, `services/wavlake.rs` |
| Podcasts | `/podcast` | `routes/podcast/`, `components/podcast/`, `services/podcasts/` |
| Radio | `/radio` | `routes/radio/`, `components/music/radio_card.rs` |
| Nests (NIP-53) | `/nests` | `routes/nests/`, `components/nests/`, `services/nests_audio/` |
| Code Hosting (NIP-34) | `/code` | `routes/code/` (40 routes), `components/code/` (30 files), `services/git_hosting/` (17 files) |
| Marketplace (NIP-99) | `/marketplace` | `routes/shop/`, `components/shop/`, `stores/shop_store/` |
| P2P Exchange | `/mostro` | `routes/mostro/`, `components/mostro/`, `stores/mostro/` |
| Wiki (NIP-54) | `/wiki` | `routes/wiki/`, `components/wiki/`, `stores/content/wiki_store.rs` |
| Recipes | `/recipes` | `routes/recipes/`, `components/recipe/` |
| Pinboards | `/pinboards` | `routes/pin/`, `components/board/` |
| Events / Calendar (NIP-52) | `/events`, `/calendar` | `routes/events/`, `components/calendar/`, `stores/calendar_store/` |
| Photos | `/photos` | `routes/photos/`, `components/photo_card.rs` |
| Voice Messages | `/voicemessages` | `routes/voice/`, `components/voice/` |
| Polls (NIP-69) | `/polls` | `routes/polls/`, `components/poll/` |
| Games / Chess | `/games` | `routes/games/`, `components/chess/`, `stores/chess/` |
| Bible | `/bible` | `routes/bible/`, `stores/bible_store.rs`, `services/bible_api.rs` |
| Quran | `/quran` | `routes/quran/`, `stores/quran_store.rs`, `services/quran_api.rs` |
| Weather | `/weather` | `routes/weather/`, `components/weather/`, `services/weather/`, `stores/weather/` |
| Places | `/places` | `routes/places/`, `components/places/`, `stores/places_store.rs`, `services/places.rs` |
| AI Chat (DVM) | `/ai-chat` | `routes/ai_chat/`, `services/ai_chat.rs`, `services/ai_tools.rs` |
| Badges (NIP-58) | `/badges` | `routes/badges/`, `components/badge_detail_modal.rs` |
| Emoji Packs | `/packs` | `routes/packs/`, `stores/social/packs_store.rs` |
| Citations | `/citations` | `routes/citations.rs`, `stores/content/citation_store.rs` |
| Highlights (NIP-84) | `/highlights` | `routes/highlights.rs`, `components/highlight/` |
| Cashu Wallet | `/cashuwallet` | `routes/cashu_wallet.rs`, `components/cashu/`, `stores/cashu/` (37 files) |
| Blossom Media | `/blossom` | `routes/blossom/`, `stores/media/blossom_store.rs` |
| Blobbi Virtual Pet | `/blobbi` | `routes/blobbi/`, `components/blobbi/` (60+ files), `stores/blobbi/`, `hooks/blobbi/` |
| Zap Goals (NIP-75) | `/zapgoals` | `routes/zapgoals.rs`, `stores/content/zap_goals_store.rs` |
| Web Bookmarks | `/webbookmarks` | `routes/webbookmarks.rs`, `stores/webbookmarks.rs` |
| Profile | `/profile/:pubkey` | `routes/profile/` |
| Settings | `/settings` | `routes/settings/` |
| NIPs Browser | `/nips` | `routes/nips/` |
| Static Sites (NIP-5A) | (internal) | `services/pages.rs` |

## Architecture

```
src/
├── main.rs               # Entry point: feature gating, service init, Router<Route>
├── error.rs              # NostrBlueError enum
├── components/           # 450+ UI components across 114 directories
│   ├── icons.rs          # All SVG icon components
│   ├── viewers/          # 35+ content-type viewers (note, article, video, etc.)
│   ├── blobbi/           # Virtual pet subsystem (60+ files)
│   ├── cashu/            # Cashu wallet UI (20+ files, feature-gated)
│   ├── code/             # Code hosting components (30+ files)
│   ├── weather/          # Weather dashboard (20+ files)
│   ├── shop/             # Marketplace components (15+ files)
│   ├── music/            # Music player + cards
│   ├── nests/            # NIP-53 live audio room UI (9 files)
│   ├── groups/           # NIP-29 group components (10 files)
│   ├── live/             # Livestream player + chat (7 files)
│   ├── podcast/          # Podcast player + cards
│   ├── recipe/           # Recipe components (12 files)
│   ├── board/            # Pinboard/mosaic grid
│   ├── calendar/         # Calendar views + event cards
│   ├── poll/             # Poll card + creator
│   ├── topic/            # Topic/forum components
│   ├── community/        # Community cards/posts
│   ├── voice/            # Voice recorder/player
│   ├── wiki/             # Wiki page components
│   ├── places/           # Places/map UI
│   ├── rich_content/     # Rich content rendering (embeds, mentions, minicards)
│   ├── toast/            # Toast notification system
│   ├── modal.rs          # Generic modal primitive
│   ├── sheet.rs          # Bottom sheet component
│   └── ...
├── routes/
│   ├── mod.rs            # Route enum (150+ routes) + Layout component
│   ├── home/             # Home feed (following/global/relay/people list)
│   ├── code/             # 40 code hosting routes
│   ├── shop/             # 13 marketplace routes
│   ├── music/            # 13 music routes
│   ├── video/            # 10 video/live stream routes
│   └── ...
├── stores/               # Global state via GlobalSignal<T>
│   ├── auth_store.rs     # Authentication (NIP-07/46/49/55, login/logout)
│   ├── signer.rs         # Unified signer abstraction
│   ├── nostr_client/     # Core Nostr SDK client (17 files)
│   ├── relay/            # Relay management (10 files: pool, scoring, health, hints)
│   ├── profiles.rs       # Profile metadata cache (LRU 5000)
│   ├── cashu/            # Cashu ecash wallet (37 files)
│   ├── social/           # Social features: DMs, communities, groups, topics, reactions
│   ├── content/          # Content creation: drafts, articles, wiki, recipes, citations
│   ├── ui/               # UI state: theme, settings, notifications, sidebar, online status
│   ├── audio/            # Music player, podcast subscriptions, voice messages
│   ├── media/            # Blossom, NIP-96, GIF, lightbox
│   ├── publish_queue/    # Event publishing pipeline (enqueue, sign, publish, retry)
│   ├── user_prefs/       # Unified NIP-78 encrypted preference blobs (Phase 0–3 refactor)
│   ├── feed_cache.rs     # Feed caching with IndexedDB persistence
│   ├── subscription_manager.rs  # Subscription lifecycle helpers
│   ├── notification_dispatcher.rs  # Pub/sub event multiplexer
│   ├── ndb/              # Native nostrdb wrapper (desktop only)
│   └── ...
├── services/             # External APIs + service layer
│   ├── nests_audio/      # Nests audio engine
│   │   ├── mod.rs        # JS interop (web/mobile)
│   │   ├── native.rs     # Desktop native audio (moq-lite + cpal + opus)
│   │   └── android.rs    # Android foreground notification (JNI)
│   ├── git_hosting/      # Full git hosting service (17 files)
│   ├── search/           # Content/profile search, trending, query parser (7 files)
│   ├── weather/          # OpenMeteo, NWS alerts, RainViewer radar
│   ├── podcasts/         # Podcast Index API + RSS parsing
│   ├── payments/         # BTC price, LNURL, mempool.space
│   ├── cloud_backup/     # Google Drive encrypted key backup
│   ├── aggregation/      # Batch interaction counting with LRU cache
│   ├── ai_chat.rs        # AI chat (OpenAI-compatible, streaming, tool calling)
│   ├── ai_tools.rs       # Nostr-specific AI tool definitions
│   ├── bible_api.rs      # HelloAO Bible API
│   ├── quran_api.rs      # Al Quran Cloud API
│   ├── wavlake.rs        # Wavlake music API
│   ├── nip05.rs          # NIP-05 verification with TTL cache
│   ├── sync.rs           # Negentropy sync service
│   ├── social_graph.rs   # nostrarchives.com social graph
│   └── ...
├── hooks/                # Custom Dioxus hooks
│   ├── use_feed.rs       # Feed loading with pagination
│   ├── use_infinite_scroll.rs  # IntersectionObserver-based infinite scroll
│   ├── use_composer_editor.rs  # Post composer state management
│   ├── use_reaction.rs   # NIP-25 reactions with optimistic updates
│   ├── use_relay_subscription.rs  # Generic relay subscription with cleanup
│   ├── use_nostr_resource.rs  # Generic resource fetcher (loading/error state)
│   ├── use_nest_audio.rs # Nest audio orchestration
│   ├── use_community.rs  # NIP-72 community actions
│   ├── use_global_interaction.rs  # Global interaction count batching
│   ├── use_mute_block_cache.rs  # Mute/block cache with auto-invalidation
│   ├── use_unsaved_changes.rs  # Form dirty tracking with beforeunload
│   ├── blobbi/           # Blobbi pet hooks (7 files: decay, sleep, quests, etc.)
│   └── ...
├── utils/                # Helpers + NIP implementations
│   ├── nips/             # NIP parsers/builders (chess, nip34, nip36, nip39, nip48,
│   │                     #   nip49, nip52, nip53, nip54, nip58, nip5a, nip69, nip73,
│   │                     #   nip84, nip89, nip98, nip99, nip_bb)
│   ├── nkbips/           # nostr.blue extensions (nkbip03, nkbip06, nkbip08)
│   ├── parsing/          # Content parsing (markdown, asciidoc, mentions, thread tree)
│   ├── audio/            # Audio utilities (podcast, radio, v4v payment)
│   ├── recipes/          # Recipe data types and tag parsing
│   ├── format.rs         # Text formatting (bytes, relative time, sats, pubkey)
│   ├── repost.rs         # FeedItem extraction, feed processing
│   ├── route_for_kind.rs # Maps Nostr event kinds to app routes
│   └── ...
├── platform/             # Platform abstraction layer
│   ├── clipboard.rs      # Clipboard (web: navigator.clipboard, native: arboard)
│   ├── download.rs       # File downloads (web: blob URL, native: reqwest)
│   ├── geolocation.rs    # Geolocation API
│   ├── lightning.rs      # Lightning invoice handling
│   ├── mpris.rs          # Linux MPRIS media player integration
│   ├── pip.rs            # Picture-in-Picture (mobile)
│   ├── storage.rs        # LocalStorage / native filesystem
│   ├── timer.rs          # Platform-specific timers
│   ├── spawn.rs          # Async task spawning
│   ├── http.rs           # HTTP client (web: gloo, native: reqwest)
│   ├── future.rs         # Future utilities
│   ├── timestamp.rs      # Platform timestamp helpers
│   ├── android_media.rs  # Android media (mobile_platform feature)
│   ├── android_signer.rs # NIP-55 Android signer (mobile_platform feature)
│   └── mobile.rs         # Mobile-specific utilities (mobile_platform feature)
├── context/
│   └── app_context.rs    # AppContext facade + app_context!() macro
└── js/
    └── git-worker.js     # Isomorphic-git web worker source (built by esbuild)
```

### Static Assets

```
public/
├── manifest.json         # PWA manifest
├── sw.js                 # Service worker (network-first for nav, cache-first for assets)
├── sw-register.js        # SW registration + hourly update checks
├── spa-redirect.js       # SPA URL restoration from 404 redirect
├── moq-nest.js           # Nests audio via MoQ/WebTransport
├── hls-manager.js        # HLS radio streaming (lazy-loads hls.js)
├── voice-recorder.js     # Voice recording via MediaRecorder API
├── google-drive.js       # Encrypted key backup to Google Drive
├── git-worker.js         # Bundled isomorphic-git worker
├── tailwind.css          # Compiled Tailwind CSS v4 output
├── css/chessboard.css    # Chess board styles
├── icons/                # PWA icons (PNG + SVG)
├── pieces/chess/         # Chess piece SVGs (12 files)
└── docs/                 # Static docs
    ├── changelogs/       # Release changelogs (consumed by deploy.yml)
    └── pinboard.md       # Draft Pinboard NIP spec
```

## Feature Flags

Defined in `Cargo.toml` with a feature hierarchy:

| Feature | Enables | Description |
|---------|---------|-------------|
| `web` (default) | `dioxus/web`, WASM crates, `cashu` | Browser WASM build |
| `desktop` | `dioxus/desktop`, `native`, `cashu_native`, moq-lite/cpal/opus | Linux desktop |
| `mobile` | `dioxus/mobile`, `native`, `jni`, `mobile_platform` | Android WebView |
| `playstore` | Same as `mobile` but uses `dioxus/mobile` flag | Play Store AAB |
| `native` | `nostr-ndb`, `git2`, `rusqlite`, tokio MT | Shared native deps |
| `cashu` | `cdk`, `cdk-common` | Cashu wallet core |
| `cashu_native` | `cashu` + `cdk-sqlite` | Cashu with SQLite backend |

Gate code with: `#[cfg(feature = "cashu")]`, `#[cfg(feature = "native")]`, `#[cfg(feature = "mobile_platform")]`

## Key Patterns

### Components

```rust
#[component]
pub fn MyComponent(prop: String) -> Element {
    rsx! { div { class: "...", "{prop}" } }
}
```

Register in `src/components/mod.rs`:
```rust
pub mod my_component;
pub use my_component::MyComponent;
```

### State

- **Local**: `use_signal(|| value)`
- **Global**: `pub static MY_STATE: GlobalSignal<MyType> = Signal::global(|| default);`
  - Read with `MY_STATE.read()` or just `MY_STATE()` in rsx
  - Write with `MY_STATE.write().field = value`
  - Defined in `src/stores/`

### AppContext

`AppContext` in `src/context/app_context.rs` provides a facade over commonly-needed stores (auth, signer, client, profiles, bookmarks, theme). Use the `app_context!()` macro for quick access.

### Icons

All in `src/components/icons.rs`:
```rust
SearchIcon { class: "w-5 h-5".to_string() }
```

### Hooks

Custom hooks in `src/hooks/` wrap reactive state with loading/error handling:
```rust
let resource = use_nostr_resource(|| fetch_my_data(id));
// resource.state() -> NostrResourceState<T> (Initializing, AuthRequired, Loading, Loaded, Error)
```

### Publish Queue

Events are published via `stores/publish_queue/` which handles enqueue, signing, relay publishing, and retry with coalescence of replaceable events.

### Feed Caching

`stores/feed_cache.rs` + `stores/feed_cache_db.rs` provide persistent feed caching via IndexedDB with optimistic inserts, network merge, and LRU eviction (5000 items).

### Platform Abstraction

`src/platform/` provides platform-specific implementations behind a unified API. Each module uses `#[cfg(feature = "...")]` to select the appropriate backend (web JS interop vs native Rust library).

### Signer & Relay Readiness Gating

Components that fetch or subscribe **on behalf of a signed-in user** MUST gate on signer + relay readiness before issuing relay queries. Otherwise subscriptions fire against an incomplete pool (the user's NIP-65 relays are absent) and sign-requiring actions fail with "Not authenticated."

Two global signals:
- `HAS_SIGNER` (`stores/nostr_client`) — flips `true` **synchronously** when a signer attaches (`set_signer`).
- `USER_RELAYS_APPLIED` (`stores/relay`) — flips `true` only **after** the user's NIP-65 relay list is fetched + connected (a background task). This lags `HAS_SIGNER` by several network round-trips.
- `auth_store::get_pubkey()` returns `Some` **immediately** from localStorage at boot — it is **NOT** a readiness signal.

Pattern (all must hold for authenticated users; logged-out users proceed on `DEFAULT_RELAYS`):
- Call `relay::wait_for_user_relays(timeout, context)` before a fetch/subscribe. It is a no-op when `!HAS_SIGNER` and only blocks authenticated users until `USER_RELAYS_APPLIED`.
- In a `use_effect`, reactively read `HAS_SIGNER` + `USER_RELAYS_APPLIED` and early-return when `has_signer && !relays_applied`; the effect re-runs when they flip.
- For `use_relay_subscription` / `use_relay_subscription_to`, pass `None` as the filter until `!has_signer || user_relays_applied`, then `Some(filter)` — the hook (re)subscribes on the `None`→`Some` transition.
- For sign-requiring actions (NIP-98 HTTP auth, publishing, reactions), pre-check `nostr_client::get_signer().is_some()` / `*HAS_SIGNER.read()` and disable the UI control while false.

**Signer invariants** (the unified signer lives in `stores/signer.rs`; never handle raw keys directly):
- Sign operations are inherently async and may **never** resolve — external signers can be dismissed (NIP-07), remote signers time out (NIP-46), relays drop messages. Always handle timeout/dismissed at the call site; do not roll your own retry (the SDK coalesces duplicate requests).
- Never log or propagate private keys / session keys. The NIP-46 session keypair is **not** the user identity key — never surface it as "your pubkey".
- All encryption (NIP-44, NIP-04 legacy, NIP-59 gift-wrap) goes through the signer abstraction, never direct key access.

Reference implementations: `routes/home/mod.rs:850,856` (home feed), `routes/dms.rs:190-204` (DMs — canonical triple-gate), `hooks/use_lists.rs:118-157` (`use_user_lists`), `stores/ui/notifications.rs:191-236` (realtime subscription), `routes/nests/home.rs` (presence poll + fetch gate), `components/viewers/nest_viewer.rs` (room subscription gate + Join Audio signer guard), `stores/user_prefs/load.rs` (unified NIP-78 blob loader — `wait_for_user_relays` + `USER_RELAYS_APPLIED` check + quorum-EOSE fetch), `stores/auth_store.rs` (`warmup_profiles` — DB-warm-immediate + network-phase-gated follow-metadata bootstrap).

### Relay Routing & Discovery

The relay pool has several relay *categories* (by purpose) and each is added to the pool with different SDK *service flags*, which determine which APIs can reach it. Getting this wrong is the single most common source of "metadata/relay-list fetches silently return nothing" bugs.

**Relay taxonomy** (constant → purpose):

| Category | Source | Holds | Flags |
|----------|--------|-------|-------|
| Default/General | `pool::DEFAULT_RELAYS` (damus.io, nos.lol, snort.social, nostr.wine, primal.net) | General events. Bootstrap pool for logged-out users. | READ+WRITE |
| **Indexer** | `nip65::DEFAULT_INDEXER_RELAYS` (purplepag.es, coracle, user.kindpag.es, directory.yabu.me, profiles.nostr1.com, nos.social) | **kind 0 metadata + kind 10002 relay lists + kind 10050 DM relays** — profile-directory data, NOT general events | DISCOVERY-only |
| NIP-65 | user kind 10002 (`USER_RELAY_METADATA` + `RELAY_COVERAGE`) | User-defined read/write relays. **Write** relays = download events FROM the user; **read** relays = download events ABOUT the user | READ and/or WRITE |
| DM | NIP-17 kind 10050 (`DEFAULT_DM_RELAYS`) | Encrypted gift-wrapped DMs | READ |
| Search | NIP-50, kind 10007 (`DEFAULT_SEARCH_RELAYS`) | Full-text search | READ |
| Specialty | `specialty::urls` (video/GIF/radio), Mostro daemon | Specific content types / persistent GiftWrap | varies |
| Favorite/Feed (10012), Outbox (10013), Blocked (10006) | NIP-51 / private | Feed aggregation, personal storage, blocklist | varies |
| Private gift-wrapped | NIP-59 kinds 10086/10087/10089 | Indexer/proxy/trusted relay lists | n/a |

**SDK flag model** (the mechanism): `RelayServiceFlags` (`/home/patrick/nostr/crates/nostr-relay-pool/src/relay/flags.rs`) — `READ`, `WRITE`, `DISCOVERY`, `GOSSIP`, `PING`. Which APIs query which:
- `client.fetch_events()` / `subscribe()` → targets **only READ-flagged** relays (`__read_relay_urls`).
- `client.send_event()` / publish queue → targets **only WRITE-flagged** relays (`__write_relay_urls`).
- `client.fetch_events_from(urls, ...)` / `send_event_to(urls, ...)` → **flag-independent**; targets specific URLs that are pool members, gated only by `can_read()` / `can_write()`:
  - `can_read()` = READ **OR** GOSSIP **OR** DISCOVERY.
  - `can_write()` = WRITE **OR** GOSSIP (DISCOVERY excluded).

> **CRITICAL — read side:** Indexers are DISCOVERY-only, so `fetch_events()` **cannot reach them** (no READ flag). Fetching metadata / relay lists from indexers MUST go through `relay::nip65::fetch_events_from_indexers(client, filter, timeout)` — the only sanctioned entry point. Never use `fetch_events()` for indexer data; it will silently return nothing.
>
> **CRITICAL — write side (the one exception):** `can_write()` excludes DISCOVERY, so `send_event_to()` on an indexer returns `WriteDisabled`. The user's **own** discovery events (kind 0 metadata, 10002 relay list, 10050 DM relays) MUST additionally be advertised to indexers via `relay::nip65::publish_event_to_indexers(client, event)` (ephemeral connect + targeted send) so other clients can discover the user (NIP-65). This is infrequent (profile/relay edits only); do NOT give indexers a WRITE flag (causes broadcast fan-out on all subscriptions).

**Connection lifecycle:** indexer relays must be both in the pool AND connected before `fetch_events_from` works (`ensure_operational` returns `NotReady` for never-connected relays). `pool.connect()` connects **every** pool member regardless of flags. `run_post_login_init` therefore adds indexers (`add_indexer_relays_to_client`) **before** the post-login `client.connect()` so they're connected DISCOVERY members. For ad-hoc targeted fetches to relays not yet in the pool, use `relay::coverage::connect_ephemeral_relays` (temporarily adds + connects, idles out).

**Outbox model** (per-author NIP-65 routing): `relay::coverage` resolves each pubkey's write/read relays into `RELAY_COVERAGE`. For author-scoped fetches (a user's posts), prefer `nostr_client::fetch_events_aggregated_outbox` (waits for `USER_RELAYS_APPLIED`, then gossip-routes to each author's write relays). For metadata, use the indexer helper instead (indexers aggregate everyone's kind 0).

### Metadata & Profile Loading

Profile metadata (kind 0) is loaded through a batched, debounced pipeline backed by the **indexer relays** (not the general pool):

- `PROFILE_CACHE` (`stores/profiles.rs`) — LRU 5000, 24h TTL. `PROFILE_CACHE_VERSION` is bumped on every insert; consumers (`NoteCard`) re-evaluate via `use_memo` keyed on the version.
- `PROFILE_REQUEST_QUEUE` — NoteCards enqueue missing pubkeys via `queue_profile_request(pubkey)`; the app-shell `use_effect` (`routes/mod.rs`) drains it after a 200ms debounce, collapsing N per-card REQs into one batched `fetch_profiles_batch_native` call.
- `fetch_profiles_batch_native` — cache check → SDK database → **indexer relays** (`fetch_events_from_indexers`) for the still-missing authors, fetching kinds `[0, 10002, 10050]` in one chunked REQ (chunk size 200). Kind 10002 results feed `record_relay_list_from_event` to build the outbox coverage map.
- `PROFILE_EXHAUSTED` — pubkeys whose indexer fetch returned nothing are tracked (2 attempts, 5-min cooldown) so we don't hammer the indexers for pubkeys that genuinely have no kind 0, while still retrying later.
- **Login bootstrap** (`warmup_profiles` in `auth_store.rs`): Phase 1 streams DB-cached follows into `PROFILE_CACHE` immediately; Phase 2 (network) is gated on `wait_for_user_relays` then batch-fetches every followed pubkey's metadata.

**Rule:** never fetch kind 0 / 10002 / 10050 via `client.fetch_events()` — it can't reach the DISCOVERY-only indexers. Always go through `fetch_events_from_indexers`. Reference: `stores/profiles.rs` (`fetch_profiles_batch_native`, `fetch_profile_from_indexers`), `routes/mod.rs` (drain effect).

### Unified NIP-78 Preference Blobs (`stores/user_prefs/`)

User preferences are consolidated into two encrypted blobs (kind 30078):

- **`nostr.blue/prefs`** — `UserPrefsBlob` encrypted via NIP-44 to self using the **main signer** (async). Contains settings, sidebar, reactions, AI credentials, notification read-pointer, and terms flags.
- **`nostr.blue/p2p`** — `MostroPrefsBlob` encrypted via NIP-44 to self using the **Mostro identity key** (sync NIP-06 keypair). Contains Mostro settings, node config, and bounded (last-50) trade history with archival spillover at `nostr.blue/p2p/trades-archive`.

**Lifecycle** (see `stores/user_prefs/sidecar.rs`):
- Load: `user_prefs::load::load_user_prefs()` / `load_mostro_prefs()` — called from `run_post_login_init` alongside legacy per-store loaders (Phase 1 dual-read).
- Save: `user_prefs::sidecar::enqueue_main_from_signals()` / `enqueue_mostro_from_signals()` — called as a sidecar after each existing per-store save. 2s debounce (main) / 500ms (Mostro), surviving route changes via `spawn_forever_catch_unwind`.
- Live sync: `user_prefs::sidecar::start_subscriptions()` — persistent subscriptions on both d-tags for cross-device realtime sync. Self-published events are skipped via `LAST_PUBLISHED_EVENT_ID` to prevent phantom-decrypt prompts on NIP-07/46/55.
- Cleanup: `stop_subscriptions()` + `flush_all()` called from `logout()`.

**Quorum-EOSE fetch** (`stores/user_prefs/fetch.rs`): replaces the old `client.fetch_events(filter, timeout)` (which waits for ALL relays) with a manual subscribe + EOSE-count + early-exit using `feeds/realtime.rs::eose_threshold` (`max(3, 30%)`).

## Protocol documentation

**Supported specs** are tracked in the hardcoded registry at
`src/routes/nips/registry.rs` (`SUPPORTED_SPECS`), surfaced on the in-app
`/nips` page. That registry is the source of truth for which NIPs/NUTs/BUDs/
NKBIPs/Market specs nostr.blue implements. To look up the *canonical* spec text
itself (not nostr.blue's support status), use the MCP tools or the upstream
repos:

| Protocol | Source | Description |
|----------|--------|-------------|
| NIPs | `mcp__nostrbook__read_nip` / [nostr-protocol/nips](https://github.com/nostr-protocol/nips) | Nostr specs (01-100+, including hex-coded) |
| NUTs | [cashubtc/nuts](https://github.com/cashubtc/nuts) | Cashu specs (00-30) |
| BUDs | [hzrd149/blossom](https://github.com/hzrd149/blossom/tree/master/buds) | Blossom media (00-12) |
| Market | [GammaMarkets/market-spec](https://github.com/GammaMarkets/market-spec) | NIP-99 marketplace |
| NKBIPs | [nostr.blue/wiki/nkbip-XX](https://nostr.blue/wiki) | nostr.blue extensions (01-08) |

## Nostr Data Modeling

### NIP/Kind Pre-flight Checklist

Before implementing any Nostr feature, check the full list of existing NIPs/kinds (use the in-app `/nips` registry at `src/routes/nips/registry.rs`, the `mcp__nostrbook__*` tools, or the upstream repos linked above) to see what's already in use. Read the relevant NIPs thoroughly — several may apply. **Prefer extending an existing kind over creating a new one**, even if it requires minor compromises: custom kinds aren't interoperable with other clients. Only generate a new kind number if no existing kind fits after comprehensive research. Document custom kinds as a new NKBIP at `https://nostr.blue/wiki/nkbip-XX`.

### Kind Ranges

An event's kind determines its storage/replacement behavior. Use the SDK helpers (`event.kind.is_replaceable()`, `is_addressable()`) rather than hardcoding ranges:

- **Regular** (1000–9999): stored permanently (e.g. kind 1 notes).
- **Replaceable** (0, 3, 10000–19999): only the latest per `pubkey+kind` is kept; newest `created_at` wins in place.
- **Addressable** (30000–39999): keyed by `pubkey+kind+d-tag`; only the latest per combination.
- **Ephemeral** (20000–29999): may not be stored at all.

This drives the publish queue's coalescence (`publish_queue/mod.rs:25-26`: `is_replaceable() || is_addressable()`) and route dispatch (`utils/route_for_kind.rs:24`).

### Tag Design

- **Kind = schema/structure; Tags = semantics.** Don't fork a kind just to add a category — add tags.
- **Relays only index single-letter tags.** Use `t` tags for categorization (multiple `t` tags = multiple categories), not multi-letter names like `product_type`. Design tags for relay-level filtering with `#t: ["x"]`; avoid client-side filtering when the relay could do it.

### Content-Field Rules

The `content` field is for semantically-important data that does NOT need to be queried (large text, freeform content, industry-standard JSON). **Structured data generally shouldn't go in `content`** (kind 0 metadata is the historical exception). Queryable/structured data belongs in **tags** — relays only index tags. Empty `content` is valid.

### Custom-Kind Publishing

Always include a NIP-31 `alt` tag with a human-readable description on custom-kind events (so other clients can explain what they're seeing). We do this in `gif_store.rs`, `nip53.rs`, `list_encryption.rs`; formalize it for any new custom kind.

### Query Efficiency

Minimize the number of separate REQs (each consumes relay resources and may rate-limit). Combine related kinds in one `Filter` (`kinds:[1,6,16]`), use multiple `Filter`s in one `subscribe(vec![...])`, and separate event types client-side after receiving results. For kinds with strict required tags, re-validate shape client-side — relays are untrusted.

### NIP-19 Addressing

- Handle NIP-19 identifiers at the **URL root** (`/npub1…`, `/note1…`, `/naddr1…`), not nested under `/profile/` etc. See `Route::Nip19Handler` (`routes/mod.rs:539`) → `routes/nips/nip19.rs`.
- **Always decode bech32 → hex/coordinate before building a `Filter`** — filters take hex, never pass a bech32 string into `ids`/`authors`/`#d`.
- Prefer `naddr1` (embeds author pubkey) over a bare `d` tag for secure addressable-event filters.
- `note1` = event ID only (kind 1); `nevent1` = event ID + optional relay hints + author (any kind). `npub1` = pubkey only; `nprofile1` = pubkey + optional relay hints + petname.

## Styling (TailwindCSS 4)

TailwindCSS v4 with CSS-based config (`tailwind.css`). Source scanning: `@source "./src/**/*.rs"` and `@source "./index.html"`.

### Theme Colors

- Background: `bg-background`, `bg-muted`, `bg-accent`
- Text: `text-foreground`, `text-muted-foreground`
- Border: `border-border`

### Responsive

```rust
class: "lg:hidden"        // Mobile only
class: "hidden lg:block"  // Desktop only
class: "hidden xl:block"  // XL only
```

### Common Patterns

```rust
// Interactive button
class: "p-2 hover:bg-accent rounded-lg transition"

// Card
class: "bg-card border border-border rounded-lg p-4"

// Modal backdrop
class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm"
```

### Custom CSS Files

| File | Purpose |
|------|---------|
| `src/components/sheet.css` | Bottom sheet animations |
| `src/components/toast/style.css` | Toast notification styles |
| `src/components/dialog/style.css` | Modal dialog animations |
| `src/components/blobbi/blobbi.css` | Virtual pet animations |
| `public/css/chessboard.css` | Chess board grid layout |

### Custom Utilities

Defined in `tailwind.css`: `scrollbar-hide`, `min-h-dynamic-screen`, `min-h-mobile-shell`, `pt-safe-top`, `pb-safe-bottom`, `pb-safe-controls`, `pb-safe-player`, `bottom-safe-banner`.

## Nests (NIP-53)

Live audio rooms powered by MoQ (Media over QUIC). Each Nest is a NIP-53 live event with real-time audio and chat.

### Audio Engine Architecture

| Platform | Path | Transport |
|----------|------|-----------|
| Web | `mod.rs` JS interop → `public/moq-nest.js` | WebTransport (WASM) |
| Android | `mod.rs` JS interop → WebView + `android.rs` JNI notification | WebTransport (WebView) |
| Desktop | `native.rs` → moq-lite + cpal + opus | WebTransport (native) |

## Testing

Tests are inline `#[cfg(test)]` blocks within source files (104 blocks total). Run with:

```bash
cargo test                                    # Default (web feature)
cargo test --no-default-features --features desktop  # As run in CI
```

Test coverage includes: Cashu wallet operations, NIP implementations, content parsing, utilities, services, stores, components, and route handlers.
