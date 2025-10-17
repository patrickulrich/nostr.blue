# PuStack

**The Complete Framework for Building Nostr Clients with AI**

PuStack is an AI-powered framework for building Nostr applications with Svelte 5, SvelteKit, TailwindCSS 3.x, Vite, shadcn-svelte, and Welshman. Build powerful Nostr applications with AI-first development - from social feeds to private messaging, PuStack provides everything you need to create decentralized apps on the Nostr protocol.

## 🚀 Quick Start

Build your Nostr app in 3 simple steps:

### 1. Install & Create
```bash
npm install -g @getstacks/stacks
stacks pustack
```

### 2. Build with AI
```bash
stacks agent
# Tell Dork AI what you want: "Build a group chat application"
```

### 3. Deploy Instantly
```bash
npm run deploy
# ✅ App deployed to NostrDeploy.com!
```

## ✨ What Makes PuStack Special

- **🤖 AI-First Development**: Build complete Nostr apps with just one prompt using Dork AI agent
- **⚡ Fast & Reactive**: Svelte 5 runes for fine-grained reactivity and optimal performance
- **🔗 50+ NIPs Supported**: Comprehensive Nostr protocol implementation via Welshman
- **🎨 Beautiful UI**: 48+ shadcn-svelte components with light/dark theme support
- **🔐 Built-in Security**: NIP-07 browser signing, NIP-44 encryption, event validation
- **💰 Payments Ready**: Lightning zaps (NIP-57), Cashu wallets (NIP-60), Wallet Connect (NIP-47)
- **📱 Production Ready**: TypeScript, testing, deployment, and responsive design included
- **🛠 Battle-Tested**: Built on Welshman, the toolkit extracted from production Coracle client

## 🛠 Technology Stack

- **Svelte 5**: Modern reactive framework with runes and fine-grained reactivity
- **SvelteKit**: Full-stack framework for routing, SSR, and server-side data loading
- **TailwindCSS 3.x**: Utility-first CSS framework for styling
- **Vite**: Fast build tool and development server
- **shadcn-svelte**: 48+ unstyled, accessible UI components built with Melt UI
- **Welshman**: Battle-tested Nostr toolkit extracted from Coracle client
- **TanStack Query**: Data fetching, caching, and state management (Svelte version)
- **TypeScript**: Type-safe JavaScript development

## 🎯 Real-World Examples

PuStack is a conversion of MKStack from React to Svelte 5 + Welshman. The framework inherits proven patterns from production applications.

### Production Apps (MKStack Legacy)

Real Nostr applications that informed PuStack's development:

