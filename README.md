# nostr.blue

A decentralized social network client built on the Nostr protocol using **Rust + Dioxus + rust-nostr**.

![Version](https://img.shields.io/badge/version-0.1.0-blue)
![License](https://img.shields.io/badge/license-MIT-green)
![Rust](https://img.shields.io/badge/rust-1.90+-orange)

## 🌟 Overview

nostr.blue is a modern, performant Nostr client built entirely in Rust and compiled to WebAssembly. It provides a comprehensive social networking experience on the decentralized Nostr protocol with advanced features like communities, Lightning zaps, encrypted messaging, and Data Vending Machines.

## ✨ Features

### Core Social Features
- ✅ **Home Feed** - Real-time feed from people you follow with infinite scroll
- ✅ **Profiles** - View and edit user profiles with follow/unfollow
- ✅ **Post Composer** - Create text notes with rich content support
- ✅ **Interactions** - Reactions, reposts, and threaded replies
- ✅ **Search** - Find users, notes, and content by hashtags
- ✅ **Notifications** - Track mentions, replies, and interactions
- ✅ **Explore** - Discover trending content and new users

### Advanced Features
- ✅ **Communities (NIP-72)** - Moderated topic-based communities
- ✅ **Lists (NIP-51)** - Create and manage custom lists and bookmarks
- ✅ **Lightning Zaps (NIP-57)** - Send and receive Bitcoin micropayments
- ✅ **Direct Messages (NIP-44)** - End-to-end encrypted private messaging
- ✅ **Long-form Articles (NIP-23)** - Rich markdown articles with metadata
- ✅ **Photos Feed** - Dedicated feed for image content
- ✅ **Videos (NIP-71)** - Video event support with playback controls
- ✅ **Data Vending Machines (NIP-90)** - AI-powered content services
- ✅ **Settings Sync (NIP-78)** - Cloud-synced app preferences via Nostr

### User Experience
- ✅ **Light/Dark Theme** - System preference detection with manual override
- ✅ **Responsive Design** - Mobile-first design with desktop optimization
- ✅ **Infinite Scroll** - Smooth pagination across all feeds
- ✅ **Rich Content** - Embedded images, videos, and link previews
- ✅ **NIP-19 Support** - Full support for npub, note, nprofile, nevent identifiers
- ✅ **Browser Extension** - NIP-07 signing with Alby, nos2x, etc.

## 🛠 Technology Stack

### Core Framework
- **[Dioxus 0.6](https://dioxuslabs.com/)** - Modern reactive web framework for Rust
- **WebAssembly** - Compiled to WASM for near-native browser performance
- **[Trunk](https://trunkrs.dev/)** - WASM web application bundler

### Nostr Protocol
- **[rust-nostr SDK](https://rust-nostr.org/)** - Comprehensive Nostr implementation
  - `nostr-sdk` - High-level client with relay pool management
  - `nostr` - Core protocol types and event handling
  - `nostr-database` - In-memory event storage optimized for WASM
  - `nostr-browser-signer` - NIP-07 browser extension integration

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

## 📦 Project Structure

```
nostrbluerust/
├── src/
│   ├── components/          # Reusable UI components
│   │   ├── note.rs         # Note/event display
│   │   ├── note_card.rs    # Compact note card
│   │   ├── note_composer.rs # Post creation
│   │   ├── reply_composer.rs # Reply creation
│   │   ├── profile_card.rs # User profile display
│   │   ├── photo_card.rs   # Photo grid item
│   │   ├── article_card.rs # Long-form article card
│   │   ├── zap_modal.rs    # Lightning zap interface
│   │   ├── rich_content.rs # Content rendering
│   │   ├── threaded_comment.rs # Comment threads
│   │   ├── sidebar.rs      # Navigation sidebar
│   │   ├── layout.rs       # App shell layout
│   │   └── icons.rs        # SVG icon components
│   ├── routes/             # Page routes
│   │   ├── home.rs         # Home feed
│   │   ├── profile.rs      # User profiles
│   │   ├── note.rs         # Single note view
│   │   ├── photos.rs       # Photo feed
│   │   ├── videos.rs       # Video feed
│   │   ├── communities.rs  # Communities
│   │   ├── lists.rs        # User lists
│   │   ├── dms.rs          # Direct messages
│   │   ├── notifications.rs # Notifications
│   │   ├── settings.rs     # User settings
│   │   ├── trending.rs     # Trending content
│   │   ├── explore.rs      # Discover feed
│   │   ├── dvm.rs          # Data Vending Machines
│   │   ├── search.rs       # Search interface
│   │   ├── hashtag.rs      # Hashtag feed
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
│   │   ├── nostr_client.rs # Nostr SDK client instance
│   │   ├── auth_store.rs   # Authentication state
│   │   ├── profiles.rs     # Profile cache
│   │   ├── bookmarks.rs    # Bookmarked content
│   │   ├── dms.rs          # DM conversations
│   │   ├── notifications.rs # Notification state
│   │   ├── settings_store.rs # NIP-78 synced settings
│   │   ├── theme_store.rs  # Theme preferences
│   │   └── signer.rs       # Event signing
│   ├── utils/              # Utility functions
│   │   ├── nip19.rs        # NIP-19 identifier parsing
│   │   ├── content_parser.rs # Content extraction
│   │   ├── markdown.rs     # Markdown rendering
│   │   ├── time.rs         # Time formatting
│   │   ├── validation.rs   # Input validation
│   │   ├── list_kinds.rs   # NIP-51 list types
│   │   ├── thread_tree.rs  # Reply threading
│   │   └── article_meta.rs # Article metadata
│   ├── services/           # External services
│   │   ├── lnurl.rs        # Lightning URL handling
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
| [NIP-07](https://github.com/nostr-protocol/nips/blob/master/07.md) | Browser extension signing | ✅ |
| [NIP-19](https://github.com/nostr-protocol/nips/blob/master/19.md) | bech32 identifiers (npub, note, etc.) | ✅ |
| [NIP-23](https://github.com/nostr-protocol/nips/blob/master/23.md) | Long-form articles | ✅ |
| [NIP-25](https://github.com/nostr-protocol/nips/blob/master/25.md) | Reactions | ✅ |
| [NIP-44](https://github.com/nostr-protocol/nips/blob/master/44.md) | Encrypted direct messages | ✅ |
| [NIP-51](https://github.com/nostr-protocol/nips/blob/master/51.md) | Lists (people, bookmarks, etc.) | ✅ |
| [NIP-57](https://github.com/nostr-protocol/nips/blob/master/57.md) | Lightning zaps | ✅ |
| [NIP-59](https://github.com/nostr-protocol/nips/blob/master/59.md) | Gift wrap (sealed sender) | ✅ |
| [NIP-65](https://github.com/nostr-protocol/nips/blob/master/65.md) | Relay list metadata | ✅ |
| [NIP-71](https://github.com/nostr-protocol/nips/blob/master/71.md) | Video events | ✅ |
| [NIP-72](https://github.com/nostr-protocol/nips/blob/master/72.md) | Moderated communities | ✅ |
| [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md) | Application-specific data | ✅ |
| [NIP-90](https://github.com/nostr-protocol/nips/blob/master/90.md) | Data Vending Machines | ✅ |

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

### Current Status: Feature Complete MVP

The core application is fully functional with comprehensive Nostr protocol support.

### Upcoming Enhancements

- 🔄 **Relay Health Monitoring** - Real-time relay connection status
- 🔄 **Media Uploads** - Direct image/video uploads via NIP-94/NIP-96
- 🔄 **Advanced Filters** - Custom feed filtering and muting
- 🔄 **Web of Trust** - Configurable WoT scoring
- 🔄 **Performance Optimizations** - Virtual scrolling, lazy loading
- 🔄 **PWA Support** - Offline capabilities and install prompt
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
- **[Dioxus](https://dioxuslabs.com/)** - Modern Rust web framework
- **[Nostr Protocol](https://nostr.com)** - Decentralized communication protocol
- **The Nostr Community** - For building the decentralized social web

## 🔗 Links

- **Website**: [https://nostr.blue](https://nostr.blue)
- **Repository**: [https://github.com/patrickulrich/nostr.blue](https://github.com/patrickulrich/nostr.blue)
- **Nostr Protocol**: [https://nostr.com](https://nostr.com)
- **rust-nostr**: [https://rust-nostr.org](https://rust-nostr.org)

## 📞 Support

- Open an [issue](https://github.com/patrickulrich/nostr.blue/issues) for bug reports
- Join discussions in [GitHub Discussions](https://github.com/patrickulrich/nostr.blue/discussions)
- Find the developer on Nostr: `npub1...` (if you have a public key to share)

---

**Built with ⚡ Rust + Dioxus + rust-nostr**
