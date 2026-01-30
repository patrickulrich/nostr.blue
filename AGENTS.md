# nostr.blue Agent Guidelines

## Git

**Use `git add -A`** before committing (not `git commit -a` which misses new files).

## Build

Run before committing:
```bash
cargo check                                                # Native
cargo clippy -- -D warnings                                # Native lints
cargo clippy --target wasm32-unknown-unknown -- -D warnings  # WASM lints (CI enforced)
cargo test
dx check
```

Local dev:
```bash
npm run dev     # Build assets + dx serve
npm run build   # Production build
```

## Project Overview

**nostr.blue** is a Nostr social client built with Rust + Dioxus, compiled to WebAssembly. Features include feeds, DMs, communities, zaps, Cashu ecash wallet, articles, wikis, podcasts, and more.

**Stack**: Dioxus 0.7 (reactive UI), rust-nostr SDK, CDK (Cashu), TailwindCSS 4

## Architecture

```
src/
├── main.rs           # Entry point
├── components/       # 90+ reusable UI components
├── routes/           # Pages + Layout (mod.rs)
├── stores/           # Global state (dioxus-stores)
├── services/         # External APIs
├── hooks/            # Custom hooks (use_*)
├── utils/            # Helpers + NIP implementations
├── context/          # App context providers
├── error.rs          # Error types (NostrBlueError)
└── js/               # JS interop
```

## Key Patterns

### Components
```rust
// src/components/my_component.rs
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
- Local: `use_signal(|| value)`
- Global: stores in `src/stores/`

### Icons
All in `src/components/icons.rs`:
```rust
SearchIcon { class: "w-5 h-5".to_string() }
```

## Protocol Documentation

**Read local docs FIRST** before MCP tools or web search:

| Protocol | Location | Description |
|----------|----------|-------------|
| NIPs | `docs/nips/` | Nostr specs (01-99+) |
| NUTs | `docs/nuts/` | Cashu specs (00-26) |
| BUDs | `docs/blossom/buds/` | Blossom media (00-10) |
| Market | `docs/market-spec/` | NIP-99 marketplace |
| NKBIPs | `docs/NKBIPs/` | nostr.blue extensions |

MCP tool `mcp__nostrbook__read_nip` available for quick NIP lookups.

## Styling (TailwindCSS 4)

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
