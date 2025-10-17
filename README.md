# PuStack

**The Complete Framework for Building Nostr Clients with Svelte 5 + Welshman**

PuStack is a production-ready framework for building Nostr applications with Svelte 5, SvelteKit, TailwindCSS 3.x, and Welshman. This is a complete conversion of MKStack from React + Nostrify to Svelte 5 + Welshman, providing developers with everything needed to quickly build custom Nostr clients - from social feeds to messaging apps to marketplaces.

## 🚀 Quick Start

### Create Your Nostr App

```bash
# Clone the framework
git clone https://github.com/yourusername/pustack.git my-nostr-app
cd my-nostr-app

# Install dependencies
npm install

# Start development server
npm run dev

# Visit http://localhost:5173
```

The included demo app shows a basic Nostr social client. Use it as a starting point to build your own custom Nostr application!

### Build & Deploy

```bash
# Type check your code
npm run check

# Run tests
npm test

# Build for production
npm run build
```

## ✨ Why Choose PuStack?

- **🚀 Start Building in Minutes**: Pre-configured setup with routing, auth, and data fetching
- **⚡ Modern Stack**: Svelte 5 runes + Welshman = optimal performance and DX
- **🎨 48+ UI Components**: Production-ready shadcn-svelte components with full theming
- **🔐 Auth Built-In**: NIP-07, NIP-46, NIP-01 login flows ready to use
- **📦 Reusable Components**: Note rendering, profiles, composer, comments - all included
- **🔗 Welshman Powered**: Battle-tested Nostr toolkit extracted from Coracle
- **💪 Type-Safe**: Full TypeScript support with strict mode
- **📱 Responsive**: Mobile-first design that works everywhere
- **🛠 Extensible**: Clean architecture makes it easy to add NIPs and features

## 🛠 Technology Stack

- **Svelte 5**: Modern reactive framework with runes and fine-grained reactivity
- **SvelteKit**: Full-stack framework for routing, SSR, and server-side data loading
- **TailwindCSS 3.x**: Utility-first CSS framework for styling
- **Vite**: Fast build tool and development server
- **shadcn-svelte**: 48+ unstyled, accessible UI components built with Melt UI
- **Welshman**: Battle-tested Nostr toolkit extracted from Coracle client
- **TanStack Query**: Data fetching, caching, and state management (Svelte version)
- **TypeScript**: Type-safe JavaScript development

## 🎯 What Can You Build?

PuStack provides the building blocks for any Nostr application. Here are some ideas:

### Social Apps
- **Twitter/X Alternative**: Microblogging with feeds, profiles, and reactions
- **Reddit/Hacker News**: Link aggregation with threaded discussions
- **Instagram/Pinterest**: Photo sharing with visual feeds
- **Facebook Groups**: Community spaces with members and moderation

### Messaging Apps
- **Slack Alternative**: Team chat with channels (NIP-28/29)
- **Discord Clone**: Communities with text channels
- **Private Messenger**: End-to-end encrypted DMs (NIP-17)
- **Group Chat**: Multi-user conversations with encryption

### Marketplace Apps
- **Classifieds**: Craigslist-style listings (NIP-99)
- **eCommerce**: Product marketplace with Lightning payments
- **Services Platform**: Gig economy on Nostr
- **NFT Marketplace**: Digital collectibles trading

### Content Platforms
- **Blogging Platform**: Long-form articles (NIP-23)
- **Newsletter**: Paid subscriptions with Lightning
- **Video Platform**: YouTube alternative with zaps
- **Podcast App**: Audio content with value-for-value

### Other Ideas
- **Event Platform**: Meetups and conferences (NIP-52)
- **Job Board**: Decentralized hiring
- **Q&A Platform**: Stack Overflow on Nostr
- **Dating App**: Matchmaking with Nostr identities

The included demo app (social feed) shows how these pieces work together. Fork it and build your vision!

## 🏆 Inspired By Production Apps

PuStack inherits proven patterns from real-world applications:

