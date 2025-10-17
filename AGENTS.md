# Project Overview

This project is a Nostr client application built with Svelte 5, TailwindCSS 3.x, Vite, shadcn-svelte, and Welshman.

## Technology Stack

- **Svelte 5**: Modern reactive framework with runes and fine-grained reactivity for optimal performance
- **SvelteKit**: Full-stack framework for routing, SSR, and server-side data loading
- **TailwindCSS 3.x**: Utility-first CSS framework for styling
- **Vite**: Fast build tool and development server
- **shadcn-svelte**: Unstyled, accessible UI components built with Melt UI and Tailwind
- **Welshman**: Modular Nostr toolkit extracted from Coracle, providing networking, storage, and utilities
- **TanStack Query**: For data fetching, caching, and state management
- **TypeScript**: For type-safe JavaScript development

## Welshman Packages

Welshman is a modular toolkit with specialized packages:

- **@welshman/app**: Batteries-included framework for building Nostr clients
- **@welshman/util**: Core Nostr utilities for events, filters, and data structures
- **@welshman/net**: Networking layer with relay connection management and message status handling
- **@welshman/signer**: Signing implementations (NIP-01, NIP-07, NIP-46, NIP-55)
- **@welshman/store**: In-memory relay and event store
- **@welshman/router**: Tools for relay selection and routing
- **@welshman/content**: Parser and renderer for Nostr notes with customizable formatting
- **@welshman/editor**: Rich text editor with mentions and embeds support
- **@welshman/feeds**: Dynamic feed compiler and loader

## Project Structure

- `/docs/`: Specialized documentation for implementation patterns and features
- `/src/lib/`: Reusable library code and utilities
  - `/src/lib/components/`: UI components including Nostr integration
  - `/src/lib/components/ui/`: shadcn-svelte components (48+ components available)
  - `/src/lib/components/auth/`: Authentication-related components (LoginArea, LoginDialog, etc.)
  - Zap components: `ZapButton`, `ZapDialog`, `WalletModal` for Lightning payments
- `/src/lib/stores/`: Svelte stores for reactive state management
- `/src/lib/welshman/`: Welshman integration utilities
  - Connection management and relay pools
  - Event publishing and querying
  - Signing methods and user authentication
  - Feed compilation and filtering
- `/src/lib/utils/`: Utility functions including:
  - Nostr query helpers and event validation
  - Profile data fetching by pubkey
  - Event publishing utilities
  - File upload via Blossom servers
  - Theme management
  - Lightning zap functionality
  - Wallet detection (WebLN + NWC)
  - Nostr Wallet Connect management
  - AI chat completions with Shakespeare API
- `/src/routes/`: SvelteKit file-based routing (pages and layouts)
  - `+page.svelte`: Page components
  - `+layout.svelte`: Layout components
  - `+page.ts` / `+page.server.ts`: Data loading
- `/src/lib/contexts/`: Context providers for global state
- `/src/tests/`: Testing utilities
- `/static/`: Static assets

## Svelte 5 Runes and Reactivity

Svelte 5 uses **runes** for reactivity instead of stores in most cases. Key runes:

- `$state()`: Reactive state (replaces `let` declarations)
- `$derived()`: Computed values (replaces `$:` reactive statements)
- `$effect()`: Side effects (replaces `onMount` and reactive blocks)
- `$props()`: Component props with reactivity

**Example:**
```svelte
<script lang="ts">
  let count = $state(0);
  let doubled = $derived(count * 2);
  
  $effect(() => {
    console.log(`Count is now: ${count}`);
  });
</script>

<button onclick={() => count++}>
  Count: {count}, Doubled: {doubled}
</button>
```

For backward compatibility with stores, use the `$` prefix to auto-subscribe:
```svelte
<script>
  import { writable } from 'svelte/store';
  const count = writable(0);
</script>

<button onclick={() => $count++}>
  Count: {$count}
</button>
```

## UI Components

The project uses shadcn-svelte components located in `$lib/components/ui`. These are unstyled, accessible components built with Melt UI and styled with Tailwind CSS. Available components include:

- **Accordion**: Vertically collapsing content panels
- **Alert**: Displays important messages to users
- **AlertDialog**: Modal dialog for critical actions requiring confirmation
- **AspectRatio**: Maintains consistent width-to-height ratio
- **Avatar**: User profile pictures with fallback support
- **Badge**: Small status descriptors for UI elements
- **Breadcrumb**: Navigation aid showing current location in hierarchy
- **Button**: Customizable button with multiple variants and sizes
- **Calendar**: Date picker component
- **Card**: Container with header, content, and footer sections
- **Carousel**: Slideshow for cycling through elements
- **Chart**: Data visualization component
- **Checkbox**: Selectable input element
- **Collapsible**: Toggle for showing/hiding content
- **Command**: Command palette for keyboard-first interfaces
- **ContextMenu**: Right-click menu component
- **Dialog**: Modal window overlay
- **Drawer**: Side-sliding panel
- **DropdownMenu**: Menu that appears from a trigger element
- **Form**: Form validation and submission handling
- **HoverCard**: Card that appears when hovering over an element
- **InputOTP**: One-time password input field
- **Input**: Text input field
- **Label**: Accessible form labels
- **Menubar**: Horizontal menu with dropdowns
- **NavigationMenu**: Accessible navigation component
- **Pagination**: Controls for navigating between pages
- **Popover**: Floating content triggered by a button
- **Progress**: Progress indicator
- **RadioGroup**: Group of radio inputs
- **Resizable**: Resizable panels and interfaces
- **ScrollArea**: Scrollable container with custom scrollbars
- **Select**: Dropdown selection component
- **Separator**: Visual divider between content
- **Sheet**: Side-anchored dialog component
- **Sidebar**: Navigation sidebar component
- **Skeleton**: Loading placeholder
- **Slider**: Input for selecting a value from a range
- **Switch**: Toggle switch control
- **Table**: Data table with headers and rows
- **Tabs**: Tabbed interface component
- **Textarea**: Multi-line text input
- **Toast**: Toast notification component
- **ToggleGroup**: Group of toggle buttons
- **Toggle**: Two-state button
- **Tooltip**: Informational text that appears on hover

These components follow Svelte patterns and use Melt UI primitives for accessibility. Import patterns:

```svelte
<script>
  import { Button } from '$lib/components/ui/button';
  import * as Card from '$lib/components/ui/card';
  import * as Dialog from '$lib/components/ui/dialog';
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Title</Card.Title>
  </Card.Header>
  <Card.Content>
    <Button>Click me</Button>
  </Card.Content>
</Card.Root>
```

