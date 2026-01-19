# nostr.blue

A nostr client built using **Rust + Dioxus + rust-nostr** with integrated CDK wallet.

![Version](https://img.shields.io/badge/version-0.7.5-blue)
![License](https://img.shields.io/badge/license-MIT-green)
![Rust](https://img.shields.io/badge/rust-1.77+-orange)
![CDK](https://img.shields.io/badge/CDK-0.14.2-purple)

## 🌟 Overview

nostr.blue is a modern Nostr client built entirely in Rust and compiled to WebAssembly. It provides a comprehensive social networking experience on the Nostr protocol with advanced features like communities, Lightning zaps, encrypted messaging, and Data Vending Machines.

## ⚡ Nostr Features

- **Real-time Social Feeds** - Smart relay routing using the outbox model (NIP-65) for reliable content discovery
- **Encrypted Messaging** - Full DM support with NIP-04 (legacy), NIP-17 (private), and NIP-44 (versioned encryption)
- **Lightning Zaps** - Send and receive Bitcoin micropayments (NIP-57) with NWC integration (NIP-47)
- **Rich Media** - Polls (NIP-88), Livestreaming (NIP-53), Voice Messages (NIP-A0), Podcasts
- **Blossom Media Management** - Upload, delete, and mirror media across Blossom servers (BUD-01/02/04)
- **Long-form Content** - Articles (NIP-23) with encrypted drafts (NIP-37), Photos (NIP-68), Videos (NIP-71)
- **Wiki & Publications** - NIP-54 wiki pages with wikilinks, NKBIP-01 curated publications with AsciiDoc
- **External Content** - NIP-73 support for books (ISBN), papers (DOI), Bitcoin transactions/addresses
- **P2P Trading** - View NIP-69 peer-to-peer Bitcoin orders with depth charts and market data
- **Marketplace** - NIP-99 classified listings with product browse, cart, and Cashu/Lightning checkout
- **Code Collaboration** - Git repositories (NIP-34), code snippets (NIP-C0), issues, and pull requests
- **Social Organization** - Communities (NIP-72), Lists (NIP-51), Data Vending Machines (NIP-90)
- **Secure Authentication** - Browser extension (NIP-07) and remote signer (NIP-46) with Amber/nsecBunker
- **Cross-device Sync** - Settings synchronized across devices via Nostr (NIP-78)

## 💰 Cashu Features

- **Multi-mint Ecash Wallet** - Bitcoin ecash with NIP-60 integration for encrypted token storage
- **Lightning Integration** - Deposits (NUT-04) and withdrawals (NUT-05) via Lightning Network
- **P2PK Token Locking** - Send ecash locked to npub recipients (NUT-11)
- **Real-time Updates** - WebSocket subscriptions for instant quote status (NUT-17)
- **Protected Mints** - Full authentication support for private mints (NUT-21/22)
- **Deterministic Recovery** - Seed derived from Nostr keys survives app reinstall
- **Mint Discovery** - Find trusted mints via community recommendations (NIP-87)
- **Security Features** - Reserved proof protection, URL normalization, keyset ID validation

## 🛠 Technology Stack

### Core Framework
- **[Dioxus 0.7.2](https://dioxuslabs.com/)** - Modern reactive web framework for Rust
- **dioxus-stores** - Advanced state management library for reactive global state
- **WebAssembly** - Compiled to WASM for near-native browser performance
- **[Dioxus CLI](https://dioxuslabs.com/learn/0.7/CLI)** - Development server and WASM bundler

### Nostr Protocol
- **[rust-nostr SDK](https://rust-nostr.org/)** - Comprehensive Nostr implementation
  - `nostr-sdk` - High-level client with relay pool management
  - `nostr` - Core protocol types and event handling
  - `nostr-database` - Database abstraction layer
  - `nostr-indexeddb` - IndexedDB persistent storage
  - `nostr-browser-signer` - NIP-07 browser extension integration
  - `nostr-connect` - NIP-46 remote signer protocol (Amber, nsecBunker)
  - `nwc` - NIP-47 Nostr Wallet Connect for remote Lightning wallet integration

### Cashu Protocol
- **[CDK](https://github.com/cashubtc/cdk)** - Cashu Development Kit for ecash wallet functionality
  - `cdk` - Core Cashu wallet implementation with mint/melt operations, quote management, and proof handling (with `auth` feature for NUT-21/22 protected mints)
  - `cdk-common` - Common types, database traits, and utilities for Cashu protocol
  - Custom IndexedDB implementation of `WalletDatabase` trait for browser persistence
  - Atomic keyset counter management prevents "Blinded Message already signed" errors

### Styling & UI
- **[TailwindCSS 3](https://tailwindcss.com/)** - Utility-first CSS framework
- Custom icon components with SVG optimization

### Additional Libraries
- **serde** - Serialization/deserialization
- **chrono** - Date and time handling
- **pulldown-cmark** - Markdown parsing
- **ammonia** - HTML sanitization
- **reqwest** - HTTP client for LNURL and external services
- **gloo-storage** - LocalStorage API wrapper
- **tokio** - Async runtime for parallel operations

## 📦 Project Structure

```
nostr.blue/
├── src/
│   ├── components/          # Reusable UI components
│   │   ├── note.rs         # Note/event display
│   │   ├── note_card.rs    # Compact note card
│   │   ├── note_composer.rs # Post creation
│   │   ├── reply_composer.rs # Reply creation (NIP-10)
│   │   ├── comment_composer.rs # Comment composer (NIP-22)
│   │   ├── media_uploader.rs # Blossom media upload
│   │   ├── emoji_picker.rs # Enhanced emoji picker with custom emojis
│   │   ├── profile_card.rs # User profile display
│   │   ├── photo_card.rs   # Photo grid item (NIP-68)
│   │   ├── article_card.rs # Long-form article card
│   │   ├── voice_message_card.rs # Voice message card (NIP-A0)
│   │   ├── poll_card.rs    # Poll display with voting (NIP-88)
│   │   ├── poll_timer.rs   # Poll countdown timer (NIP-88)
│   │   ├── poll_option_list.rs # Poll option editor (NIP-88)
│   │   ├── webbookmark_card.rs # Web bookmark card (NIP-B0)
│   │   ├── webbookmark_modal.rs # Add/edit bookmark modal (NIP-B0)
│   │   ├── zap_modal.rs    # Lightning zap interface
│   │   ├── share_modal.rs  # Video sharing modal
│   │   ├── live_stream_card.rs # Livestream card (NIP-53)
│   │   ├── mini_live_stream_card.rs # Compact livestream card (NIP-53)
│   │   ├── live_stream_player.rs # HLS video player for livestreams
│   │   ├── live_chat.rs    # Livestream chat component (NIP-53)
│   │   ├── rich_content.rs # Content rendering (Wavlake embeds)
│   │   ├── threaded_comment.rs # Comment threads
│   │   ├── music_player.rs # Unified audio player (music + podcasts)
│   │   ├── track_card.rs   # Music track display
│   │   ├── wavlake_zap_dialog.rs # Music artist zaps
│   │   ├── podcast_show_card.rs # Podcast show card
│   │   ├── podcast_episode_card.rs # Podcast episode display
│   │   ├── podcast_chapters.rs # JSON chapters with seek-on-click
│   │   ├── external_content_card.rs # NIP-73 external content display
│   │   ├── p2p_order_card.rs # P2P order list card (NIP-69)
│   │   ├── p2p_order_filters.rs # P2P order filter panel
│   │   ├── p2p_depth_chart.rs # P2P market depth visualization
│   │   ├── code_repo_card.rs # Git repository card (NIP-34)
│   │   ├── code_snippet_card.rs # Code snippet card (NIP-C0)
│   │   ├── wiki_card.rs    # Wiki page card (NIP-54)
│   │   ├── wiki_content.rs # Wiki content renderer (NIP-54)
│   │   ├── wiki_backlinks.rs # Wiki backlinks (NIP-54)
│   │   ├── publication_card.rs # Publication card (NKBIP-01)
│   │   ├── publication_toc.rs # Publication table of contents (NKBIP-01)
│   │   ├── publication_section.rs # Publication section renderer (NKBIP-01)
│   │   ├── asciidoc_content.rs # AsciiDoc content renderer
│   │   ├── wallet_balance_card.rs # Cashu wallet balance display
│   │   ├── token_list.rs   # Cashu token list by mint
│   │   ├── transaction_history.rs # Cashu transaction history
│   │   ├── cashu_setup_wizard.rs # Cashu wallet setup flow
│   │   ├── cashu_send_modal.rs # Send ecash modal
│   │   ├── cashu_receive_modal.rs # Receive ecash modal
│   │   ├── cashu_receive_lightning_modal.rs # Lightning deposit modal
│   │   ├── cashu_send_lightning_modal.rs # Lightning withdrawal modal
│   │   ├── nwc_setup_modal.rs # Nostr Wallet Connect setup (NIP-47)
│   │   ├── sidebar.rs      # Navigation sidebar
│   │   ├── layout.rs       # App shell layout
│   │   ├── client_initializing.rs # Loading animation
│   │   └── icons.rs        # SVG icon components
│   ├── routes/             # Page routes
│   │   ├── home.rs         # Home feed
│   │   ├── profile.rs      # User profiles
│   │   ├── note.rs         # Single note view with threading
│   │   ├── article_detail.rs # Article view with NIP-22 comments
│   │   ├── video_detail.rs # Video view with NIP-22 comments
│   │   ├── photo_detail.rs # Photo detail view with NIP-22 comments
│   │   ├── photos.rs       # Photo feed (NIP-68)
│   │   ├── videos.rs       # Video feed (NIP-71)
│   │   ├── videos_live.rs  # Livestream feed (NIP-53)
│   │   ├── videos_live_tag.rs # Tagged livestream feed (NIP-53)
│   │   ├── live_stream_detail.rs # Livestream detail page (NIP-53)
│   │   ├── live_stream_new.rs # Create new livestream (NIP-53)
│   │   ├── voicemessages.rs # Voice messages feed (NIP-A0)
│   │   ├── polls.rs        # Polls feed (NIP-88)
│   │   ├── poll_view.rs    # Individual poll view (NIP-88)
│   │   ├── poll_new.rs     # Poll creation form (NIP-88)
│   │   ├── webbookmarks.rs # Web bookmarks manager (NIP-B0)
│   │   ├── blossom/        # Blossom media management
│   │   │   └── blossom_home.rs # Media library with upload, delete, mirror (BUD-01/02/04)
│   │   ├── cashu_wallet.rs # Cashu ecash wallet (NIP-60)
│   │   ├── communities.rs  # Communities (NIP-72)
│   │   ├── lists.rs        # User lists (NIP-51)
│   │   ├── dms.rs          # Direct messages (NIP-04/17/44)
│   │   ├── notifications.rs # Notifications
│   │   ├── settings.rs     # User settings (NIP-78 sync)
│   │   ├── trending.rs     # Trending content
│   │   ├── explore.rs      # Discover feed
│   │   ├── dvm.rs          # Data Vending Machines (NIP-90)
│   │   ├── search.rs       # Search interface
│   │   ├── hashtag.rs      # Hashtag feed
│   │   ├── music/          # Music routes
│   │   │   ├── music_home.rs # Music discovery
│   │   │   ├── artist.rs   # Artist pages
│   │   │   ├── album.rs    # Album pages
│   │   │   ├── radio.rs    # Wavlake radio
│   │   │   └── leaderboard.rs # Music leaderboard
│   │   ├── podcast.rs      # Podcast home (discovery, search)
│   │   ├── podcast_nostr_detail.rs # Nostr podcast show page
│   │   ├── podcast_rss_detail.rs # RSS podcast show page
│   │   ├── podcast_episode_detail.rs # Podcast episode detail
│   │   ├── p2p.rs          # P2P trading order book (NIP-69)
│   │   ├── p2p_order_detail.rs # P2P order detail view
│   │   ├── code.rs         # Code collaboration home (NIP-34)
│   │   ├── code_repo.rs    # Repository detail page
│   │   ├── code_snippets.rs # Code snippets list (NIP-C0)
│   │   ├── wiki.rs         # Wiki home (NIP-54)
│   │   ├── wiki_detail.rs  # Wiki page detail (NIP-54)
│   │   ├── wiki_new.rs     # Create/edit wiki page (NIP-54)
│   │   ├── publications.rs # Publications home (NKBIP-01)
│   │   ├── publication_detail.rs # Publication viewer (NKBIP-01)
│   │   ├── publication_new.rs # Create publication (NKBIP-01)
│   │   ├── terms.rs        # Terms of Service
│   │   ├── privacy.rs      # Privacy Policy
│   │   ├── cookies.rs      # Cookie Policy
│   │   └── about.rs        # About page
│   ├── hooks/              # Custom reactive hooks
│   │   ├── use_auth.rs     # Authentication state
│   │   ├── use_profile.rs  # Profile data fetching
│   │   ├── use_feed.rs     # Feed management
│   │   ├── use_lists.rs    # List management
│   │   └── use_infinite_scroll.rs # Pagination
│   ├── stores/             # Global state management
│   │   ├── nostr_client.rs # Nostr SDK client with IndexedDB
│   │   ├── auth_store.rs   # Authentication state (NIP-07)
│   │   ├── profiles.rs     # Profile cache with batch fetching
│   │   ├── bookmarks.rs    # Bookmarked content (NIP-51)
│   │   ├── dms.rs          # DM conversations with NIP-17 compliance
│   │   ├── notifications.rs # Notification state with real-time
│   │   ├── music_player.rs # Music player state with NIP-38 status
│   │   ├── settings_store.rs # NIP-78 synced settings
│   │   ├── theme_store.rs  # Theme preferences
│   │   ├── blossom_store.rs # Blossom media storage (BUD-01)
│   │   ├── voice_messages_store.rs # Voice message playback state
│   │   ├── webbookmarks.rs # Web bookmarks store (NIP-B0)
│   │   ├── emoji_store.rs  # Custom emoji management (NIP-30/NIP-51)
│   │   ├── cashu_wallet.rs # Cashu wallet state and operations (NIP-60)
│   │   ├── indexeddb_database.rs # IndexedDB persistent storage for CDK wallet
│   │   ├── nwc_store.rs    # Nostr Wallet Connect state and operations (NIP-47)
│   │   ├── p2p_store.rs    # P2P order cache and filtering (NIP-69)
│   │   ├── shop_store.rs   # Marketplace state (NIP-99)
│   │   ├── code_store.rs   # Git repository state (NIP-34)
│   │   ├── wiki_store.rs   # Wiki state (NIP-54)
│   │   ├── publication_store.rs # Publication state (NKBIP-01)
│   │   ├── embedding_store.rs # Embedding state (NKBIP-02)
│   │   ├── citation_store.rs # Citation state (NKBIP-03)
│   │   ├── directory_store.rs # Directory state (NKBIP-04)
│   │   └── signer.rs       # Event signing
│   ├── utils/              # Utility functions
│   │   ├── nip19.rs        # NIP-19 identifier parsing
│   │   ├── content_parser.rs # Content extraction
│   │   ├── markdown.rs     # Markdown rendering
│   │   ├── time.rs         # Time formatting
│   │   ├── validation.rs   # Input validation
│   │   ├── list_kinds.rs   # NIP-51 list types
│   │   ├── thread_tree.rs  # Reply threading
│   │   ├── article_meta.rs # Article metadata
│   │   ├── url_metadata.rs # URL metadata fetching (Open Graph, Twitter Cards)
│   │   ├── repost.rs       # Repost handling and FeedItem enum
│   │   ├── profile_prefetch.rs # Batch profile metadata prefetching
│   │   ├── podcast.rs      # Podcast event parsing (Kind 30054/30055)
│   │   ├── nip69.rs        # NIP-69 P2P order parsing
│   │   ├── nip99.rs        # NIP-99 marketplace parsing
│   │   ├── nip34.rs        # NIP-34 Git repository helpers
│   │   ├── nip73.rs        # NIP-73 external content helpers
│   │   ├── nip54.rs        # NIP-54 wiki d-tag normalization and wikilinks
│   │   ├── asciidoc.rs     # AsciiDoc to HTML rendering
│   │   ├── nkbip03.rs      # NKBIP-03 citation parsing
│   │   ├── nkbip06.rs      # NKBIP-06 MIME type tags
│   │   └── nkbip08.rs      # NKBIP-08 book wikilinks
│   ├── services/           # External services
│   │   ├── lnurl.rs        # Lightning URL handling
│   │   ├── wavlake.rs      # Wavlake API integration
│   │   ├── trending.rs     # Trending algorithm
│   │   ├── podcast_rss.rs  # Podcasting 2.0 RSS parsing
│   │   ├── podcast_index.rs # Podcast Index API client
│   │   ├── mempool.rs      # Mempool.space Bitcoin API
│   │   ├── openlibrary.rs  # OpenLibrary book covers
│   │   ├── btc_price.rs    # Binance BTC price API for P2P sats calculations
│   │   └── git_hosting/    # NIP-34 Git hosting services
│   └── main.rs             # Application entry point
├── assets/                 # Static assets
│   ├── favicon.svg         # SVG favicon
│   ├── favicon.ico         # ICO favicon
│   └── tailwind.css        # Compiled CSS
├── public/                 # Public build output
├── dist/                   # Production build
├── Cargo.toml              # Rust dependencies
├── Dioxus.toml             # Dioxus configuration
├── tailwind.config.js      # TailwindCSS configuration
├── package.json            # Node.js dependencies
└── index.html              # HTML template
```

## 🚦 Getting Started

### Prerequisites

- **Rust 1.77+** (install via [rustup](https://rustup.rs/))
- **Node.js 18+** and **npm** (for TailwindCSS)
- **Dioxus CLI** (development server and bundler)
- **wasm32-unknown-unknown** target

### Installation

```bash
# Clone the repository
git clone https://github.com/patrickulrich/nostr.blue.git
cd nostr.blue

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WASM target
rustup target add wasm32-unknown-unknown

# Install Dioxus CLI (0.7.x required)
cargo install dioxus-cli@0.7

# Install Node dependencies
npm install

# Build TailwindCSS
npm run tailwind:build
```

### Development

```bash
# Terminal 1: Watch and rebuild CSS
npm run tailwind:watch

# Terminal 2: Run development server
dx serve

# Visit http://localhost:8080
```

The development server includes:
- Hot reload on Rust code changes
- Auto-rebuild on file modifications
- Source maps for debugging

### Building for Production

```bash
# Build optimized CSS
npm run tailwind:build

# Build optimized WASM bundle
dx build --release

# Output files in dist/
```

Production builds are optimized with:
- Link-time optimization (LTO)
- Size optimization (`opt-level = "z"`)
- Single codegen unit for minimal binary size
- Panic abort for smaller WASM binaries

## 🔌 Protocol Support

### Nostr

| NIP | Description | Status |
|-----|-------------|--------|
| [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) | Basic protocol | ✅ |
| [NIP-02](https://github.com/nostr-protocol/nips/blob/master/02.md) | Follow List | ✅ |
| [NIP-03](https://github.com/nostr-protocol/nips/blob/master/03.md) | OpenTimestamps Attestations | ❌ |
| [NIP-04](https://github.com/nostr-protocol/nips/blob/master/04.md) | Encrypted DM (legacy) | ✅ |
| [NIP-05](https://github.com/nostr-protocol/nips/blob/master/05.md) | DNS Identifiers | ✅ |
| [NIP-06](https://github.com/nostr-protocol/nips/blob/master/06.md) | Key derivation from mnemonic | ❌ |
| [NIP-07](https://github.com/nostr-protocol/nips/blob/master/07.md) | Browser extension signing | ✅ |
| [NIP-09](https://github.com/nostr-protocol/nips/blob/master/09.md) | Event Deletion | ✅ |
| [NIP-10](https://github.com/nostr-protocol/nips/blob/master/10.md) | Text Notes and Threads | ✅ |
| [NIP-11](https://github.com/nostr-protocol/nips/blob/master/11.md) | Relay Information Document | ❌ |
| [NIP-13](https://github.com/nostr-protocol/nips/blob/master/13.md) | Proof of Work | ❌ |
| [NIP-14](https://github.com/nostr-protocol/nips/blob/master/14.md) | Subject tag | ❌ |
| [NIP-15](https://github.com/nostr-protocol/nips/blob/master/15.md) | Nostr Marketplace | ❌ |
| [NIP-17](https://github.com/nostr-protocol/nips/blob/master/17.md) | Private Direct Messages | ✅ |
| [NIP-18](https://github.com/nostr-protocol/nips/blob/master/18.md) | Reposts | ✅ |
| [NIP-19](https://github.com/nostr-protocol/nips/blob/master/19.md) | bech32 identifiers | ✅ |
| [NIP-21](https://github.com/nostr-protocol/nips/blob/master/21.md) | nostr: URI scheme | ✅ |
| [NIP-22](https://github.com/nostr-protocol/nips/blob/master/22.md) | Comments | ✅ |
| [NIP-23](https://github.com/nostr-protocol/nips/blob/master/23.md) | Long-form Content | ✅ |
| [NIP-24](https://github.com/nostr-protocol/nips/blob/master/24.md) | Extra metadata fields | ✅ |
| [NIP-25](https://github.com/nostr-protocol/nips/blob/master/25.md) | Reactions | ✅ |
| [NIP-27](https://github.com/nostr-protocol/nips/blob/master/27.md) | Text Note References | ✅ |
| [NIP-28](https://github.com/nostr-protocol/nips/blob/master/28.md) | Public Chat | ❌ |
| [NIP-29](https://github.com/nostr-protocol/nips/blob/master/29.md) | Relay-based Groups | ❌ |
| [NIP-30](https://github.com/nostr-protocol/nips/blob/master/30.md) | Custom Emoji | ✅ |
| [NIP-31](https://github.com/nostr-protocol/nips/blob/master/31.md) | Unknown Events | ❌ |
| [NIP-32](https://github.com/nostr-protocol/nips/blob/master/32.md) | Labeling | ❌ |
| [NIP-34](https://github.com/nostr-protocol/nips/blob/master/34.md) | Git stuff | ✅ |
| [NIP-35](https://github.com/nostr-protocol/nips/blob/master/35.md) | Torrents | ❌ |
| [NIP-36](https://github.com/nostr-protocol/nips/blob/master/36.md) | Sensitive Content | ❌ |
| [NIP-37](https://github.com/nostr-protocol/nips/blob/master/37.md) | Draft Events | ✅ |
| [NIP-38](https://github.com/nostr-protocol/nips/blob/master/38.md) | User Statuses | ✅ |
| [NIP-39](https://github.com/nostr-protocol/nips/blob/master/39.md) | External Identities | ❌ |
| [NIP-40](https://github.com/nostr-protocol/nips/blob/master/40.md) | Expiration Timestamp | ❌ |
| [NIP-42](https://github.com/nostr-protocol/nips/blob/master/42.md) | Client Auth to Relays | ✅ |
| [NIP-43](https://github.com/nostr-protocol/nips/blob/master/43.md) | Relay Access Metadata | ❌ |
| [NIP-44](https://github.com/nostr-protocol/nips/blob/master/44.md) | Encrypted Payloads | ✅ |
| [NIP-45](https://github.com/nostr-protocol/nips/blob/master/45.md) | Counting results | ❌ |
| [NIP-46](https://github.com/nostr-protocol/nips/blob/master/46.md) | Remote Signing | ✅ |
| [NIP-47](https://github.com/nostr-protocol/nips/blob/master/47.md) | Wallet Connect | ✅ |
| [NIP-48](https://github.com/nostr-protocol/nips/blob/master/48.md) | Proxy Tags | ✅ |
| [NIP-49](https://github.com/nostr-protocol/nips/blob/master/49.md) | Private Key Encryption | ✅ |
| [NIP-50](https://github.com/nostr-protocol/nips/blob/master/50.md) | Search Capability | ✅ |
| [NIP-51](https://github.com/nostr-protocol/nips/blob/master/51.md) | Lists | ✅ |
| [NIP-52](https://github.com/nostr-protocol/nips/blob/master/52.md) | Calendar Events | ✅ |
| [NIP-53](https://github.com/nostr-protocol/nips/blob/master/53.md) | Live Activities | ✅ |
| [NIP-54](https://github.com/nostr-protocol/nips/blob/master/54.md) | Wiki | ✅ |
| [NIP-56](https://github.com/nostr-protocol/nips/blob/master/56.md) | Reporting | ✅ |
| [NIP-57](https://github.com/nostr-protocol/nips/blob/master/57.md) | Lightning Zaps | ✅ |
| [NIP-58](https://github.com/nostr-protocol/nips/blob/master/58.md) | Badges | ✅ |
| [NIP-59](https://github.com/nostr-protocol/nips/blob/master/59.md) | Gift Wrap | ✅ |
| [NIP-60](https://github.com/nostr-protocol/nips/blob/master/60.md) | Cashu Wallet | ✅ |
| [NIP-61](https://github.com/nostr-protocol/nips/blob/master/61.md) | Nutzaps | ❌ |
| [NIP-62](https://github.com/nostr-protocol/nips/blob/master/62.md) | Request to Vanish | ❌ |
| [NIP-64](https://github.com/nostr-protocol/nips/blob/master/64.md) | Chess (PGN) | ❌ |
| [NIP-65](https://github.com/nostr-protocol/nips/blob/master/65.md) | Relay List Metadata | ✅ |
| [NIP-66](https://github.com/nostr-protocol/nips/blob/master/66.md) | Relay Discovery | ❌ |
| [NIP-68](https://github.com/nostr-protocol/nips/blob/master/68.md) | Picture-first feeds | ✅ |
| [NIP-69](https://github.com/nostr-protocol/nips/blob/master/69.md) | P2P Order events | ✅ |
| [NIP-70](https://github.com/nostr-protocol/nips/blob/master/70.md) | Protected Events | ❌ |
| [NIP-71](https://github.com/nostr-protocol/nips/blob/master/71.md) | Video Events | ✅ |
| [NIP-72](https://github.com/nostr-protocol/nips/blob/master/72.md) | Moderated Communities | ✅ |
| [NIP-73](https://github.com/nostr-protocol/nips/blob/master/73.md) | External Content IDs | ✅ |
| [NIP-75](https://github.com/nostr-protocol/nips/blob/master/75.md) | Zap Goals | ❌ |
| [NIP-77](https://github.com/nostr-protocol/nips/blob/master/77.md) | Negentropy Syncing | ❌ |
| [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md) | App-specific data | ✅ |
| [NIP-7D](https://github.com/nostr-protocol/nips/blob/master/7D.md) | Threads | ❌ |
| [NIP-84](https://github.com/nostr-protocol/nips/blob/master/84.md) | Highlights | ❌ |
| [NIP-86](https://github.com/nostr-protocol/nips/blob/master/86.md) | Relay Management API | ❌ |
| [NIP-87](https://github.com/nostr-protocol/nips/blob/master/87.md) | Mint Discoverability | ✅ |
| [NIP-88](https://github.com/nostr-protocol/nips/blob/master/88.md) | Polls | ✅ |
| [NIP-89](https://github.com/nostr-protocol/nips/blob/master/89.md) | App Handlers | ❌ |
| [NIP-90](https://github.com/nostr-protocol/nips/blob/master/90.md) | Data Vending Machines | ✅ |
| [NIP-92](https://github.com/nostr-protocol/nips/blob/master/92.md) | Media Attachments | ✅ |
| [NIP-94](https://github.com/nostr-protocol/nips/blob/master/94.md) | File Metadata | ✅ |
| [NIP-96](https://github.com/nostr-protocol/nips/blob/master/96.md) | HTTP File Storage | ✅ |
| [NIP-98](https://github.com/nostr-protocol/nips/blob/master/98.md) | HTTP Auth | ✅ |
| [NIP-99](https://github.com/nostr-protocol/nips/blob/master/99.md) | Classified Listings | ✅ |
| [NIP-A0](https://github.com/nostr-protocol/nips/blob/master/A0.md) | Voice Messages | ✅ |
| [NIP-B0](https://github.com/nostr-protocol/nips/blob/master/B0.md) | Web Bookmarks | ✅ |
| [NIP-B7](https://github.com/nostr-protocol/nips/blob/master/B7.md) | Blossom | ✅ |
| [NIP-BE](https://github.com/nostr-protocol/nips/blob/master/BE.md) | BLE Communications | ❌ |
| [NIP-C0](https://github.com/nostr-protocol/nips/blob/master/C0.md) | Code Snippets | ✅ |
| [NIP-C7](https://github.com/nostr-protocol/nips/blob/master/C7.md) | Chats | ❌ |
| [NIP-EE](https://github.com/nostr-protocol/nips/blob/master/EE.md) | E2EE MLS Protocol | ❌ |

### Blossom

| BUD | Description | Status |
|-----|-------------|--------|
| [BUD-01](https://github.com/hzrd149/blossom/blob/master/buds/01.md) | Server requirements | ✅ |
| [BUD-02](https://github.com/hzrd149/blossom/blob/master/buds/02.md) | Blob upload/management | ✅ |
| [BUD-03](https://github.com/hzrd149/blossom/blob/master/buds/03.md) | User Server List | ✅ |
| [BUD-04](https://github.com/hzrd149/blossom/blob/master/buds/04.md) | Mirroring blobs | ✅ |
| [BUD-05](https://github.com/hzrd149/blossom/blob/master/buds/05.md) | Media optimization | ❌ |
| [BUD-06](https://github.com/hzrd149/blossom/blob/master/buds/06.md) | Upload requirements | ❌ |
| [BUD-07](https://github.com/hzrd149/blossom/blob/master/buds/07.md) | Payment required | ❌ |
| [BUD-08](https://github.com/hzrd149/blossom/blob/master/buds/08.md) | File Metadata Tags | ❌ |
| [BUD-09](https://github.com/hzrd149/blossom/blob/master/buds/09.md) | Blob Report | ❌ |
| [BUD-10](https://github.com/hzrd149/blossom/blob/master/buds/10.md) | Blossom URI Schema | ❌ |

### Cashu

The wallet is built on [CDK (Cashu Development Kit)](https://github.com/cashubtc/cdk) with a custom IndexedDB storage backend for browser persistence.

| NUT | Description | Status |
|-----|-------------|--------|
| [NUT-00](https://github.com/cashubtc/nuts/blob/main/00.md) | Notation and Encoding | ✅ |
| [NUT-01](https://github.com/cashubtc/nuts/blob/main/01.md) | Mint public keys | ✅ |
| [NUT-02](https://github.com/cashubtc/nuts/blob/main/02.md) | Keysets and fees | ✅ |
| [NUT-03](https://github.com/cashubtc/nuts/blob/main/03.md) | Swapping tokens | ✅ |
| [NUT-04](https://github.com/cashubtc/nuts/blob/main/04.md) | Minting tokens | ✅ |
| [NUT-05](https://github.com/cashubtc/nuts/blob/main/05.md) | Melting tokens | ✅ |
| [NUT-06](https://github.com/cashubtc/nuts/blob/main/06.md) | Mint info | ✅ |
| [NUT-07](https://github.com/cashubtc/nuts/blob/main/07.md) | Token state check | ✅ |
| [NUT-08](https://github.com/cashubtc/nuts/blob/main/08.md) | Overpaid fees | ✅ |
| [NUT-09](https://github.com/cashubtc/nuts/blob/main/09.md) | Signature restore | ✅ |
| [NUT-10](https://github.com/cashubtc/nuts/blob/main/10.md) | Spending conditions | ✅ |
| [NUT-11](https://github.com/cashubtc/nuts/blob/main/11.md) | P2PK | ✅ |
| [NUT-12](https://github.com/cashubtc/nuts/blob/main/12.md) | DLEQ proofs | ✅ |
| [NUT-13](https://github.com/cashubtc/nuts/blob/main/13.md) | Deterministic secrets | ✅ |
| [NUT-14](https://github.com/cashubtc/nuts/blob/main/14.md) | HTLCs | ✅ |
| [NUT-15](https://github.com/cashubtc/nuts/blob/main/15.md) | Multi-path payments | ✅ |
| [NUT-16](https://github.com/cashubtc/nuts/blob/main/16.md) | Animated QR codes | ❌ |
| [NUT-17](https://github.com/cashubtc/nuts/blob/main/17.md) | WebSocket subscriptions | ✅ |
| [NUT-18](https://github.com/cashubtc/nuts/blob/main/18.md) | Payment requests | ✅ |
| [NUT-19](https://github.com/cashubtc/nuts/blob/main/19.md) | Cached responses | ✅ |
| [NUT-20](https://github.com/cashubtc/nuts/blob/main/20.md) | Signature on mint quote | ✅ |
| [NUT-21](https://github.com/cashubtc/nuts/blob/main/21.md) | Clear authentication | ✅ |
| [NUT-22](https://github.com/cashubtc/nuts/blob/main/22.md) | Blind authentication | ✅ |
| [NUT-23](https://github.com/cashubtc/nuts/blob/main/23.md) | Payment Method: BOLT11 | ❌ |
| [NUT-24](https://github.com/cashubtc/nuts/blob/main/24.md) | HTTP 402 Payment Required | ❌ |
| [NUT-25](https://github.com/cashubtc/nuts/blob/main/25.md) | Payment Method: BOLT12 | ❌ |

### NKBIP (Nostr Knowledge Base Interoperability Proposals)

These specifications extend Nostr for publishing, citations, and knowledge management. Based on [gc-alexandria](https://github.com/limina1/gc-alexandria) implementation.

| NKBIP | Description | Kinds | Status |
|-------|-------------|-------|--------|
| [NKBIP-01](https://nostr.blue/wiki/nkbip-01) | Curated Publications | 30040, 30041 | ✅ |
| [NKBIP-02](https://nostr.blue/wiki/nkbip-02) | Vector Embeddings | 1987 | ✅ |
| [NKBIP-03](https://nostr.blue/wiki/nkbip-03) | Citations | 30, 31, 32, 33 | ✅ |
| [NKBIP-04](https://nostr.blue/wiki/nkbip-04) | Directory System | 30042-30045 | ✅ |
| [NKBIP-06](https://nostr.blue/wiki/nkbip-06) | Nostr MIME Types | M tag | ✅ |
| [NKBIP-08](https://nostr.blue/wiki/nkbip-08) | Book Wikilinks | book:: macro | ✅ |

**NKBIP Features:**
- **Publications (NKBIP-01)**: Create and browse curated publications with nested chapters/sections using Kind 30040 (index) and Kind 30041 (content) with AsciiDoc markup and wikilink support
- **Wiki (NIP-54)**: Browse and create wiki articles with full wikilink syntax (`[[Target]]` or `[[target|display]]`), backlink discovery, and d-tag normalization
- **Embeddings (NKBIP-02)**: Vector embedding storage for semantic search across publications
- **Citations (NKBIP-03)**: Four citation types - internal Nostr references (Kind 30), external web (Kind 31), hardcopy (Kind 32), and AI prompts (Kind 33)
- **Directories (NKBIP-04)**: File system abstraction with drives, directories, tracebacks, and symlinks
- **Book References (NKBIP-08)**: `book::` macro syntax for referencing publication sections (e.g., `book::bible:genesis 2:4-9 | kjv`)

## 🤝 Contributing

Contributions are welcome! Please follow these guidelines:

### Development Guidelines

- Follow Rust conventions and use `cargo clippy` for linting
- Use `cargo fmt` for consistent formatting
- Keep components small and focused (< 300 lines)
- Utilize hooks for reusable reactive logic
- Document public APIs with doc comments
- Write meaningful commit messages
- Test on multiple browsers and screen sizes

### Pull Request Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

MIT License - See [LICENSE](LICENSE) file for details

## 🙏 Acknowledgments

- **[rust-nostr](https://rust-nostr.org/)** - Comprehensive Nostr SDK by [@yukibtc](https://github.com/yukibtc)
  - Special thanks for the IndexedDB implementation that enabled 0.2.0's performance gains
- **[CDK (Cashu Development Kit)](https://github.com/cashubtc/cdk)** - Production-grade Cashu ecash wallet implementation
- **[Dioxus](https://dioxuslabs.com/)** - Modern Rust web framework with excellent reactive state management
- **[Nostr Protocol](https://nostr.com)** - Decentralized communication protocol
- **The Nostr Community** - For building the decentralized social web

## 🔗 Links

- **Website**: [https://nostr.blue](https://nostr.blue)
- **Repository**: [https://github.com/patrickulrich/nostr.blue](https://github.com/patrickulrich/nostr.blue)
- **Nostr Protocol**: [https://nostr.com](https://nostr.com)
- **rust-nostr**: [https://rust-nostr.org](https://rust-nostr.org)

## 📞 Support

- Open an [issue](https://github.com/patrickulrich/nostr.blue/issues) for bug reports
- Find the developer on Nostr: `npub1patrlck0muvqevgytp4etpen0xsvrlw0hscp4qxgy40n852lqwwsz79h9a`

---

**Built with ⚡ Rust + Dioxus + rust-nostr + CDK**
