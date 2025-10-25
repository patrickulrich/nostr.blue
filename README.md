# nostr.blue

**Decentralized Social Network on Nostr**

nostr.blue is a modern social network client built on the Nostr protocol, providing a full-featured decentralized social experience with communities, DVMs, Lightning zaps, and more.

## 🌟 Features

### Core Social Features
- **Home Feed**: Personalized feed showing posts from people you follow
- **Profile Pages**: View user profiles, follow/unfollow, and see post history
- **Notifications**: Real-time notifications for mentions, reactions, and replies
- **Messages**: Private encrypted messaging with other Nostr users
- **Bookmarks**: Save posts for later reading (NIP-51)
- **Lists**: Create and manage custom user lists (NIP-51)
- **Communities**: Browse and join NIP-72 communities (moderated Reddit-style groups)

### Advanced Features
- **Data Vending Machines (DVMs)**: Browse DVM services and view curated feeds (NIP-90)
- **Lightning Zaps**: Send and receive Bitcoin payments on posts (NIP-57)
- **Real Zap Counts**: Live zap totals displayed on every post
- **Dark Mode**: System, light, or dark theme with persistent settings
- **Reply Threading**: Threaded conversations with inline reply composer
- **Rich Content**: Support for images, videos, custom emoji, code blocks, and more

### Content & Interaction
- **Reactions**: React to posts with likes
- **Reposts**: Share content with your followers
- **Replies**: Inline reply composer with character counter
- **Hashtag Navigation**: Browse posts by hashtag
- **Profile Mentions**: @ mentions with profile previews
- **Media Gallery**: Image and video support with NIP-94 imeta tags
- **Embedded Notes**: Quote posts with embedded note previews

## 🚀 Getting Started

### Prerequisites
- Node.js 18+
- npm or yarn

### Installation

```bash
# Clone the repository
git clone https://github.com/patrickulrich/nostr.blue.git
cd nostr.blue

# Install dependencies
npm install

# Start development server
npm run dev

# Visit http://localhost:5173
```

### Building for Production

```bash
# Type check your code
npm run check

# Run tests
npm test

# Build for production
npm run build
```

## 🛠 Technology Stack

- **Svelte 5**: Modern reactive framework with runes and fine-grained reactivity
- **SvelteKit**: Full-stack framework with routing, SSR, and server-side data loading
- **Welshman v0.5.3**: Battle-tested Nostr toolkit extracted from Coracle
- **TanStack Query v6**: Data fetching, caching, and state management
- **TypeScript**: Type-safe development with strict mode
- **TailwindCSS 3**: Utility-first CSS framework for styling
- **shadcn-svelte**: 48+ accessible UI components built with Melt UI
- **Vite**: Fast build tool and development server

### Welshman Integration

nostr.blue uses all 8 Welshman packages for comprehensive Nostr support:
- **@welshman/app**: High-level Nostr client framework
- **@welshman/content**: Rich text content parsing (links, mentions, emoji, etc.)
- **@welshman/feeds**: Feed management and filtering
- **@welshman/net**: Relay connection and messaging
- **@welshman/router**: Intelligent relay routing and outbox model
- **@welshman/signer**: NIP-07, NIP-46, NIP-01 signing support
- **@welshman/store**: Event caching and storage
- **@welshman/util**: Nostr utilities and helpers

## 📡 Nostr Protocol Support

nostr.blue implements many Nostr Improvement Proposals (NIPs):

- **NIP-01**: Basic protocol flow and event kinds
- **NIP-02**: Contact/follow lists
- **NIP-07**: Browser extension signing (Alby, nos2x, etc.)
- **NIP-10**: Text note references and threading
- **NIP-19**: Identifier encoding (npub, note, nevent, naddr, nprofile)
- **NIP-25**: Reactions
- **NIP-44**: Encrypted direct messages
- **NIP-46**: Remote signer support (nsecbunker)
- **NIP-51**: Lists (bookmarks, pin lists, follow sets)
- **NIP-57**: Lightning zaps
- **NIP-65**: Relay list metadata for outbox model
- **NIP-72**: Moderated communities
- **NIP-90**: Data Vending Machines (content discovery)
- **NIP-94**: File metadata (imeta tags for images)

## 🎨 Features in Detail

### Dark Mode & Theming
Choose from light, dark, or system theme. Theme preference is saved locally and persists across sessions.

### Communities (NIP-72)
Browse and participate in moderated communities. Join communities to see them in your sidebar navigation.

### Data Vending Machines (NIP-90)
Discover AI-powered services on Nostr that can:
- Curate content feeds
- Provide personalized recommendations
- Search and discover content
- Process data requests

Browse available DVMs and request custom feeds from each service.

### Lightning Integration (NIP-57)
Send and receive Bitcoin payments directly on posts:
- Zap posts to support creators
- Real-time zap counts displayed
- Multiple payment methods (WebLN, NWC, QR codes)
- Custom zap amounts and comments

### Bookmarks & Lists (NIP-51)
Organize your Nostr experience:
- Bookmark notes, articles, hashtags, and URLs
- Create custom user lists
- Public and private lists
- View bookmarked content in dedicated feed

### Reply Composer
Inline reply functionality on every note:
- Character counter (5000 character limit)
- Cmd/Ctrl + Enter keyboard shortcut
- Cancel and reset options
- Real-time publish feedback