## Documentation

The project includes a **`docs/`** directory containing specialized documentation for specific implementation tasks. You are encouraged to add new documentation files to help future development.

- **`docs/AI_CHAT.md`**: Read when building any AI-powered chat interfaces, implementing streaming responses, or integrating with the Shakespeare API.
- **`docs/NOSTR_COMMENTS.md`**: Read when implementing comment systems, adding discussion features to posts/articles, or building community interaction features.
- **`docs/NOSTR_INFINITE_SCROLL.md`**: Read when building feed interfaces, implementing pagination for Nostr events, or creating social media-style infinite scroll experiences.

## System Prompt Management

The AI assistant's behavior and knowledge is defined by the AGENTS.md file, which serves as the system prompt. To modify the assistant's instructions or add new project-specific guidelines:

1. Edit AGENTS.md directly
2. The changes take effect in the next session

## Nostr Protocol Integration with Welshman

This project uses Welshman, the battle-tested Nostr toolkit extracted from Coracle. Welshman provides a modular, highly configurable system for Nostr clients.

### Welshman Setup

Install the necessary Welshman packages:

```bash
npm install @welshman/app @welshman/net @welshman/util @welshman/signer @welshman/store @welshman/router
```

### Core Welshman Integration

Welshman uses individual Svelte stores rather than a single context object. All stores are available from `@welshman/app`.

**Key Welshman Stores:**
- `pubkey` - Current user's public key
- `session` - Current session information
- `signer` - Current ISigner instance
- `repository` - In-memory event storage (Repository instance)
- `relays` - Array of relay information
- `profiles` - Map of user profiles
- And many more specialized stores

**Router Configuration** in `$lib/welshman/setup.ts`:

```typescript
import { Router } from '@welshman/router';
import { pubkey } from '@welshman/app';
import { get } from 'svelte/store';

// Configure the global router
export function setupRouter(defaultRelays: string[]) {
  Router.configure({
    getUserPubkey: () => get(pubkey),
    getDefaultRelays: () => defaultRelays,
    getPubkeyRelays: (pk, mode) => {
      // Return relay URLs for a given pubkey
      // You can query kind 10002 (NIP-65) relay lists here
      return defaultRelays; // Fallback
    },
    getRelayQuality: (url) => {
      // Return 0-1 quality score for relay selection
      return 0.5; // Default quality
    },
    getLimit: () => 5, // Max relays per scenario
  });
}

// Get router instance
export const router = Router.get();
```

### Nostr Implementation Guidelines

- Always check the full list of existing NIPs before implementing any Nostr features to see what kinds are currently in use across all NIPs.
- If any existing kind or NIP might offer the required functionality, read the relevant NIPs to investigate thoroughly. Several NIPs may need to be read before making a decision.
- Only generate new kind numbers if no existing suitable kinds are found after comprehensive research.

Knowing when to create a new kind versus reusing an existing kind requires careful judgement. Introducing new kinds means the project won't be interoperable with existing clients. But deviating too far from the schema of a particular kind can cause different interoperability issues.

#### Choosing Between Existing NIPs and Custom Kinds

When implementing features that could use existing NIPs, follow this decision framework:

1. **Thorough NIP Review**: Before considering a new kind, always perform a comprehensive review of existing NIPs and their associated kinds. Get an overview of all NIPs, and then read specific NIPs and kind documentation to investigate any potentially relevant NIPs or kinds in detail. The goal is to find the closest existing solution.

2. **Prioritize Existing NIPs**: Always prefer extending or using existing NIPs over creating custom kinds, even if they require minor compromises in functionality.

3. **Interoperability vs. Perfect Fit**: Consider the trade-off between:
   - **Interoperability**: Using existing kinds means compatibility with other Nostr clients
   - **Perfect Schema**: Custom kinds allow perfect data modeling but create ecosystem fragmentation

4. **Extension Strategy**: When existing NIPs are close but not perfect:
   - Use the existing kind as the base
   - Add domain-specific tags for additional metadata
   - Document the extensions in `NIP.md`

5. **When to Generate Custom Kinds**:
   - No existing NIP covers the core functionality
   - The data structure is fundamentally different from existing patterns
   - The use case requires different storage characteristics (regular vs replaceable vs addressable)

6. **Custom Kind Publishing**: When publishing events with custom generated kinds, always include a NIP-31 "alt" tag with a human-readable description of the event's purpose.

**Example Decision Process**:
```
Need: Equipment marketplace for farmers
Options:
1. NIP-15 (Marketplace) - Too structured for peer-to-peer sales
2. NIP-99 (Classified Listings) - Good fit, can extend with farming tags
3. Custom kind - Perfect fit but no interoperability

Decision: Use NIP-99 + farming-specific tags for best balance
```

#### Tag Design Principles

When designing tags for Nostr events, follow these principles:

1. **Kind vs Tags Separation**:
   - **Kind** = Schema/structure (how the data is organized)
   - **Tags** = Semantics/categories (what the data represents)
   - Don't create different kinds for the same data structure

2. **Use Single-Letter Tags for Categories**:
   - **Relays only index single-letter tags** for efficient querying
   - Use `t` tags for categorization, not custom multi-letter tags
   - Multiple `t` tags allow items to belong to multiple categories

3. **Relay-Level Filtering**:
   - Design tags to enable efficient relay-level filtering with `#t: ["category"]`
   - Avoid client-side filtering when relay-level filtering is possible
   - Consider query patterns when designing tag structure

4. **Tag Examples**:
   ```json
   // ❌ Wrong: Multi-letter tag, not queryable at relay level
   ["product_type", "electronics"]

   // ✅ Correct: Single-letter tag, relay-indexed and queryable
   ["t", "electronics"]
   ["t", "smartphone"]
   ["t", "android"]
   ```

5. **Querying Best Practices**:
   ```typescript
   // ❌ Inefficient: Get all events, filter in JavaScript
   const events = await load([{ kinds: [30402] }]);
   const filtered = events.filter(e => hasTag(e, 'product_type', 'electronics'));

   // ✅ Efficient: Filter at relay level
   const events = await load([{ kinds: [30402], '#t': ['electronics'] }]);
   ```

#### `t` Tag Filtering for Community-Specific Content

For applications focused on a specific community or niche, you can use `t` tags to filter events for the target audience.

