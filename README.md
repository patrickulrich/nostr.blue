# nostr.blue

A decentralized social network client built on the Nostr protocol using **Rust + Dioxus + rust-nostr**.

![Version](https://img.shields.io/badge/version-0.6.0-blue)
![License](https://img.shields.io/badge/license-MIT-green)
![Rust](https://img.shields.io/badge/rust-1.90+-orange)

## 🌟 Overview

nostr.blue is a modern Nostr client built entirely in Rust and compiled to WebAssembly. It provides a comprehensive social networking experience on the decentralized Nostr protocol with advanced features like communities, Lightning zaps, encrypted messaging, and Data Vending Machines.

## ✨ Features

### Core Social Features
- ✅ **Home Feed** - Real-time feed from people you follow with infinite scroll
- ✅ **Profiles** - View and edit user profiles with follow/unfollow
- ✅ **Post Composer** - Create text notes with rich content support
- ✅ **Interactions** - Reactions, reposts, and threaded replies
- ✅ **Search** - Find users, notes, and content by hashtags with NIP-50 relay search support
- ✅ **Notifications** - Track mentions, replies, and interactions
- ✅ **Explore** - Discover trending content and new users
- ✅ **Polls (NIP-88)** - Create and vote on single-choice and multiple-choice polls with real-time results, countdown timers, and hashtag support

### Advanced Features
- ✅ **Outbox Model (NIP-65)** - Smart relay routing using author's preferred write relays for improved content discovery
- ✅ **Communities (NIP-72)** - Moderated topic-based communities
- ✅ **Lists (NIP-51)** - Create and manage custom lists and bookmarks
- ✅ **Lightning Zaps (NIP-57)** - Send and receive Bitcoin micropayments
- ✅ **Direct Messages (NIP-04/NIP-17/NIP-44)** - Encrypted private messaging with full NIP-17 compliance
- ✅ **Long-form Articles (NIP-23)** - Rich markdown articles with metadata and threaded comments
- ✅ **Photos Feed (NIP-68)** - Dedicated feed for image content with metadata
- ✅ **Videos (NIP-71)** - Video event support with playback controls and comments
- ✅ **Voice Messages (NIP-A0)** - Short audio messages up to 60 seconds with waveform visualization and threaded replies
- ✅ **Web Bookmarks (NIP-B0)** - Pocket-inspired bookmark manager with auto-metadata fetching, tag filtering, favorites, and search
- ✅ **Comments (NIP-22)** - Structured threaded comments on articles and videos
- ✅ **Music Player (NIP-38)** - Wavlake integration with live listening status broadcast
- ✅ **Data Vending Machines (NIP-90)** - AI-powered content services
- ✅ **Cashu Wallet (NIP-60)** - Bitcoin ecash wallet with Lightning deposits/withdrawals, multi-mint support, persistent IndexedDB storage, atomic keyset counter management, and automatic cleanup of spent proofs with browser extension signer compatibility
- ✅ **Settings Sync (NIP-78)** - Cloud-synced app preferences via Nostr

### User Experience
- ✅ **Light/Dark Theme** - System preference detection with manual override
- ✅ **Responsive Design** - Mobile-first design with desktop optimization
- ✅ **Infinite Scroll** - Smooth pagination across all feeds
- ✅ **Rich Content** - Embedded images, videos, and link previews with proper parsing
- ✅ **NIP-19 Support** - Full support for npub, note, nprofile, nevent identifiers
- ✅ **Browser Extension** - NIP-07 signing with Alby, nos2x, etc.
- ✅ **Remote Signer (NIP-46)** - Secure signing with Amber, nsecBunker via bunker:// URIs
- ✅ **Enhanced Login UI** - Educational sign-in experience with security best practices
- ✅ **Real-Time Updates** - Live feed and notification updates
- ✅ **Offline Support** - Browse cached content without internet
- ✅ **Instant Loading** - Sub-100ms load times with IndexedDB cache
- ✅ **Blossom Media Storage** - Decentralized image uploads with quality control
- ✅ **Enhanced Emoji Picker** - 600+ emojis across 11 categories with NIP-51 custom emoji support
- ✅ **GIF Search (NIP-50)** - Search and insert GIFs directly from the post composer using relay search
- ✅ **Username Autocomplete** - @ mention autocomplete in all composers with relay search
- ✅ **Video Sharing Modal** - Share videos via link, Nostr post, or encrypted DM
- ✅ **Followers Count** - Real-time follower statistics using nostr.band API
- ✅ **Interactive Notifications** - Clickable avatars and usernames in notification feed
- ✅ **Social Media Embeds** - Twitter/X and Twitch embeds with click-to-load for performance

## 🛠 Technology Stack

### Core Framework
- **[Dioxus 0.6](https://dioxuslabs.com/)** - Modern reactive web framework for Rust
- **WebAssembly** - Compiled to WASM for near-native browser performance
- **[Trunk](https://trunkrs.dev/)** - WASM web application bundler

### Nostr Protocol
- **[rust-nostr SDK](https://rust-nostr.org/)** - Comprehensive Nostr implementation
  - `nostr-sdk` - High-level client with relay pool management
  - `nostr` - Core protocol types and event handling
  - `nostr-database` - Database abstraction layer
  - `nostr-indexeddb` - IndexedDB persistent storage
  - `nostr-browser-signer` - NIP-07 browser extension integration
  - `nostr-connect` - NIP-46 remote signer protocol (Amber, nsecBunker)

### Bitcoin & Ecash
- **[CDK](https://github.com/cashubtc/cdk)** - Cashu Development Kit for ecash wallet functionality
  - `cdk` - Core Cashu wallet implementation with mint/melt operations, quote management, and proof handling
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

## 📊 Performance Metrics

### Architecture Improvements

- **Database-First Pattern**: Check IndexedDB → Background relay sync
- **Parallel Fetching**: `tokio::join!()` for simultaneous queries
- **Smart Caching**: 5-minute TTL for profiles, persistent event storage
- **Outbox Model (NIP-65)**: Fetches content from author's preferred write relays for reliable content discovery
- **Cashu Wallet Persistence**:
  - IndexedDB storage for quotes, proofs, keyset counters, and mint cache
  - Atomic keyset counter increments prevent duplicate blinded messages
  - Multi-step mint/melt operations survive page refresh
  - 9 object stores for complete wallet state management
- **Relay Optimization**:
  - Max latency: 2 seconds (auto-skip slow relays)
  - Subscription verification (ban mismatched events)
  - Adaptive retry intervals based on success rate
  - Automatic relay hints in nevent/nprofile identifiers
- **Real-Time Subscriptions**: Live updates via `limit=0, since=now` filters

## 📦 Project Structure

```
nostrbluerust/
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
│   │   ├── rich_content.rs # Content rendering (Wavlake embeds)
│   │   ├── threaded_comment.rs # Comment threads
│   │   ├── music_player.rs # Wavlake music player (NIP-38)
│   │   ├── track_card.rs   # Music track display
│   │   ├── wavlake_zap_dialog.rs # Music artist zaps
│   │   ├── wallet_balance_card.rs # Cashu wallet balance display
│   │   ├── token_list.rs   # Cashu token list by mint
│   │   ├── transaction_history.rs # Cashu transaction history
│   │   ├── cashu_setup_wizard.rs # Cashu wallet setup flow
│   │   ├── cashu_send_modal.rs # Send ecash modal
│   │   ├── cashu_receive_modal.rs # Receive ecash modal
│   │   ├── cashu_receive_lightning_modal.rs # Lightning deposit modal
│   │   ├── cashu_send_lightning_modal.rs # Lightning withdrawal modal
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
│   │   ├── voicemessages.rs # Voice messages feed (NIP-A0)
│   │   ├── polls.rs        # Polls feed (NIP-88)
│   │   ├── poll_view.rs    # Individual poll view (NIP-88)
│   │   ├── poll_new.rs     # Poll creation form (NIP-88)
│   │   ├── webbookmarks.rs # Web bookmarks manager (NIP-B0)
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
│   │   └── url_metadata.rs # URL metadata fetching (Open Graph, Twitter Cards)
│   ├── services/           # External services
│   │   ├── lnurl.rs        # Lightning URL handling
│   │   ├── wavlake.rs      # Wavlake API integration
│   │   └── trending.rs     # Trending algorithm
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

- **Rust 1.90+** (install via [rustup](https://rustup.rs/))
- **Node.js 18+** and **npm** (for TailwindCSS)
- **Trunk** (WASM bundler)
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

# Install Trunk
cargo install trunk wasm-bindgen-cli

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
trunk serve

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
trunk build --release

# Output files in dist/
```

Production builds are optimized with:
- Link-time optimization (LTO)
- Size optimization (`opt-level = "z"`)
- Single codegen unit for minimal binary size
- Panic abort for smaller WASM binaries

## 🔌 Nostr Protocol Support

This client implements the following Nostr Improvement Proposals (NIPs):

| NIP | Description | Status |
|-----|-------------|--------|
| [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) | Basic protocol (events, signatures, filters) | ✅ |
| [NIP-02](https://github.com/nostr-protocol/nips/blob/master/02.md) | Contact/follow lists | ✅ |
| [NIP-04](https://github.com/nostr-protocol/nips/blob/master/04.md) | Encrypted direct messages (legacy) | ✅ |
| [NIP-07](https://github.com/nostr-protocol/nips/blob/master/07.md) | Browser extension signing | ✅ |
| [NIP-10](https://github.com/nostr-protocol/nips/blob/master/10.md) | Reply conventions and threading | ✅ |
| [NIP-17](https://github.com/nostr-protocol/nips/blob/master/17.md) | Private direct messages | ✅ |
| [NIP-18](https://github.com/nostr-protocol/nips/blob/master/18.md) | Reposts | ✅ |
| [NIP-19](https://github.com/nostr-protocol/nips/blob/master/19.md) | bech32 identifiers (npub, note, naddr, etc.) | ✅ |
| [NIP-22](https://github.com/nostr-protocol/nips/blob/master/22.md) | Comments on articles, videos, and other events | ✅ |
| [NIP-23](https://github.com/nostr-protocol/nips/blob/master/23.md) | Long-form articles | ✅ |
| [NIP-25](https://github.com/nostr-protocol/nips/blob/master/25.md) | Reactions | ✅ |
| [NIP-30](https://github.com/nostr-protocol/nips/blob/master/30.md) | Custom emoji | ✅ |
| [NIP-38](https://github.com/nostr-protocol/nips/blob/master/38.md) | User status (music listening, etc.) | ✅ |
| [NIP-44](https://github.com/nostr-protocol/nips/blob/master/44.md) | Encrypted direct messages (versioned) | ✅ |
| [NIP-46](https://github.com/nostr-protocol/nips/blob/master/46.md) | Nostr Connect (remote signer protocol) | ✅ |
| [NIP-50](https://github.com/nostr-protocol/nips/blob/master/50.md) | Search capability (relay-based search, GIF search) | ✅ |
| [NIP-51](https://github.com/nostr-protocol/nips/blob/master/51.md) | Lists (people, bookmarks, music votes, emoji sets) | ✅ |
| [NIP-57](https://github.com/nostr-protocol/nips/blob/master/57.md) | Lightning zaps | ✅ |
| [NIP-59](https://github.com/nostr-protocol/nips/blob/master/59.md) | Gift wrap (sealed sender) | ✅ |
| [NIP-60](https://github.com/nostr-protocol/nips/blob/master/60.md) | Cashu wallet (ecash) | ✅ |
| [NIP-65](https://github.com/nostr-protocol/nips/blob/master/65.md) | Relay list metadata | ✅ |
| [NIP-68](https://github.com/nostr-protocol/nips/blob/master/68.md) | Picture events with imeta tags | ✅ |
| [NIP-71](https://github.com/nostr-protocol/nips/blob/master/71.md) | Video events | ✅ |
| [NIP-72](https://github.com/nostr-protocol/nips/blob/master/72.md) | Moderated communities | ✅ |
| [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md) | Application-specific data | ✅ |
| [NIP-88](https://github.com/nostr-protocol/nips/blob/master/88.md) | Polls (single/multiple choice with results) | ✅ |
| [NIP-90](https://github.com/nostr-protocol/nips/blob/master/90.md) | Data Vending Machines | ✅ |
| [NIP-A0](https://github.com/nostr-protocol/nips/blob/master/A0.md) | Voice messages | ✅ |
| [NIP-B0](https://github.com/nostr-protocol/nips/blob/master/B0.md) | Web bookmarks | ✅ |

### Blossom Protocol Support

| BUD | Description | Status |
|-----|-------------|--------|
| [BUD-01](https://github.com/hzrd149/blossom) | Core Blossom protocol for decentralized blob storage | ✅ |

## 🔧 Configuration

### Default Relays

The client connects to a default set of popular Nostr relays. Users can customize their relay list in Settings.

### Environment Variables

No environment variables are required. All configuration is managed through the UI and stored locally or synced via NIP-78.

### Theme Configuration

Themes are configured in `tailwind.config.js` with CSS variables for easy customization. The theme system supports:
- Light mode
- Dark mode
- System preference detection
- Persistent user preference (LocalStorage + NIP-78 sync)

## 🎯 Roadmap

### In Planning
- 🔄 **Negentropy Sync** - 10-100x bandwidth reduction
- 🔄 **Database Cleanup** - Auto-delete old events
- 🔄 **Relay Statistics UI** - Monitor relay performance
- 🔄 **Background Sync Tasks** - Auto-update every 5 minutes
- 🔄 **Advanced Filters** - Custom feed filtering and muting
- 🔄 **Web of Trust** - Configurable WoT scoring
- 🔄 **Virtual Scrolling** - Handle feeds with 10,000+ events
- 🔄 **PWA Support** - Install to home screen, push notifications
- 🔄 **Multi-account** - Switch between multiple Nostr identities
- 🔄 **Backup/Restore** - Account data export/import

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
- Find the developer on Nostr: `npub1...` (if you have a public key to share)

---

**Built with ⚡ Rust + Dioxus + rust-nostr**