- **[Chorus](https://chorus.community/)**: Facebook-style groups on Nostr with built-in eCash wallet
- **[Blobbi](https://www.blobbi.pet/)**: Digital pet companions that live forever on the decentralized web
- **[Treasures](https://treasures.to/)**: Decentralized geocaching adventure powered by Nostr
- **[Coracle](https://coracle.social/)**: Social client that pioneered the Welshman toolkit

These apps demonstrate the production-readiness of the patterns PuStack implements with Svelte + Welshman.

## 🔧 Core Features

### Authentication & Users
- `LoginArea` component with account switching
- Welshman stores (`pubkey`, `session`, `signer`) for authentication state
- `createQuery` with `load()` for fetching user profiles
- Built-in login functions (`loginWithNip07`, `loginWithNip46`, etc.)
- Multi-account management

### Nostr Protocol Support
- **Social Features**: User profiles (NIP-01), follow lists (NIP-02), reactions (NIP-25), reposts (NIP-18)
- **Messaging**: Private DMs (NIP-17), public chat (NIP-28), group chat (NIP-29), encryption (NIP-44)
- **Payments**: Lightning zaps (NIP-57), Cashu wallets (NIP-60), Nutzaps (NIP-61), Wallet Connect (NIP-47)
- **Content**: Long-form articles (NIP-23), file metadata (NIP-94), live events (NIP-53), calendars (NIP-52)

### Data Management
- Welshman `load()` for querying and `publishEvent()` for publishing
- Welshman Router for intelligent relay selection
- Event validation and filtering
- Infinite scroll with TanStack Query
- Multi-relay support with quality scoring

### UI Components
- 48+ shadcn-svelte components (buttons, forms, dialogs, etc.)
- `NoteContent` component for rich text rendering (using `@welshman/content`)
- `EditProfileForm` for profile management
- `RelaySelector` for relay switching
- `CommentsSection` for threaded discussions
- Light/dark theme system with mode-watcher

### Media & Files
- File upload with Blossom server integration
- NIP-94 compatible file metadata
- Image and video support
- File attachment to events with `imeta` tags

### Advanced Features
- NIP-19 identifier routing (`npub1`, `note1`, `nevent1`, `naddr1`)
- Cryptographic operations via Welshman signers (NIP-44 encryption/decryption)
- Lightning payments and zaps
- Real-time event subscriptions with Welshman `subscribe()`
- Responsive design with mobile support

## 🤖 AI Development with Dork

PuStack includes Dork, a built-in AI agent that understands your codebase and Nostr protocols:

### Supported AI Providers

Configure your AI provider with `stacks configure`:

- **OpenRouter** ([openrouter.ai](https://openrouter.ai/)): Enter your API key from settings
- **Routstr** ([routstr.com](https://www.routstr.com/)): Use Cashu tokens for payment
- **PayPerQ** ([ppq.ai](https://ppq.ai/)): OpenAI-compatible API

### How Dork Works

- **Context-Aware**: Understands your entire codebase and project structure
- **Nostr Expert**: Built-in knowledge of 50+ NIPs and best practices
- **Instant Implementation**: Makes changes directly to your code following Svelte 5/TypeScript best practices

Example prompts:
```bash
"Add user profiles with avatars and bio using Welshman"
"Implement NIP-17 private messaging with encryption"
"Add a dark mode toggle with mode-watcher"
"Create a marketplace with NIP-15 using addressable events"
```

## 📁 Project Structure

```
src/
├── routes/              # SvelteKit file-based routing
│   ├── +page.svelte    # Page components
│   ├── +layout.svelte  # Layout components
│   └── +page.ts        # Data loading functions
├── lib/                 # Reusable library code
│   ├── components/     # UI components
│   │   ├── ui/         # shadcn-svelte components (48+ available)
│   │   ├── auth/       # Authentication components
│   │   └── comments/   # Comment system components
│   ├── stores/         # Svelte stores for state management
│   ├── welshman/       # Welshman integration utilities
│   │   ├── setup.ts    # Router configuration
│   │   └── client.ts   # Welshman helpers
│   └── utils/          # Utility functions
│       ├── nostr.ts    # Nostr helpers
│       ├── blossom.ts  # File uploads
│       └── zaps.ts     # Lightning payments
├── test/               # Testing utilities
└── app.html            # HTML template
```

## 🎨 UI Components

PuStack includes 48+ shadcn-svelte components:

**Layout**: Card, Separator, Sheet, Sidebar, ScrollArea, Resizable
**Navigation**: Breadcrumb, NavigationMenu, Menubar, Tabs, Pagination
**Forms**: Button, Input, Textarea, Select, Checkbox, RadioGroup, Switch, Slider
**Feedback**: Alert, AlertDialog, Toast, Progress, Skeleton
**Overlay**: Dialog, Popover, HoverCard, Tooltip, ContextMenu, DropdownMenu
**Data Display**: Table, Avatar, Badge, Calendar, Chart, Carousel
**And many more...

## 🔐 Security & Best Practices

- **Never use `any` type**: Always use proper TypeScript types
- **Event validation**: Filter events through validator functions for custom kinds
- **Efficient queries**: Minimize separate queries to avoid rate limiting
- **Proper error handling**: Graceful handling of invalid NIP-19 identifiers
- **Secure authentication**: Use Welshman signers, never request private keys directly

## 📱 Responsive Design

- Mobile-first approach with Tailwind breakpoints
- Svelte 5 runes for reactive responsive behavior
- Touch-friendly interactions
- Optimized for all screen sizes

## 🧪 Testing

- Vitest with jsdom environment
- Svelte Testing Library with jest-dom matchers
- Test setup provides all necessary context and stores
- Mocked browser APIs (matchMedia, scrollTo, IntersectionObserver, ResizeObserver)

## 🚀 Deployment

Built-in deployment to NostrDeploy.com:

```bash
npm run deploy
```

Your app goes live instantly with:
- Automatic builds
- CDN distribution
- HTTPS support
- Custom domains available

## 📚 Documentation

For detailed documentation on building Nostr applications with PuStack:

- [Svelte 5 Documentation](https://svelte.dev/)
- [SvelteKit Documentation](https://svelte.dev/docs/kit)
- [Welshman GitHub](https://github.com/coracle-social/welshman)
- [Nostr Protocol Documentation](https://nostr.com)
- [shadcn-svelte Components](https://shadcn-svelte.com/)

## 🤝 Contributing

PuStack is open source and welcomes contributions. The framework is designed to be:

- **Extensible**: Easy to add new NIPs and features
- **Maintainable**: Clean architecture with TypeScript and Svelte 5
- **Testable**: Comprehensive testing setup included
- **Documented**: Clear patterns and examples

## 📄 License

Open source - build amazing Nostr applications and help grow the decentralized web!

---

**Built with PuStack** - Powered by Svelte 5 + Welshman

*Build your Nostr app in minutes, not months. Start with AI, deploy instantly.*