**When to Use:**
- ✅ Community apps: "farmers" → `t: "farming"`, "Poland" → `t: "poland"`
- ❌ Generic platforms: Twitter clones, general Nostr clients

**Implementation:**
```typescript
import { publish } from '@welshman/app';

// Publishing with community tag
await publish({
  kind: 1,
  content: data.content,
  tags: [['t', 'farming']]
});

// Querying community content with Welshman
import { load } from '@welshman/app';

const events = await load([{
  kinds: [1],
  '#t': ['farming'],
  limit: 20
}]);
```

### Kind Ranges

An event's kind number determines the event's behavior and storage characteristics:

- **Regular Events** (1000 ≤ kind < 10000): Expected to be stored by relays permanently. Used for persistent content like notes, articles, etc.
- **Replaceable Events** (10000 ≤ kind < 20000): Only the latest event per pubkey+kind combination is stored. Used for profile metadata, contact lists, etc.
- **Addressable Events** (30000 ≤ kind < 40000): Identified by pubkey+kind+d-tag combination, only latest per combination is stored. Used for articles, long-form content, etc.

Kinds below 1000 are considered "legacy" kinds, and may have different storage characteristics based on their kind definition. For example, kind 1 is regular, while kind 3 is replaceable.

### Content Field Design Principles

When designing new event kinds, the `content` field should be used for semantically important data that doesn't need to be queried by relays. **Structured JSON data generally shouldn't go in the content field** (kind 0 being an early exception).

#### Guidelines

- **Use content for**: Large text, freeform human-readable content, or existing industry-standard JSON formats (Tiled maps, FHIR, GeoJSON)
- **Use tags for**: Queryable metadata, structured data, anything that needs relay-level filtering
- **Empty content is valid**: Many events need only tags with `content: ""`
- **Relays only index tags**: If you need to filter by a field, it must be a tag

#### Example

**✅ Good - queryable data in tags:**
```json
{
  "kind": 30402,
  "content": "",
  "tags": [["d", "product-123"], ["title", "Camera"], ["price", "250"], ["t", "photography"]]
}
```

**❌ Bad - structured data in content:**
```json
{
  "kind": 30402,
  "content": "{\"title\":\"Camera\",\"price\":250,\"category\":\"photo\"}",
  "tags": [["d", "product-123"]]
}
```

### NIP.md

The file `NIP.md` is used by this project to define a custom Nostr protocol document. If the file doesn't exist, it means this project doesn't have any custom kinds associated with it.

Whenever new kinds are generated, the `NIP.md` file in the project must be created or updated to document the custom event schema. Whenever the schema of one of these custom events changes, `NIP.md` must also be updated accordingly.

### Query Nostr Data with Welshman and TanStack Query

When querying Nostr with Welshman, combine `load()` from `@welshman/app` with `createQuery` from TanStack Query:

```svelte
<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { load } from '@welshman/app';

  function usePosts() {
    return createQuery({
      queryKey: ['posts'],
      queryFn: async (context) => {
        const signal = context.signal;
        const events = await load([{ kinds: [1], limit: 20 }], { signal });
        return events;
      },
    });
  }

  const posts = usePosts();
</script>

{#if $posts.isLoading}
  <div>Loading...</div>
{:else if $posts.error}
  <div>Error: {$posts.error.message}</div>
{:else if $posts.data}
  {#each $posts.data as post (post.id)}
    <Post event={post} />
  {/each}
{/if}
```

**Key Welshman Functions:**
- `load(filters, options)`: Load events from relays based on filters
- `publish(event)`: Publish an event to configured relays
- `subscribe(filters, callbacks)`: Subscribe to real-time events
- `count(filters)`: Count events matching filters

### Efficient Query Design

**Critical**: Always minimize the number of separate queries to avoid rate limiting and improve performance. Combine related queries whenever possible.

**✅ Efficient - Single query with multiple kinds:**
```typescript
import { load } from '@welshman/app';

// Query multiple event types in one request
const events = await load([
  {
    kinds: [1, 6, 16], // All repost kinds in one query
    '#e': [eventId],
    limit: 150,
  }
], { signal });

// Separate by type in JavaScript
const notes = events.filter((e) => e.kind === 1);
const reposts = events.filter((e) => e.kind === 6);
const genericReposts = events.filter((e) => e.kind === 16);
```

**❌ Inefficient - Multiple separate queries:**
```typescript
// This creates unnecessary load and can trigger rate limiting
const [notes, reposts, genericReposts] = await Promise.all([
  load([{ kinds: [1], '#e': [eventId] }], { signal }),
  load([{ kinds: [6], '#e': [eventId] }], { signal }),
  load([{ kinds: [16], '#e': [eventId] }], { signal }),
]);
```

**Query Optimization Guidelines:**
1. **Combine kinds**: Use `kinds: [1, 6, 16]` instead of separate queries
2. **Use multiple filters**: When you need different tag filters, use multiple filter objects in a single query
3. **Adjust limits**: When combining queries, increase the limit appropriately
4. **Filter in JavaScript**: Separate event types after receiving results rather than making multiple requests
5. **Consider relay capacity**: Each query consumes relay resources and may count against rate limits

### Reactive Queries with Svelte 5 Runes

With Svelte 5, queries automatically track reactive dependencies:

```svelte
<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { load } from '@welshman/app';
  
  let search = $state('');
  
  // Query automatically re-runs when search changes
  const results = createQuery(() => ({
    queryKey: ['search', search],
    queryFn: async () => {
      if (!search) return [];
      return await load([{
        kinds: [1],
        search, // NIP-50 search if relay supports it
        limit: 20
      }]);
    }
  }));
</script>

<input bind:value={search} placeholder="Search..." />

{#if $results.data}
  <div>Found {$results.data.length} results</div>
{/if}
```

### Event Validation

When querying events, if the event kind being returned has required tags or required JSON fields in the content, the events should be filtered through a validator function. This is not generally needed for kinds such as 1, where all tags are optional and the content is freeform text, but is especially useful for custom kinds as well as kinds with strict requirements.

