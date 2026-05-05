# Fix Comment Thread Nesting

## Root Cause

`get_parent_id()` in `src/utils/parsing/thread_tree.rs:35-96` has **completely broken** NIP-10 marker parsing:

1. It calls `tag.content()` which returns only the event ID hex string (index 1 of the tag array)
2. It then does `split('\t')` on this single value, producing a Vec of length 1
3. The check `parts_vec.len() >= 3 && parts_vec[2] == "reply"` **never passes**
4. Falls back to `e_tags.last()` — but the nostr SDK's `text_note_reply()` puts `Marker::Root` as the LAST tag and `Marker::Reply` as the FIRST
5. Result: replies to comments always resolve parent = root note → appear at top level

### Secondary Bug

Cache invalidation at `src/components/reply_composer.rs:161-162` uses `target_event.id` (the comment being replied to) as the cache key, but the cache is keyed by the ROOT note's event ID. This is a no-op.

## Changes

### 1. Rewrite `get_parent_id()` — `src/utils/parsing/thread_tree.rs`

**Add imports:**
```rust
use nostr_sdk::nips::nip10::Marker;
use nostr_sdk::TagStandard;
```
(Remove `TagKind` if no longer needed — but it's still used in the uppercase E tag filter for NIP-22.)

**Replace the function body (lines 35-96) with:**

```rust
fn get_parent_id(event: &Event) -> Option<EventId> {
    let mut reply_marker_id = None;
    let mut root_marker_id = None;
    let mut last_unmarked_id = None;

    for tag in event.tags.iter() {
        if let Some(TagStandard::Event {
            event_id,
            marker,
            uppercase: false,
            ..
        }) = tag.as_standardized()
        {
            match marker {
                Some(Marker::Reply) => {
                    if reply_marker_id.is_none() {
                        reply_marker_id = Some(*event_id);
                    }
                }
                Some(Marker::Root) => {
                    if root_marker_id.is_none() {
                        root_marker_id = Some(*event_id);
                    }
                }
                None => {
                    last_unmarked_id = Some(*event_id);
                }
            }
        }
    }

    if reply_marker_id.is_some() {
        return reply_marker_id;
    }
    if root_marker_id.is_some() {
        return root_marker_id;
    }
    if last_unmarked_id.is_some() {
        return last_unmarked_id;
    }

    if event.kind == nostr_sdk::Kind::Comment {
        let upper_e_tags: Vec<_> = event
            .tags
            .iter()
            .filter(|tag| {
                tag.kind()
                    == TagKind::SingleLetter(nostr_sdk::SingleLetterTag::uppercase(
                        nostr_sdk::Alphabet::E,
                    ))
            })
            .collect();
        if let Some(first_tag) = upper_e_tags.first() {
            if let Some(TagStandard::Event { event_id, .. }) = first_tag.as_standardized() {
                return Some(*event_id);
            }
        }
    }
    None
}
```

**Priority order** (matches wisp and amethyst):
1. `Marker::Reply` → the comment being replied to
2. `Marker::Root` → direct reply to root
3. Last unmarked `e` tag → legacy positional fallback
4. Uppercase `E` tag → NIP-22 comment fallback

### 2. Fix cache invalidation — `src/components/reply_composer.rs:161-162`

**Current (broken):**
```rust
if let Ok(root_event_id) = EventId::from_hex(&target_event.id.to_hex()) {
    invalidate_thread_tree_cache(&root_event_id);
}
```

**Fixed:**
```rust
if let Some(ref r) = root {
    invalidate_thread_tree_cache(&r.id);
} else {
    invalidate_thread_tree_cache(&target_event.id);
}
```

Note: Inside the `spawn(async move { ... })` block, the prop `root_event` is cloned into a local variable `root` (line 92). We must use `root`, not `root_event`. The `ref` binding is needed since `root` is owned by the async block and was already used at line 110 via `root.as_ref()` (a borrow that doesn't consume).

When `root` is `Some`, use its ID (the actual root note). When `None`, the target IS the root (direct reply to root note), so use `target_event.id`.

## Files Modified

| File | Lines | Change |
|------|-------|--------|
| `src/utils/parsing/thread_tree.rs` | 1-3, 35-96 | Add imports, rewrite `get_parent_id()` |
| `src/components/reply_composer.rs` | 161-162 | Fix cache invalidation key |

## Validation Notes

### SDK API paths (confirmed in nostr crate source)
- `TagStandard::Event { event_id, relay_url, marker, public_key, uppercase }` — `crates/nostr/src/event/tag/standard.rs:47-55`
- `Tag::as_standardized()` returns `Option<&TagStandard>` — `crates/nostr/src/event/tag/mod.rs:160`
- `Marker` enum: `Root`, `Reply` only — `crates/nostr/src/nips/nip10.rs:33-39`
- `nostr_sdk::nips::nip10::Marker` accessible — confirmed in `src/stores/nostr_client/types.rs:160`
- `nostr_sdk::TagStandard` re-exported — `crates/nostr/src/lib.rs:55`
- `Tag::content()` returns `buf[1]` (single string, NOT tab-separated) — `crates/nostr/src/event/tag/mod.rs:145`

### NIP-10 priority order (matches wisp + amethyst + NIP spec)
1. `Marker::Reply` → direct parent (the comment being replied to)
2. `Marker::Root` → direct reply to root (when no Reply marker exists)
3. Last unmarked `e` tag → legacy positional fallback (pre-NIP-10)

### Edge cases covered
- **Reply to root note**: `text_note_reply()` with `root == None` → only Reply marker → `get_parent_id()` returns reply target (= root) → appears at top level ✓
- **Reply to root note (root == reply_to)**: Only Root marker added (no Reply) → `get_parent_id()` returns root_marker_id → top level ✓
- **Reply to comment**: Reply marker FIRST, Root marker LAST → `get_parent_id()` returns reply_marker_id → nested under comment ✓
- **Legacy tags without markers**: Falls through to `last_unmarked_id` → matches wisp/amethyst behavior ✓
- **NIP-22 comments (Kind 1111)**: Uppercase E tag fallback preserved ✓

## Verification

```bash
cargo check --target wasm32-unknown-unknown
cargo clippy --target wasm32-unknown-unknown -- -D warnings
cargo check --no-default-features --features desktop
cargo clippy --no-default-features --features desktop -- -D warnings
cargo test
```
