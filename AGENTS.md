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
| Groups (NIP-28) | `/groups` | `stores/social/group_store/`, `components/groups/` |
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
| P2P Exchange | `/p2p` | `routes/p2p/`, `components/p2p/`, `stores/social/p2p_store.rs` |
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
│   ├── groups/           # NIP-28 group components (10 files)
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
└── docs/                 # Protocol documentation (git submodules)
    ├── nips/             # NIP specs (01-100+ including hex-coded)
    ├── nuts/             # Cashu NUT specs (00-30)
    ├── blossom/buds/     # Blossom BUD specs (00-12)
    ├── market-spec/      # NIP-99 marketplace spec
    ├── NKBIPs/           # nostr.blue extensions (01-08)
    ├── changelogs/       # Release changelogs
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

## Protocol Documentation

**Read local docs FIRST** before MCP tools or web search:

| Protocol | Location | Description |
|----------|----------|-------------|
| NIPs | `public/docs/nips/` | Nostr specs (01-100+, including hex-coded) |
| NUTs | `public/docs/nuts/` | Cashu specs (00-30) |
| BUDs | `public/docs/blossom/buds/` | Blossom media (00-12) |
| Market | `public/docs/market-spec/` | NIP-99 marketplace |
| NKBIPs | `public/docs/NKBIPs/` | nostr.blue extensions (01-08) |

MCP tool `mcp__nostrbook__read_nip` available for quick NIP lookups.

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