```typescript
// Example validator function for NIP-52 calendar events
function validateCalendarEvent(event: NostrEvent): boolean {
  // Check if it's a calendar event kind
  if (![31922, 31923].includes(event.kind)) return false;

  // Check for required tags according to NIP-52
  const d = event.tags.find(([name]) => name === 'd')?.[1];
  const title = event.tags.find(([name]) => name === 'title')?.[1];
  const start = event.tags.find(([name]) => name === 'start')?.[1];

  // All calendar events require 'd', 'title', and 'start' tags
  if (!d || !title || !start) return false;

  // Additional validation for date-based events (kind 31922)
  if (event.kind === 31922) {
    const dateRegex = /^\d{4}-\d{2}-\d{2}$/;
    if (!dateRegex.test(start)) return false;
  }

  // Additional validation for time-based events (kind 31923)
  if (event.kind === 31923) {
    const timestamp = parseInt(start);
    if (isNaN(timestamp) || timestamp <= 0) return false;
  }

  return true;
}

function useCalendarEvents() {
  return createQuery({
    queryKey: ['calendar-events'],
    queryFn: async (context) => {
      const events = await load([{ kinds: [31922, 31923], limit: 20 }], { signal: context.signal });
      // Filter events through validator to ensure they meet NIP-52 requirements
      return events.filter(validateCalendarEvent);
    },
  });
}
```

### Profile Data with Welshman

To display profile data for a user by their Nostr pubkey:

```svelte
<script lang="ts">
  import type { NostrEvent } from '@welshman/util';
  import { createQuery } from '@tanstack/svelte-query';
  import { load } from '@welshman/app';
  import { genUserName } from '$lib/utils/genUserName';

  interface Props {
    event: NostrEvent;
  }
  
  let { event }: Props = $props();
  
  const author = createQuery(() => ({
    queryKey: ['author', event.pubkey],
    queryFn: async () => {
      const profiles = await load([{
        kinds: [0],
        authors: [event.pubkey],
        limit: 1
      }]);
      return profiles[0];
    }
  }));
  
  let metadata = $derived($author.data?.content ? JSON.parse($author.data.content) : undefined);
  let displayName = $derived(metadata?.name ?? genUserName(event.pubkey));
  let profileImage = $derived(metadata?.picture);
</script>

<div class="flex items-center gap-2">
  {#if profileImage}
    <img src={profileImage} alt={displayName} class="w-10 h-10 rounded-full" />
  {/if}
  <span>{displayName}</span>
</div>
```

### `NostrMetadata` type

```ts
/** Kind 0 metadata. */
interface NostrMetadata {
  /** A short description of the user. */
  about?: string;
  /** A URL to a wide (~1024x768) picture to be optionally displayed in the background of a profile screen. */
  banner?: string;
  /** A boolean to clarify that the content is entirely or partially the result of automation, such as with chatbots or newsfeeds. */
  bot?: boolean;
  /** An alternative, bigger name with richer characters than `name`. `name` should always be set regardless of the presence of `display_name` in the metadata. */
  display_name?: string;
  /** A bech32 lightning address according to NIP-57 and LNURL specifications. */
  lud06?: string;
  /** An email-like lightning address according to NIP-57 and LNURL specifications. */
  lud16?: string;
  /** A short name to be displayed for the user. */
  name?: string;
  /** An email-like Nostr address according to NIP-05. */
  nip05?: string;
  /** A URL to the user's avatar. */
  picture?: string;
  /** A web URL related in any way to the event author. */
  website?: string;
}
```

### Publishing Events with Welshman

To publish events, create an event template, sign it with the current signer, then publish using `publishEvent()` from `@welshman/app`:

```svelte
<script lang="ts">
  import { createMutation } from '@tanstack/svelte-query';
  import { publishEvent, pubkey, signer } from '@welshman/app';
  import { makeEvent } from '@welshman/util';

  let content = $state('');

  const publish = createMutation({
    mutationFn: async (noteContent: string) => {
      const currentSigner = $signer;
      if (!currentSigner) throw new Error('Not logged in');

      // Create event template
      const template = makeEvent(1, {
        content: noteContent,
        tags: [['client', 'your-app-name']]
      });

      // Sign the event
      const signedEvent = await currentSigner.sign(template);

      // Publish to relays (Router automatically selects relays)
      return await publishEvent(signedEvent);
    },
    onSuccess: () => {
      content = '';
      // Show success toast
    }
  });

  function handleSubmit(e: Event) {
    e.preventDefault();
    $publish.mutate(content);
  }
</script>

{#if $pubkey}
  <form onsubmit={handleSubmit}>
    <textarea bind:value={content} placeholder="What's on your mind?" />
    <button type="submit" disabled={$publish.isPending}>
      {$publish.isPending ? 'Publishing...' : 'Publish'}
    </button>
  </form>
{:else}
  <p>Please log in to publish</p>
{/if}
```

**Publishing with Router Scenarios:**

```typescript
import { Router } from '@welshman/router';
import { publishEvent, signer } from '@welshman/app';
import { makeEvent } from '@welshman/util';

// Publish to specific relay selection
const router = Router.get();
const template = makeEvent(1, { content: 'Hello Nostr!' });
const signedEvent = await $signer.sign(template);

// Router automatically determines best relays based on configuration
await publishEvent(signedEvent);

// Or manually specify relay scenario
const scenario = router.FromUser(); // User's write relays
const relays = scenario.getUrls();
// Use these relays with lower-level publish function if needed
```

### Nostr Login with Welshman

Welshman provides built-in login functions and session management through `@welshman/app`:

```typescript
import {
  loginWithNip07,
  loginWithNip46,
  loginWithNip01,
  loginWithPubkey,
  pubkey,
  session,
  signer
} from '@welshman/app';
import { get } from 'svelte/store';

// Login with browser extension (NIP-07)
export async function loginWithExtension() {
  if (!window.nostr) {
    throw new Error('No Nostr extension found');
  }

  const userPubkey = await window.nostr.getPublicKey();
  await loginWithNip07(userPubkey);

  // Current user is now available via stores
  console.log('Logged in:', get(pubkey));
}

// Login with nsecbunker (NIP-46)
export async function loginWithBunker(bunkerPubkey: string, relays: string[]) {
  const clientSecret = makeSecret(); // Generate ephemeral key
  const userPubkey = await getUserPubkeyFromBunker(bunkerPubkey);

  await loginWithNip46(userPubkey, clientSecret, bunkerPubkey, relays);
}

// Login with private key (NIP-01) - use carefully!
export async function loginWithPrivateKey(secret: string) {
  const userPubkey = getPubkey(secret);
  await loginWithNip01(secret);
}

// Read-only mode (no signing)
export async function loginReadOnly(userPubkey: string) {
  await loginWithPubkey(userPubkey);
}

// Check current session
export function getCurrentUser() {
  return {
    pubkey: get(pubkey),
    session: get(session),
    signer: get(signer)
  };
}

// Logout
export function logout() {
  clearSessions();
}
```