### Rich Content Parsing
Using @welshman/content for comprehensive content rendering:
- Links with automatic previews
- Profile mentions with hover cards
- Hashtag navigation
- Custom emoji from emoji tags
- Inline code blocks
- Cashu tokens
- Lightning invoices
- Link grids for adjacent URLs
- Embedded note quotes
- Image and video galleries

## 🔐 Authentication

nostr.blue supports multiple authentication methods:
- **Browser extensions (NIP-07)**: Alby, nos2x, Flamingo, and other Nostr extensions
- **Remote signers (NIP-46)**: nsecbunker and compatible remote signers
- **nsec (private key)**: Direct private key login (use with caution)
- **Multi-account**: Switch between multiple logged-in accounts
- **Read-only mode**: Browse with npub (public key)

## 🌐 Deployment

The app can be deployed to any static hosting service:

```bash
npm run build
```

Deploy the `build` folder to:
- Vercel
- Netlify
- GitHub Pages
- Cloudflare Pages
- IPFS
- Any static host

## 📁 Project Structure

```
src/
├── routes/                       # SvelteKit file-based routing
│   ├── +page.svelte             # Home feed page
│   ├── +layout.svelte           # Root layout with theme/query setup
│   ├── [nip19]/+page.svelte     # NIP-19 identifier routing
│   ├── bookmarks/               # Bookmarks page
│   ├── communities/             # Communities discovery
│   ├── community/[id]/          # Individual community pages
│   ├── dvm/                     # DVM discovery and feeds
│   ├── explore/                 # Content exploration
│   ├── lists/                   # User lists management
│   ├── messages/                # Private messaging
│   ├── notifications/           # Notifications feed
│   ├── settings/                # User settings
│   ├── trending/                # Trending content
│   └── t/[tag]/                 # Hashtag pages
├── lib/
│   ├── components/              # UI components
│   │   ├── ui/                 # shadcn-svelte components (48+)
│   │   ├── auth/               # Authentication components
│   │   ├── AppSidebar.svelte   # Main navigation sidebar
│   │   ├── MainLayout.svelte   # 3-column layout wrapper
│   │   ├── Note.svelte         # Note card display
│   │   ├── NoteContent.svelte  # Rich text content parser
│   │   ├── NoteComposer.svelte # Note creation modal
│   │   ├── ReplyComposer.svelte # Inline reply composer
│   │   ├── BookmarkButton.svelte
│   │   ├── ReactionButton.svelte
│   │   ├── RepostButton.svelte
│   │   ├── ZapButton.svelte
│   │   └── ...
│   ├── stores/                 # Svelte stores (.svelte.ts)
│   │   ├── auth.ts            # Authentication & session
│   │   ├── publish.svelte.ts  # Note publishing
│   │   ├── notifications.svelte.ts
│   │   ├── messages.svelte.ts
│   │   ├── following.svelte.ts
│   │   ├── dvm.svelte.ts
│   │   └── ...
│   ├── hooks/                  # Custom hooks
│   │   ├── useUserBookmarks.svelte.ts
│   │   ├── useLists.svelte.ts
│   │   ├── useDVMs.svelte.ts
│   │   ├── useDVMJob.svelte.ts
│   │   └── ...
│   ├── services/              # Service layer
│   │   ├── outbox.ts         # Outbox model implementation
│   │   └── trending.ts       # Trending content service
│   └── utils/                 # Utility functions
└── test/                      # Testing utilities
```

## 🎯 Roadmap

### Planned Features
- [ ] Infinite scroll for feeds
- [ ] Advanced search (NIP-50)
- [ ] Media uploads (Blossom/NIP-96)
- [ ] Long-form articles (NIP-23)
- [ ] Live events (NIP-53)
- [ ] Calendar events (NIP-52)
- [ ] Badges (NIP-58)
- [ ] Relay management UI
- [ ] Offline mode with service workers
- [ ] Mobile app (Capacitor)

## 🧪 Testing

- Vitest with jsdom environment
- Svelte Testing Library with jest-dom matchers
- Type checking with svelte-check
- ESLint for code quality

```bash
# Run all tests
npm test

# Type check
npm run check

# Watch mode
npm run check:watch
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

### Development Guidelines
- Use Svelte 5 runes for all reactivity (no legacy $: syntax)
- Follow TanStack Query patterns for data fetching
- Use Welshman for all Nostr protocol operations
- TypeScript strict mode - no `any` types
- Use shadcn-svelte for UI components
- Test thoroughly before submitting PRs
- Keep components focused and reusable

## 📄 License

Open source - built for the decentralized web.

## 🔗 Links

- **Website**: [nostr.blue](https://nostr.blue)
- **GitHub**: [patrickulrich/nostr.blue](https://github.com/patrickulrich/nostr.blue)
- **Nostr Protocol**: [nostr.com](https://nostr.com)

## 🙏 Acknowledgments

- **[Coracle](https://coracle.social/)** - For pioneering Welshman and Nostr client patterns
- **[Welshman](https://github.com/coracle-social/welshman)** - Battle-tested Nostr toolkit
- **[shadcn-svelte](https://shadcn-svelte.com/)** - Beautiful accessible components
- **The Nostr Community** - For building the decentralized web

---

**nostr.blue** - Decentralized social network on Nostr
*Built with Svelte 5 and Welshman*
