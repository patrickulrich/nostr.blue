# nostr.blue

A multi-platform Nostr client built using **Rust + Dioxus + rust-nostr** with integrated CDK wallet.

![Version](https://img.shields.io/badge/version-0.8.7-blue)
![License](https://img.shields.io/badge/license-MIT-green)
![Rust](https://img.shields.io/badge/rust-1.82+-orange)
![Platforms](https://img.shields.io/badge/platforms-Web%20%7C%20Android%20%7C%20Desktop-blue)
![CDK](https://img.shields.io/badge/CDK-0.14.2-purple)

## 🌟 Overview

nostr.blue is a modern Nostr client built entirely in Rust. It runs as a WebAssembly app in browsers, as a native Android app via WebView, and as a Linux desktop application. It provides a broad Nostr experience across social feeds, code collaboration, marketplaces, publishing, media, and payments, with a shared cross-platform application layer for web, Android, and desktop.

## ⚡ Nostr Features

- **Real-time Social Feeds** - Smart relay routing using the outbox model (NIP-65) for reliable content discovery
- **Encrypted Messaging** - Full DM support with NIP-04 (legacy), NIP-17 (private), and NIP-44 (versioned encryption)
- **Lightning Zaps** - Send and receive Bitcoin micropayments (NIP-57) with NWC integration (NIP-47)
- **AI Chat** - Provider-aware AI chat with authenticated and custom-provider flows
- **Rich Media** - Polls (NIP-88), Livestreaming (NIP-53), Voice Messages (NIP-A0), Podcasts
- **Blossom Media Management** - Upload, delete, and mirror media across Blossom servers (BUD-01/02/04)
- **Long-form Content** - Articles (NIP-23) with encrypted drafts (NIP-37), Photos (NIP-68), Videos (NIP-71)
- **Wiki & Publications** - NIP-54 wiki pages with wikilinks, NKBIP-01 curated publications with AsciiDoc
- **Custom Emoji Packs** - Discover, install, manage, and search custom emoji packs and recents
- **External Content** - NIP-73 support for books (ISBN), papers (DOI), Bitcoin transactions/addresses
- **P2P Trading** - View NIP-69 peer-to-peer Bitcoin orders with depth charts and market data
- **Marketplace** - NIP-99 classified listings with product browse, cart, and Cashu/Lightning checkout
- **Code Collaboration** - Git repositories (NIP-34), code snippets (NIP-C0), issues, and pull requests
- **Social Organization** - Communities (NIP-72), Lists (NIP-51), Data Vending Machines (NIP-90)
- **Secure Authentication** - Browser extension (NIP-07), remote signer (NIP-46), and Android signer (NIP-55) with Amber/nsecBunker
- **Platform Integration** - Shared platform abstractions plus native Android signer, storage, clipboard, download, and media integration
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
- **[Dioxus 0.7.3](https://dioxuslabs.com/)** - Multi-platform reactive UI framework for Rust
- **dioxus-stores** - Advanced state management library for reactive global state
- **[Dioxus CLI](https://dioxuslabs.com/learn/0.7/CLI)** - Development server, WASM bundler, and native build tooling

### Platform Targets
| Platform | Backend | Database | Feature Flag |
|----------|---------|----------|-------------|
| Web (default) | WebAssembly | IndexedDB | `web` |
| Android | WebView via Dioxus mobile | nostrdb (LMDB) + SQLite | `mobile` |
| Linux Desktop | WebView via Dioxus desktop | nostrdb (LMDB) + SQLite | `desktop` |

### Nostr Protocol
- **[rust-nostr SDK](https://rust-nostr.org/)** - Comprehensive Nostr implementation
  - `nostr-sdk` - High-level client with relay pool management
  - `nostr` - Core protocol types and event handling
  - `nostr-database` - Database abstraction layer
  - `nostr-indexeddb` - IndexedDB persistent storage (web)
  - `nostr-ndb` - nostrdb/LMDB persistent storage (native)
  - `nostr-browser-signer` - NIP-07 browser extension integration (web)
  - `nostr-connect` - NIP-46 remote signer protocol (Amber, nsecBunker)
  - `nwc` - NIP-47 Nostr Wallet Connect for remote Lightning wallet integration

### Cashu Protocol
- **[CDK](https://github.com/cashubtc/cdk)** - Cashu Development Kit for ecash wallet functionality
  - `cdk` - Core Cashu wallet implementation with mint/melt operations, quote management, and proof handling (with `auth` feature for NUT-21/22 protected mints)
  - `cdk-common` - Common types, database traits, and utilities for Cashu protocol
  - `cdk-sqlite` - SQLite wallet database (native platforms)
  - Custom IndexedDB implementation of `WalletDatabase` trait for browser persistence
  - Atomic keyset counter management prevents "Blinded Message already signed" errors

### Styling & UI
- **[TailwindCSS 4](https://tailwindcss.com/)** - Utility-first CSS framework
- Custom icon components with SVG optimization

### Additional Libraries
- **serde** - Serialization/deserialization
- **chrono** - Date and time handling
- **pulldown-cmark** - Markdown parsing
- **ammonia** - HTML sanitization
- **reqwest** - HTTP client for LNURL and external services
- **gloo-storage** - LocalStorage API wrapper (web)
- **tokio** - Async runtime for parallel operations
- **jni** / **ndk-context** - Android JNI bridge (mobile)
- **git2** - Native git operations (native platforms)

## 📦 Architecture

- `android/` - Android shell, JNI bridge, resources, and mobile-specific integration
- `scripts/` - Developer and release tooling, including Android packaging
- `src/platform/` - Cross-platform abstraction layer for storage, clipboard, downloads, timers, spawning, timestamps, Android signer, and mobile helpers
- `src/components/` - Reusable UI building blocks for feeds, chat, code, media, emoji, wallet, and commerce flows
- `src/routes/` - Page-level route modules; larger product areas are grouped in submodules such as `code/`, `music/`, `wiki/`, `shop/`, and `settings/`
- `src/stores/` - Global reactive state for auth, Nostr client access, wallet state, media, relays, social features, and UI settings
- `src/services/` - External-service integrations and domain logic such as AI chat, payments, search, git hosting, and podcast APIs
- `src/utils/` - Parsing, protocol helpers, formatting, validation, and shared utility code

## 🚦 Getting Started

### Prerequisites (All Platforms)

- **Rust 1.82+** (install via [rustup](https://rustup.rs/))
- **Node.js 18+** and **npm** (for TailwindCSS)
- **Dioxus CLI 0.7.x** (development server and bundler)

### Installation

```bash
# Clone the repository
git clone https://github.com/patrickulrich/nostr.blue.git
cd nostr.blue

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WASM target (for web builds)
rustup target add wasm32-unknown-unknown

# Install Dioxus CLI (0.7.x required)
cargo install dioxus-cli@0.7

# Install Node dependencies
npm install

# Initialize protocol documentation submodules
git submodule update --init --recursive

# Build frontend assets
npm run build:assets
```

Run `git submodule update --init --recursive` again when new submodules are added. See `.gitmodules` for the list of submodules.

### Web Development

```bash
# Build assets and run the Dioxus dev server
npm run dev

# Visit http://localhost:8080
```

The development server includes:
- Hot reload on Rust code changes
- Automatic asset generation for Tailwind and the git worker bundle
- Source maps for debugging

### Web Production Build

```bash
# Build frontend assets and optimized WASM bundle
npm run build

# Output files in dist/
```

Production builds are optimized with:
- Link-time optimization (LTO)
- Size optimization (`opt-level = "z"`)
- Single codegen unit for minimal binary size
- Panic abort for smaller WASM binaries

### Android Build

#### Prerequisites

- **Android SDK** with platform tools and build tools
- **Android NDK 27** (27.0.12077973 or compatible)
- **aarch64-linux-android** Rust target
- Committed launcher assets under `android/res`

```bash
# Install Android Rust target
rustup target add aarch64-linux-android

# Set environment variables (adjust paths to your setup)
export ANDROID_HOME="$HOME/android-sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
```

#### Building Android release artifacts

The Android build scripts handle Rust cross-compilation, Gradle packaging, OpenSSL library bundling, ProGuard rules, app name/icons, and release signing.

```bash
# Build the Android APK (ARM64)
./scripts/build-android.sh

# Output: ./nostrblue-release.apk

# Build the Android App Bundle
./scripts/build-android-aab.sh

# Output: ./nostrblue-release.aab
```

#### What the build script does

1. Cleans stale JNI libraries from previous builds
2. Runs `dx build --platform android --target aarch64-linux-android --no-default-features --features mobile`
3. Ensures OpenSSL shared libraries (`libssl.so`, `libcrypto.so`) are in `jniLibs/arm64-v8a`
4. Copies ProGuard rules to keep JNI bridge methods from R8 stripping
5. Overlays the repo-owned Android resources from `android/res`, including launcher assets
6. Re-runs Gradle (`assembleRelease` for APKs, `bundleRelease` for AABs) to package the final Android artifact
7. Copies the artifact to project root as `nostrblue-release.apk` or `nostrblue-release.aab`

#### Installing on a device

```bash
# Via USB or wireless ADB
adb install -r nostrblue-release.apk
```

#### Android-specific features

- **NIP-55 Signer Integration** — Login via Amber or other NIP-55 signer apps using Android's ContentResolver. Auto-detects installed signers and can retrieve public keys without manual npub entry.
- **Native Storage** — Uses the app-private files directory (`context.filesDir`) for persistent data, resolved via JNI since `dirs::data_dir()` returns `None` on Android.
- **ProGuard/R8 Safe** — Custom keep rules in `android/proguard-rules.pro` prevent code stripping of JNI-called static methods in `MainActivity.kt`.

### Desktop Build (Linux)

```bash
# Build and run the desktop app
dx serve --platform desktop --no-default-features --features desktop
```

## 🔌 Protocol Support

### Nostr

| NIP | Description | Status |
|-----|-------------|--------|
| [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) | Basic protocol | ✅ |
| [NIP-02](https://github.com/nostr-protocol/nips/blob/master/02.md) | Follow List | ✅ |
| [NIP-03](https://github.com/nostr-protocol/nips/blob/master/03.md) | OpenTimestamps Attestations | ❌ |
| [NIP-04](https://github.com/nostr-protocol/nips/blob/master/04.md) | Encrypted DM (legacy) | ✅ |
| [NIP-05](https://github.com/nostr-protocol/nips/blob/master/05.md) | DNS Identifiers | ✅ |
| [NIP-06](https://github.com/nostr-protocol/nips/blob/master/06.md) | Key derivation from mnemonic | ✅ |
| [NIP-07](https://github.com/nostr-protocol/nips/blob/master/07.md) | Browser extension signing | ✅ |
| [NIP-09](https://github.com/nostr-protocol/nips/blob/master/09.md) | Event Deletion | ✅ |
| [NIP-10](https://github.com/nostr-protocol/nips/blob/master/10.md) | Text Notes and Threads | ✅ |
| [NIP-11](https://github.com/nostr-protocol/nips/blob/master/11.md) | Relay Information Document | ✅ |
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
| [NIP-28](https://github.com/nostr-protocol/nips/blob/master/28.md) | Public Chat | ✅ |
| [NIP-29](https://github.com/nostr-protocol/nips/blob/master/29.md) | Relay-based Groups | ❌ |
| [NIP-30](https://github.com/nostr-protocol/nips/blob/master/30.md) | Custom Emoji | ✅ |
| [NIP-31](https://github.com/nostr-protocol/nips/blob/master/31.md) | Unknown Events | ❌ |
| [NIP-32](https://github.com/nostr-protocol/nips/blob/master/32.md) | Labeling | ❌ |
| [NIP-34](https://github.com/nostr-protocol/nips/blob/master/34.md) | Git stuff | ✅ |
| [NIP-35](https://github.com/nostr-protocol/nips/blob/master/35.md) | Torrents | ❌ |
| [NIP-36](https://github.com/nostr-protocol/nips/blob/master/36.md) | Sensitive Content | ✅ |
| [NIP-37](https://github.com/nostr-protocol/nips/blob/master/37.md) | Draft Events | ✅ |
| [NIP-38](https://github.com/nostr-protocol/nips/blob/master/38.md) | User Statuses | ✅ |
| [NIP-39](https://github.com/nostr-protocol/nips/blob/master/39.md) | External Identities | ❌ |
| [NIP-40](https://github.com/nostr-protocol/nips/blob/master/40.md) | Expiration Timestamp | ✅ |
| [NIP-41](https://github.com/vitorpamplona/nips/blob/09da0dd779db90f41a557f199cc483be6f4916c5/41.md) | Editable Short Notes | ✅ |
| [NIP-42](https://github.com/nostr-protocol/nips/blob/master/42.md) | Client Auth to Relays | ✅ |
| [NIP-43](https://github.com/nostr-protocol/nips/blob/master/43.md) | Relay Access Metadata | ❌ |
| [NIP-44](https://github.com/nostr-protocol/nips/blob/master/44.md) | Encrypted Payloads | ✅ |
| [NIP-45](https://github.com/nostr-protocol/nips/blob/master/45.md) | Counting results | ✅ |
| [NIP-46](https://github.com/nostr-protocol/nips/blob/master/46.md) | Remote Signing | ✅ |
| [NIP-47](https://github.com/nostr-protocol/nips/blob/master/47.md) | Wallet Connect | ✅ |
| [NIP-48](https://github.com/nostr-protocol/nips/blob/master/48.md) | Proxy Tags | ✅ |
| [NIP-49](https://github.com/nostr-protocol/nips/blob/master/49.md) | Private Key Encryption | ✅ |
| [NIP-50](https://github.com/nostr-protocol/nips/blob/master/50.md) | Search Capability | ✅ |
| [NIP-51](https://github.com/nostr-protocol/nips/blob/master/51.md) | Lists | ✅ |
| [NIP-52](https://github.com/nostr-protocol/nips/blob/master/52.md) | Calendar Events | ✅ |
| [NIP-53](https://github.com/nostr-protocol/nips/blob/master/53.md) | Live Activities | ✅ |
| [NIP-54](https://github.com/nostr-protocol/nips/blob/master/54.md) | Wiki | ✅ |
| [NIP-55](https://github.com/nostr-protocol/nips/blob/master/55.md) | Android Signer Application | ✅ |
| [NIP-56](https://github.com/nostr-protocol/nips/blob/master/56.md) | Reporting | ✅ |
| [NIP-57](https://github.com/nostr-protocol/nips/blob/master/57.md) | Lightning Zaps | ✅ |
| [NIP-58](https://github.com/nostr-protocol/nips/blob/master/58.md) | Badges | ✅ |
| [NIP-59](https://github.com/nostr-protocol/nips/blob/master/59.md) | Gift Wrap | ✅ |
| [NIP-60](https://github.com/nostr-protocol/nips/blob/master/60.md) | Cashu Wallet | ✅ |
| [NIP-61](https://github.com/nostr-protocol/nips/blob/master/61.md) | Nutzaps | ✅ |
| [NIP-62](https://github.com/nostr-protocol/nips/blob/master/62.md) | Request to Vanish | ✅ |
| [NIP-64](https://github.com/nostr-protocol/nips/blob/master/64.md) | Chess (PGN) | ❌ |
| [NIP-65](https://github.com/nostr-protocol/nips/blob/master/65.md) | Relay List Metadata | ✅ |
| [NIP-66](https://github.com/nostr-protocol/nips/blob/master/66.md) | Relay Discovery | ❌ |
| [NIP-68](https://github.com/nostr-protocol/nips/blob/master/68.md) | Picture-first feeds | ✅ |
| [NIP-69](https://github.com/nostr-protocol/nips/blob/master/69.md) | P2P Order events | ✅ |
| [NIP-70](https://github.com/nostr-protocol/nips/blob/master/70.md) | Protected Events | ❌ |
| [NIP-71](https://github.com/nostr-protocol/nips/blob/master/71.md) | Video Events | ✅ |
| [NIP-72](https://github.com/nostr-protocol/nips/blob/master/72.md) | Moderated Communities | ✅ |
| [NIP-73](https://github.com/nostr-protocol/nips/blob/master/73.md) | External Content IDs | ✅ |
| [NIP-75](https://github.com/nostr-protocol/nips/blob/master/75.md) | Zap Goals | ✅ |
| [NIP-77](https://github.com/nostr-protocol/nips/blob/master/77.md) | Negentropy Syncing | ✅ |
| [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md) | App-specific data | ✅ |
| [NIP-7D](https://github.com/nostr-protocol/nips/blob/master/7D.md) | Threads | ❌ |
| [NIP-84](https://github.com/nostr-protocol/nips/blob/master/84.md) | Highlights | ✅ |
| [NIP-86](https://github.com/nostr-protocol/nips/blob/master/86.md) | Relay Management API | ❌ |
| [NIP-87](https://github.com/nostr-protocol/nips/blob/master/87.md) | Mint Discoverability | ✅ |
| [NIP-88](https://github.com/nostr-protocol/nips/blob/master/88.md) | Polls | ✅ |
| [NIP-89](https://github.com/nostr-protocol/nips/blob/master/89.md) | App Handlers | ✅ |
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
- Test on multiple browsers, screen sizes, and platforms

### Verification

Run these checks before submitting PRs:

```bash
dx check
cargo check
cargo check --target wasm32-unknown-unknown
cargo clippy --target wasm32-unknown-unknown -- -D warnings
cargo check --no-default-features --features desktop
cargo clippy --no-default-features --features desktop -- -D warnings
cargo check --no-default-features --features mobile
cargo clippy --no-default-features --features mobile -- -D warnings
cargo test
```

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