**Using in Components:**

```svelte
<script lang="ts">
  import { pubkey, signer } from '@welshman/app';

  // Auto-reactive to login state changes
  let isLoggedIn = $derived($pubkey !== undefined);
  let currentPubkey = $derived($pubkey);
</script>

{#if isLoggedIn}
  <p>Logged in as {currentPubkey}</p>
{:else}
  <button onclick={loginWithExtension}>Login</button>
{/if}
```

### LoginArea Component

To enable login with Nostr, use the `LoginArea` component:

```svelte
<script>
  import { LoginArea } from '$lib/components/auth/LoginArea.svelte';
</script>

<div>
  <!-- other components ... -->
  <LoginArea class="max-w-60" />
</div>
```

The `LoginArea` component handles all login-related UI and interactions. It displays "Log in" and "Sign Up" buttons when logged out, and an account switcher when logged in.

**Important**: Social applications should include a profile menu button in the main interface for access to account settings, profile editing, and logout functionality.

### `npub`, `naddr`, and other Nostr addresses

Nostr defines a set of bech32-encoded identifiers in NIP-19. Their prefixes and purposes:

- `npub1`: **public keys** - Just the 32-byte public key, no additional metadata
- `nsec1`: **private keys** - Secret keys (should never be displayed publicly)
- `note1`: **event IDs** - Just the 32-byte event ID (hex), no additional metadata
- `nevent1`: **event pointers** - Event ID plus optional relay hints and author pubkey
- `nprofile1`: **profile pointers** - Public key plus optional relay hints and petname
- `naddr1`: **addressable event coordinates** - For parameterized replaceable events (kind 30000-39999)
- `nrelay1`: **relay references** - Relay URLs (deprecated)

#### Key Differences Between Similar Identifiers

**`note1` vs `nevent1`:**
- `note1`: Contains only the event ID (32 bytes) - specifically for kind:1 events (Short Text Notes) as defined in NIP-10
- `nevent1`: Contains event ID plus optional relay hints and author pubkey - for any event kind
- Use `note1` for simple references to text notes and threads
- Use `nevent1` when you need to include relay hints or author context for any event type

**`npub1` vs `nprofile1`:**
- `npub1`: Contains only the public key (32 bytes)
- `nprofile1`: Contains public key plus optional relay hints and petname
- Use `npub1` for simple user references
- Use `nprofile1` when you need to include relay hints or display name context

#### NIP-19 Routing with SvelteKit

NIP-19 identifiers should be handled at the **root level** of URLs using SvelteKit's dynamic routing:

Create `/src/routes/[nip19]/+page.svelte`:

```svelte
<script lang="ts">
  import { page } from '$app/stores';
  import { nip19 } from 'nostr-tools';
  import { goto } from '$app/navigation';
  
  let decoded = $derived.by(() => {
    try {
      return nip19.decode($page.params.nip19);
    } catch {
      goto('/404');
      return null;
    }
  });
</script>

{#if decoded}
  {#if decoded.type === 'npub' || decoded.type === 'nprofile'}
    <!-- Profile view -->
    <ProfileView pubkey={decoded.type === 'npub' ? decoded.data : decoded.data.pubkey} />
  {:else if decoded.type === 'note'}
    <!-- Note view -->
    <NoteView eventId={decoded.data} />
  {:else if decoded.type === 'nevent'}
    <!-- Event view with relay hints -->
    <EventView eventId={decoded.data.id} relays={decoded.data.relays} />
  {:else if decoded.type === 'naddr'}
    <!-- Addressable event view -->
    <AddressableView data={decoded.data} />
  {:else}
    <p>Unsupported identifier type</p>
  {/if}
{/if}
```

#### Use in Filters

The base Nostr protocol uses hex string identifiers when filtering by event IDs and pubkeys. Nostr filters only accept hex strings.

```ts
// ❌ Wrong: naddr is not decoded
const events = await load([{ ids: [naddr] }]);
```

Corrected example:

```ts
// Import nip19 from nostr-tools
import { nip19 } from 'nostr-tools';
import { load } from '@welshman/app';

// Decode a NIP-19 identifier
const decoded = nip19.decode(value);

// Optional: guard certain types (depending on the use-case)
if (decoded.type !== 'naddr') {
  throw new Error('Unsupported Nostr identifier');
}

// Get the addr object
const naddr = decoded.data;

// ✅ Correct: naddr is expanded into the correct filter
const events = await load([{
  kinds: [naddr.kind],
  authors: [naddr.pubkey],
  '#d': [naddr.identifier],
}]);
```

#### Implementation Guidelines

1. **Always decode NIP-19 identifiers** before using them in queries
2. **Use the appropriate identifier type** based on your needs:
   - Use `note1` for kind:1 text notes specifically
   - Use `nevent1` when including relay hints or for non-kind:1 events
   - Use `naddr1` for addressable events (always includes author pubkey for security)
3. **Handle different identifier types** appropriately:
   - `npub1`/`nprofile1`: Display user profiles
   - `note1`: Display kind:1 text notes specifically
   - `nevent1`: Display any event with optional relay context
   - `naddr1`: Display addressable events (articles, marketplace items, etc.)
4. **Security considerations**: Always use `naddr1` for addressable events instead of just the `d` tag value, as `naddr1` contains the author pubkey needed to create secure filters
5. **Error handling**: Gracefully handle invalid or unsupported NIP-19 identifiers with 404 responses

### Edit Profile with Welshman

To include an Edit Profile form:

```svelte
<script lang="ts">
  import { createMutation } from '@tanstack/svelte-query';
  import { publishEvent, pubkey, signer } from '@welshman/app';
  import { makeEvent } from '@welshman/util';

  let name = $state('');
  let about = $state('');
  let picture = $state('');

  const updateProfile = createMutation({
    mutationFn: async (metadata: Record<string, string>) => {
      const currentSigner = $signer;
      if (!currentSigner) throw new Error('Not logged in');

      // Create kind 0 profile event
      const template = makeEvent(0, {
        content: JSON.stringify(metadata),
        tags: []
      });

      // Sign and publish
      const signedEvent = await currentSigner.sign(template);
      return await publishEvent(signedEvent);
    }
  });

  function handleSubmit(e: Event) {
    e.preventDefault();
    $updateProfile.mutate({ name, about, picture });
  }
</script>

{#if $pubkey}
  <form onsubmit={handleSubmit}>
    <input bind:value={name} placeholder="Name" />
    <textarea bind:value={about} placeholder="About" />
    <input bind:value={picture} placeholder="Picture URL" />
    <button type="submit" disabled={$updateProfile.isPending}>
      {$updateProfile.isPending ? 'Saving...' : 'Save Profile'}
    </button>
  </form>
{:else}
  <p>Please log in to edit profile</p>
{/if}
```