- **[Coracle](https://coracle.social/)**: Social client that pioneered Welshman
- **[Chorus](https://chorus.community/)**: Facebook-style groups with eCash
- **[Blobbi](https://www.blobbi.pet/)**: Digital pet companions
- **[Treasures](https://treasures.to/)**: Decentralized geocaching

## 📦 What's Included

PuStack comes with a complete demo app that showcases the framework's capabilities. Use these pre-built components and patterns in your own app:

### ✅ Authentication System
- **LoginArea Component**: Login/signup UI with account switching
- **Multi-Account Support**: Switch between multiple logged-in accounts
- **NIP-07 Login**: Browser extension signing (Alby, nos2x, etc.)
- **NIP-46 Login**: Remote signer support (nsecbunker)
- **NIP-01 Login**: Private key login (nsec)
- **Session Management**: Persistent sessions across page reloads
- **Profile Editing**: Full profile editor with metadata fields

### ✅ Feed Components
- **Feed Query Pattern**: TanStack Query + Welshman load() integration
- **Note Cards**: Reusable component with author, content, timestamp
- **Skeleton Loading**: Beautiful loading states while data fetches
- **Auto-Refresh**: Configurable polling for real-time updates
- **Error Handling**: Graceful error states with retry buttons

### ✅ Publishing Components
- **Note Composer**: Modal dialog for creating/replying to notes
- **Character Counter**: Visual feedback with limit enforcement
- **Reply Threading**: Proper e-tag structure for replies
- **Mutation Hooks**: useNostrPublish() for easy publishing
- **Toast Notifications**: User feedback for publish success/failure

### ✅ Content Rendering
- **NoteContent Component**: Parse and render Nostr content
- **Rich Text Support**: Links, mentions, hashtags, code, invoices
- **Welshman Parser**: Uses `@welshman/content` for correct parsing
- **Extensible**: Easy to add custom content types
- **Styled**: Tailwind classes for consistent appearance

### ✅ Profile System
- **Profile Queries**: Fetch and cache user metadata (kind 0)
- **Profile Editor**: Full modal for editing user profiles
- **Avatar Display**: Fallback to generated names when no avatar
- **Metadata Fields**: name, display_name, about, picture, banner, nip05, lud16
- **Profile Publishing**: Uses Welshman publishProfile() helper

### ✅ Routing Patterns
- **NIP-19 Route**: Dynamic `[nip19]` route handles all identifier types
- **Type Detection**: Decodes and routes npub, note, nevent, nprofile, naddr
- **Profile Pages**: Automatic profile view for npub/nprofile
- **Note Pages**: Single note view for note/nevent/naddr
- **Error Handling**: 404 redirect for invalid identifiers

### ✅ Welshman Integration
- **Router Setup**: Pre-configured Welshman Router in `stores/welshman.ts`
- **Relay Selection**: Smart relay routing based on pubkey and mode
- **Connection Pool**: Managed relay connections with cleanup
- **Query Helpers**: TanStack Query patterns for Welshman load()
- **Publish Helpers**: Mutation wrappers for event publishing
- **Signer Support**: NIP-07, NIP-46, NIP-01 signing flows

### ✅ UI Component Library
- **48+ shadcn-svelte Components**: Button, Card, Dialog, Input, etc.
- **Theme System**: Light/dark mode with mode-watcher integration
- **Toast Store**: Reactive toast notifications with useToast() hook
- **Loading States**: Skeleton components for all data loading
- **Empty States**: Consistent patterns for no data scenarios
- **Responsive**: Mobile-first with Tailwind breakpoints
- **Accessible**: ARIA labels and keyboard navigation

## 🔨 Built With

### Core Technologies
- **Svelte 5**: Modern reactive framework with runes
- **SvelteKit**: Full-stack framework with file-based routing
- **Welshman**: Battle-tested Nostr toolkit from Coracle
- **TanStack Query**: Powerful data fetching and caching
- **TypeScript**: Type-safe development

### UI & Styling
- **shadcn-svelte**: 48+ accessible UI components
- **TailwindCSS 3.x**: Utility-first CSS framework
- **Melt UI**: Headless component primitives
- **mode-watcher**: System-aware dark mode

### Nostr Integration
- **@welshman/app**: High-level Nostr client framework
- **@welshman/net**: Relay connection and messaging
- **@welshman/util**: Nostr utilities and helpers
- **@welshman/signer**: NIP-07, NIP-46, NIP-01 signing
- **@welshman/content**: Rich text content parsing
- **@welshman/router**: Intelligent relay routing
- **nostr-tools**: NIP-19 encoding/decoding

## 📁 Project Structure

```
src/
├── routes/                    # SvelteKit file-based routing
│   ├── +page.svelte          # Home feed page
│   ├── +layout.svelte        # Root layout with theme/query setup
│   ├── +error.svelte         # Error boundary
│   └── [nip19]/
│       └── +page.svelte      # NIP-19 identifier routing
├── lib/                       # Reusable library code
│   ├── components/           # UI components
│   │   ├── ui/              # shadcn-svelte components (48+)
│   │   ├── auth/            # Authentication components
│   │   │   ├── LoginArea.svelte
│   │   │   ├── LoginDialog.svelte
│   │   │   ├── SignupDialog.svelte
│   │   │   └── AccountSwitcher.svelte
│   │   ├── comments/        # Comment system
│   │   │   ├── CommentsSection.svelte
│   │   │   ├── CommentForm.svelte
│   │   │   └── Comment.svelte
│   │   ├── Note.svelte      # Note display with reactions
│   │   ├── NoteContent.svelte  # Rich text content parser
│   │   ├── NoteComposer.svelte # Note creation modal
│   │   ├── ProfileEditor.svelte # Profile editing modal
│   │   └── RelaySelector.svelte # Relay management
│   ├── stores/              # Svelte stores (.svelte.ts)
│   │   ├── auth.ts         # Authentication & session management
│   │   ├── welshman.ts     # Welshman Router configuration
│   │   ├── publish.svelte.ts    # Note publishing
│   │   ├── comments.svelte.ts   # Comment queries
│   │   ├── accounts.svelte.ts   # Multi-account management
│   │   ├── toast.svelte.ts      # Toast notifications
│   │   ├── theme.svelte.ts      # Theme management
│   │   └── appStore.ts          # App configuration
│   └── utils/               # Utility functions
│       ├── genUserName.ts  # Fallback username generator
│       └── ...
├── test/                    # Testing utilities
└── index.css               # Global styles and Tailwind
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

## 🧩 Extend the Framework

PuStack is designed to be extended. Here are common additions developers make:

### Popular Extensions
- **Threaded Replies**: Add note detail pages with comment trees
- **Follow Lists**: Implement NIP-02 for social graphs
- **Personalized Feeds**: Filter by followed users
- **Search**: Add NIP-50 relay search
- **Lightning Zaps**: Integrate NIP-57 for tipping
- **Private Messages**: Build NIP-17 encrypted DMs
- **Notifications**: Real-time notification system
- **Infinite Scroll**: Pagination for large feeds
- **Media Uploads**: Blossom or NIP-96 file hosting
- **Link Previews**: Open Graph meta tag parsing

### Framework Additions
- **Long-form Articles** (NIP-23): Blog/newsletter components
- **Communities** (NIP-72): Group discussion spaces
- **Live Events** (NIP-53): Streaming and live chat
- **Marketplaces** (NIP-15, NIP-99): eCommerce flows
- **Calendar Events** (NIP-52): Event management
- **Badges** (NIP-58): Achievement system
- **Lists** (NIP-51): Bookmarks, mutes, pins
- **Relay Hints**: Advanced relay discovery
- **Offline Mode**: Service worker caching

## 📚 Documentation

For detailed documentation on building Nostr applications with PuStack:

- [Svelte 5 Documentation](https://svelte.dev/)
- [SvelteKit Documentation](https://svelte.dev/docs/kit)
- [Welshman GitHub](https://github.com/coracle-social/welshman)
- [Nostr Protocol Documentation](https://nostr.com)
- [shadcn-svelte Components](https://shadcn-svelte.com/)

## 🧑‍💻 Building Your App

### Framework Patterns

PuStack establishes clean patterns you can follow when building your app:

#### 1. Querying Nostr Data

```svelte
<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { load } from '@welshman/net';

  // Create a query for any Nostr data
  const myData = createQuery(() => ({
    queryKey: ['my-data', /* params */],
    queryFn: async ({ signal }) => {
      return await load({
        relays: [],  // Empty = use Router defaults
        filters: [{ kinds: [1], limit: 20 }],
        signal
      });
    }
  }));
</script>

{#if $myData.isLoading}
  <Skeleton />
{:else if $myData.data}
  {#each $myData.data as item}
    <YourComponent {item} />
  {/each}
{/if}
```

#### 2. Publishing Events
```svelte
<script lang="ts">
  import { useNostrPublish } from '$lib/stores/publish.svelte';

  const publish = useNostrPublish();

  function handlePublish() {
    $publish.mutate({
      kind: 1,
      content: 'Hello Nostr!',
      tags: []
    });
  }
</script>
```

#### 3. Using Svelte 5 Runes
```svelte
<script lang="ts">
  // Reactive state
  let count = $state(0);

  // Computed values
  let doubled = $derived(count * 2);

  // Effects
  $effect(() => {
    console.log('Count changed:', count);
  });
</script>
```

#### 4. Creating Reusable Components

Follow the pattern in `src/lib/components/Note.svelte`:
- Accept event data as props
- Query related data (profiles, reactions, etc.)
- Emit events for user interactions
- Use shadcn-svelte for consistent UI

### Customizing the Framework

**Relay Configuration:**
Edit `src/lib/stores/appStore.ts` to change default relays for your app.

**Theme & Branding:**
Modify `src/index.css` to customize colors, fonts, and design tokens.

**Router Behavior:**
Configure relay selection logic in `src/lib/stores/welshman.ts`.

**Add NIPs:**
Create new stores in `src/lib/stores/` for additional NIPs.

**UI Components:**
Add custom components to `src/lib/components/` and reuse across routes.

## 🤝 Contributing to the Framework

We welcome contributions that make PuStack better for all developers:

### Framework Guidelines
- **Svelte 5 runes** for all new reactivity (no legacy $: syntax)
- **TanStack Query** patterns for all Nostr data fetching
- **Welshman** for all Nostr protocol operations
- **TypeScript strict mode** - no `any` types
- **shadcn-svelte** for UI components (don't reinvent)
- **Reusable patterns** - components should work in any app
- **Clear documentation** - examples and JSDoc comments

### What to Contribute
- ✅ New reusable components (feeds, profiles, media, etc.)
- ✅ NIP implementations (with proper TypeScript types)
- ✅ Query patterns and hooks
- ✅ UI component extensions
- ✅ Documentation and examples
- ✅ Bug fixes and improvements

### What NOT to Contribute
- ❌ App-specific business logic
- ❌ Opinionated features (add as optional examples)
- ❌ Heavy dependencies (keep the framework light)
- ❌ Breaking changes without discussion

## 🙏 Acknowledgments

- **[Coracle](https://coracle.social/)** - For pioneering Welshman and Nostr client patterns
- **[MKStack](https://github.com/michaelkernaghan/mkstack)** - Original React implementation
- **[Welshman](https://github.com/coracle-social/welshman)** - Battle-tested Nostr toolkit
- **[shadcn-svelte](https://shadcn-svelte.com/)** - Beautiful accessible components

## 📄 License

Open source - build amazing Nostr applications and help grow the decentralized web!

---

**PuStack** - The framework for building Nostr clients with Svelte 5 + Welshman
*Your app. Your rules. Built in hours, not months.*