### Uploading Files with Welshman

Create a file upload utility using Blossom servers:

```svelte
<script lang="ts">
  import { createMutation } from '@tanstack/svelte-query';
  import { uploadToBlossom } from '$lib/utils/blossom';

  const uploadFile = createMutation({
    mutationFn: async (file: File) => {
      const tags = await uploadToBlossom(file);
      return tags[0][1]; // Return URL
    }
  });

  function handleFileChange(e: Event) {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    if (file) {
      $uploadFile.mutate(file);
    }
  }
</script>

<input 
  type="file" 
  onchange={handleFileChange}
  disabled={$uploadFile.isPending}
/>

{#if $uploadFile.isPending}
  <p>Uploading...</p>
{:else if $uploadFile.data}
  <img src={$uploadFile.data} alt="Uploaded" />
{/if}
```

To attach files to kind 1 events, each file's URL should be appended to the event's `content`, and an `imeta` tag should be added for each file.

### Nostr Encryption and Decryption

Use Welshman's signer interface for encryption and decryption:

```typescript
import { pubkey, signer } from '@welshman/app';
import { get } from 'svelte/store';

// Get current signer and pubkey
const currentSigner = get(signer);
const currentPubkey = get(pubkey);

if (!currentSigner?.nip44) {
  throw new Error("Please upgrade your signer to support NIP-44 encryption");
}

// Encrypt message to self
const encrypted = await currentSigner.nip44.encrypt(currentPubkey, "hello world");

// Decrypt message
const decrypted = await currentSigner.nip44.decrypt(currentPubkey, encrypted); // "hello world"

// In Svelte components, use stores directly:
// const encrypted = await $signer.nip44.encrypt($pubkey, "hello world");
```

### Rendering Rich Text Content

Use a `NoteContent` component powered by `@welshman/content`:

```svelte
<script lang="ts">
  import { parseContent } from '@welshman/content';
  import type { NostrEvent } from '@welshman/util';
  
  interface Props {
    event: NostrEvent;
    class?: string;
  }
  
  let { event, class: className = '' }: Props = $props();
  
  let parsed = $derived(parseContent(event.content));
</script>

<div class="whitespace-pre-wrap break-words {className}">
  {#each parsed as part}
    {#if part.type === 'text'}
      {part.value}
    {:else if part.type === 'link'}
      <a href={part.value} target="_blank" rel="noopener noreferrer" class="text-blue-500 hover:underline">
        {part.value}
      </a>
    {:else if part.type === 'mention'}
      <span class="text-purple-500">@{part.value}</span>
    {/if}
  {/each}
</div>
```

### Real-Time Subscriptions with Welshman

Welshman provides powerful subscription capabilities:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { subscribe } from '@welshman/app';
  import { writable } from 'svelte/store';

  const events = writable<NostrEvent[]>([]);

  onMount(() => {
    // Subscribe to real-time events
    const sub = subscribe([{ kinds: [1], limit: 20 }], {
      onEvent: (event) => {
        events.update(list => [event, ...list]);
      },
      onEose: () => {
        console.log('End of stored events');
      }
    });

    // Cleanup on unmount
    return () => sub.close();
  });
</script>

{#each $events as event (event.id)}
  <Post {event} />
{/each}
```

### Feed Compilation with `@welshman/feeds`

Welshman includes a powerful feed system:

```svelte
<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { FeedLoader } from '@welshman/feeds';
  import { getUserRelays } from '$lib/welshman/relays';

  let feedDefinition = $state({
    kinds: [1],
    authors: [], // Add followed pubkeys
    limit: 50
  });

  const feed = createQuery(() => ({
    queryKey: ['feed', feedDefinition],
    queryFn: async () => {
      const loader = new FeedLoader({
        filters: [feedDefinition],
        relays: getUserRelays()
      });
      
      return await loader.load();
    }
  }));
</script>

{#if $feed.data}
  {#each $feed.data as event (event.id)}
    <Post {event} />
  {/each}
{/if}
```

## App Configuration

The project uses SvelteKit's context API with Welshman for global state. Create a setup function and initialize in `+layout.svelte`:

```svelte
<script lang="ts">
  import { setContext, onMount } from 'svelte';
  import { writable } from 'svelte/store';
  import { setupRouter } from '$lib/welshman/setup';
  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';

  const theme = writable('light');
  const defaultRelays = ['wss://relay.damus.io', 'wss://relay.nostr.band', 'wss://purplepag.es'];

  setContext('app', { theme });

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 1000 * 60 * 5, // 5 minutes
      }
    }
  });

  onMount(() => {
    // Initialize Welshman Router with default relays
    setupRouter(defaultRelays);
  });
</script>

<QueryClientProvider client={queryClient}>
  <slot />
</QueryClientProvider>
```

**Note:** Welshman stores (`pubkey`, `signer`, `repository`, etc.) are global and accessible from `@welshman/app` anywhere in your application. You don't need to pass them through context.

Access Welshman stores directly in any component:

```svelte
<script lang="ts">
  import { pubkey, signer } from '@welshman/app';
  import { getContext } from 'svelte';

  const { theme } = getContext('app');
</script>

<div class:dark={$theme === 'dark'}>
  {#if $pubkey}
    <p>Logged in as {$pubkey.slice(0, 8)}...</p>
  {:else}
    <p>Not logged in</p>
  {/if}
</div>
```

## Routing

SvelteKit uses file-based routing. Routes are defined by the file structure in `/src/routes/`:

- `/src/routes/+page.svelte` → `/`
- `/src/routes/about/+page.svelte` → `/about`
- `/src/routes/post/[id]/+page.svelte` → `/post/123`
- `/src/routes/+layout.svelte` → Wraps all routes

**Data loading** with `+page.ts`:

```typescript
// /src/routes/post/[id]/+page.ts
import type { PageLoad } from './$types';
import { load } from '@welshman/app';

export const load: PageLoad = async ({ params }) => {
  const events = await load([{
    kinds: [1],
    ids: [params.id]
  }]);
  
  return { post: events[0] };
};
```

**Use data in component**:

```svelte
<!-- /src/routes/post/[id]/+page.svelte -->
<script lang="ts">
  import type { PageData } from './$types';
  
  let { data }: { data: PageData } = $props();
</script>

<h1>{data.post.content}</h1>
```

## Development Practices

- Uses TanStack Query with `createQuery` and `createMutation`
- Uses Welshman for all Nostr operations (networking, signing, storage)
- Follows shadcn-svelte component patterns
- Implements Path Aliases with `$lib/` prefix for library code
- Uses Vite for fast development and production builds
- Component-based architecture with Svelte 5 runes
- Battle-tested patterns extracted from Coracle
- Comprehensive provider setup with QueryClientProvider and SvelteKit context
- **Never use the `any` type**: Always use proper TypeScript types for type safety

## Loading States

**Use skeleton loading** for structured content (feeds, profiles, forms). **Use spinners** only for buttons or short operations.

```svelte
<script>
  import { Skeleton } from '$lib/components/ui/skeleton';
  import * as Card from '$lib/components/ui/card';
</script>

<Card.Root>
  <Card.Header>
    <div class="flex items-center space-x-3">
      <Skeleton class="h-10 w-10 rounded-full" />
      <div class="space-y-1">
        <Skeleton class="h-4 w-24" />
        <Skeleton class="h-3 w-16" />
      </div>
    </div>
  </Card.Header>
  <Card.Content>
    <div class="space-y-2">
      <Skeleton class="h-4 w-full" />
      <Skeleton class="h-4 w-4/5" />
    </div>
  </Card.Content>
</Card.Root>
```

### Empty States and No Content Found

When no content is found, display a minimalist empty state with the `RelaySelector` component:

```svelte
<script>
  import { RelaySelector } from '$lib/components/RelaySelector.svelte';
  import * as Card from '$lib/components/ui/card';
</script>

<div class="col-span-full">
  <Card.Root class="border-dashed">
    <Card.Content class="py-12 px-8 text-center">
      <div class="max-w-sm mx-auto space-y-6">
        <p class="text-muted-foreground">
          No results found. Try another relay?
        </p>
        <RelaySelector class="w-full" />
      </div>
    </Card.Content>
  </Card.Root>
</div>
```

## CRITICAL Design Standards

- Create breathtaking, immersive designs that feel like bespoke masterpieces, rivaling the polish of Apple, Stripe, or luxury brands
- Designs must be production-ready, fully featured, with no placeholders unless explicitly requested, ensuring every element serves a functional and aesthetic purpose
- Avoid generic or templated aesthetics at all costs; every design must have a unique, brand-specific visual signature that feels custom-crafted
- Headers must be dynamic, immersive, and storytelling-driven, using layered visuals, motion, and symbolic elements to reflect the brand's identity—never use simple "icon and text" combos
- Incorporate purposeful, lightweight animations for scroll reveals, micro-interactions (e.g., hover, click, transitions), and section transitions to create a sense of delight and fluidity

### Design Principles

- Achieve Apple-level refinement with meticulous attention to detail, ensuring designs evoke strong emotions (e.g., wonder, inspiration, energy) through color, motion, and composition
- Deliver fully functional interactive components with intuitive feedback states, ensuring every element has a clear purpose and enhances user engagement
- Use custom illustrations, 3D elements, or symbolic visuals instead of generic stock imagery to create a unique brand narrative; stock imagery, when required, must be sourced exclusively from Pexels (NEVER Unsplash) and align with the design's emotional tone
- Ensure designs feel alive and modern with dynamic elements like gradients, glows, or parallax effects, avoiding static or flat aesthetics
- Before finalizing, ask: "Would this design make Apple or Stripe designers pause and take notice?" If not, iterate until it does

### Avoid Generic Design

- No basic layouts (e.g., text-on-left, image-on-right) without significant custom polish, such as dynamic backgrounds, layered visuals, or interactive elements
- No simplistic headers; they must be immersive, animated, and reflective of the brand's core identity and mission
- No designs that could be mistaken for free templates or overused patterns; every element must feel intentional and tailored

### Interaction Patterns

- Use progressive disclosure for complex forms or content to guide users intuitively and reduce cognitive load
- Incorporate contextual menus, smart tooltips, and visual cues to enhance navigation and usability
- Implement drag-and-drop, hover effects, and transitions with clear, dynamic visual feedback to elevate the user experience
- Support power users with keyboard shortcuts, ARIA labels, and focus states for accessibility and efficiency
- Add subtle parallax effects or scroll-triggered animations to create depth and engagement without overwhelming the user

### Technical Requirements

- Curated color palette (3-5 evocative colors + neutrals) that aligns with the brand's emotional tone and creates a memorable impact
- Ensure a minimum 4.5:1 contrast ratio for all text and interactive elements to meet accessibility standards
- Use expressive, readable fonts (18px+ for body text, 40px+ for headlines) with a clear hierarchy; pair a modern sans-serif (e.g., Inter) with an elegant serif (e.g., Playfair Display) for personality
- Design for full responsiveness, ensuring flawless performance and aesthetics across all screen sizes (mobile, tablet, desktop)
- Adhere to WCAG 2.1 AA guidelines, including keyboard navigation, screen reader support, and reduced motion options
- Follow an 8px grid system for consistent spacing, padding, and alignment to ensure visual harmony
- Add depth with subtle shadows, gradients, glows, and rounded corners (e.g., 16px radius) to create a polished, modern aesthetic
- Optimize animations and interactions to be lightweight and performant, ensuring smooth experiences across devices

### Components

- Design reusable, modular components with consistent styling, behavior, and feedback states (e.g., hover, active, focus, error)
- Include purposeful animations (e.g., scale-up on hover, fade-in on scroll) to guide attention and enhance interactivity without distraction
- Ensure full accessibility support with keyboard navigation, ARIA labels, and visible focus states (e.g., a glowing outline in an accent color)
- Use custom icons or illustrations for components to reinforce the brand's visual identity

### Adding Fonts

To add custom fonts, follow these steps:

1. **Install a font package** using npm:

   **Any Google Font can be installed** using the @fontsource packages. Examples:
   - For Inter Variable: `@fontsource-variable/inter`
   - For Roboto: `@fontsource/roboto`
   - For Outfit Variable: `@fontsource-variable/outfit`
   - For Poppins: `@fontsource/poppins`
   - For Open Sans: `@fontsource/open-sans`

   **Format**: `@fontsource/[font-name]` or `@fontsource-variable/[font-name]` (for variable fonts)

2. **Import the font** in `src/routes/+layout.svelte`:
   ```typescript
   import '@fontsource-variable/inter';
   ```

3. **Update Tailwind configuration** in `tailwind.config.ts`:
   ```typescript
   export default {
     theme: {
       extend: {
         fontFamily: {
           sans: ['Inter Variable', 'Inter', 'system-ui', 'sans-serif'],
         },
       },
     },
   }
   ```

### Recommended Font Choices by Use Case

- **Modern/Clean**: Inter Variable, Outfit Variable, or Manrope
- **Professional/Corporate**: Roboto, Open Sans, or Source Sans Pro
- **Creative/Artistic**: Poppins, Nunito, or Comfortaa
- **Technical/Code**: JetBrains Mono, Fira Code, or Source Code Pro (for monospace)

### Theme System

The project includes a complete light/dark theme system using CSS custom properties. The theme can be controlled via:

- Context API with writable stores for theme switching
- CSS custom properties defined in `src/app.css`
- Automatic dark mode support with `.dark` class

### Color Scheme Implementation

When users specify color schemes:
- Update CSS custom properties in `src/app.css` (both `:root` and `.dark` selectors)
- Use Tailwind's color palette or define custom colors
- Ensure proper contrast ratios for accessibility
- Apply colors consistently across components (buttons, links, accents)
- Test both light and dark mode variants

### Component Styling Patterns

- Use `cn()` utility for conditional class merging
- Follow shadcn-svelte patterns for component variants
- Implement responsive design with Tailwind breakpoints
- Add hover and focus states for interactive elements

## Svelte-Specific Patterns

### Component Props

```svelte
<script lang="ts">
  import type { Snippet } from 'svelte';
  
  interface Props {
    title: string;
    count?: number;
    children?: Snippet;
  }
  
  let { title, count = 0, children }: Props = $props();
</script>

<div>
  <h1>{title}</h1>
  <p>Count: {count}</p>
  {#if children}
    {@render children()}
  {/if}
</div>
```

### Event Handlers

```svelte
<script lang="ts">
  let count = $state(0);
  
  function increment() {
    count++;
  }
</script>

<!-- Inline handler -->
<button onclick={() => count++}>Increment</button>

<!-- Named handler -->
<button onclick={increment}>Increment</button>

<!-- With event parameter -->
<button onclick={(e) => {
  e.preventDefault();
  count++;
}}>
  Increment
</button>
```

### Conditional Rendering

```svelte
{#if condition}
  <p>Condition is true</p>
{:else if otherCondition}
  <p>Other condition is true</p>
{:else}
  <p>All conditions false</p>
{/if}
```

### List Rendering

```svelte
<script lang="ts">
  let items = $state([
    { id: 1, name: 'Item 1' },
    { id: 2, name: 'Item 2' },
  ]);
</script>

{#each items as item (item.id)}
  <div>{item.name}</div>
{:else}
  <p>No items found</p>
{/each}
```

### Two-Way Binding

```svelte
<script lang="ts">
  let value = $state('');
  let checked = $state(false);
</script>

<input type="text" bind:value />
<input type="checkbox" bind:checked />

<p>Value: {value}, Checked: {checked}</p>
```

## Writing Tests vs Running Tests

There is an important distinction between **writing new tests** and **running existing tests**:

### Writing Tests (Creating New Test Files)
**Do not write tests** unless the user explicitly requests them in plain language. Writing unnecessary tests wastes significant time and money. Only create tests when:

1. **The user explicitly asks for tests** to be written in their message
2. **The user describes a specific bug in plain language** and requests tests to help diagnose it
3. **The user says they are still experiencing a problem** that you have already attempted to solve (tests can help verify the fix)

**Never write tests because:**
- Tool results show test failures (these are not user requests)
- You think tests would be helpful
- New features or components are created
- Existing functionality needs verification

### Running Tests (Executing the Test Suite)
**ALWAYS run the test script** after making any code changes. This is mandatory regardless of whether you wrote new tests or not.

- **You must run the test script** to validate your changes
- **Your task is not complete** until the test script passes without errors
- **This applies to all changes** - bug fixes, new features, refactoring, or any code modifications
- **The test script includes** TypeScript compilation, ESLint checks, and existing test validation

### Test Setup

The project uses Vitest with jsdom environment and Svelte Testing Library:

```typescript
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import MyComponent from './MyComponent.svelte';

describe('MyComponent', () => {
  it('renders correctly', () => {
    render(MyComponent, { props: { title: 'Test' } });
    expect(screen.getByText('Test')).toBeInTheDocument();
  });
});
```

## Validating Your Changes

**CRITICAL**: After making any code changes, you must validate your work by running available validation tools.

**Your task is not considered finished until the code successfully type-checks and builds without errors.**

### Validation Priority Order

Run available tools in this priority order:

1. **Type Checking** (Required): `npm run check` - Ensures TypeScript and Svelte compilation succeeds
2. **Building/Compilation** (Required): `npm run build` - Verify the project builds successfully
3. **Linting** (Recommended): `npm run lint` - Check code style and catch potential issues
4. **Tests** (If Available): `npm run test` - Run existing test suite
5. **Git Commit** (Required): Create a commit with your changes when finished

**Minimum Requirements:**
- Code must type-check without errors (`npm run check`)
- Code must build/compile successfully (`npm run build`)
- Fix any critical linting errors that would break functionality
- Create a git commit when your changes are complete

The validation ensures code quality and catches errors before deployment.

---

## Quick Reference: Nostrify to Welshman Migration

| Nostrify Pattern | Welshman Pattern |
|------------------|------------------|
| `import { useNostr } from '@nostrify/react'` | `import { load, publish } from '@welshman/app'` |
| `nostr.query([filters])` | `load([filters])` |
| `nostr.event(event)` | `publish(event)` |
| `nostr.relay('wss://...')` | Use Router for relay management |
| Custom hooks | TanStack Query + Welshman functions |
| Provider setup | Initialize Welshman context in layout |

## Welshman Advantages

- **Battle-tested**: Extracted from production Coracle client
- **Modular**: Use only the packages you need
- **Performance**: Optimized for real-time feeds and subscriptions
- **Svelte-native**: Designed for Svelte's reactivity model
- **Feature-rich**: Includes feeds, routing, content parsing, and more out of the box
- **Active development**: Maintained by Coracle team