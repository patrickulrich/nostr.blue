# Code Review Round 12: Verification and Remediation

## Context

~25 code review findings verified against current codebase and validated against nostr SDK (`/home/patrick/nostr`) and Dioxus framework (`/home/patrick/dioxus`). **5 findings require fixes** and ~20 are rejected.

---

## Findings to FIX (5)

### Fix 1: player.rs — Use platform timer in non-web branch
**File:** `src/components/music/player.rs:202`
**Issue:** Non-web branch calls `tokio::time::sleep(std::time::Duration::from_millis(500)).await` directly, breaking the platform abstraction. Web branch (line 198) correctly uses `crate::platform::timer::sleep_ms(500).await`.
**Validation:** Nostr SDK uses `#[cfg(...)]` gates with platform-specific time crates (`instant` for WASM vs `std::time` for native) in `types/time/supplier.rs`. Dioxus uses `web_time::Instant` for cross-platform timing. Both validate the pattern of using a platform abstraction rather than calling `tokio` directly.
**Fix:** Replace:
```rust
#[cfg(not(feature = "web"))]
{
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
}
```
With:
```rust
#[cfg(not(feature = "web"))]
{
    crate::platform::timer::sleep_ms(500).await;
}
```

### Fix 2: wavlake.rs — Safe UTF-8 truncation in debug log
**File:** `src/services/wavlake.rs:282`
**Issue:** `&body[..body.len().min(200)]` slices by byte index, which panics if a multi-byte UTF-8 character straddles the 200-byte boundary.
**Validation:** Nostr SDK uses `String::from_utf8_lossy()` for safe byte-to-string conversion (`nip19.rs:456`). Dioxus uses `chars().take_while()` for safe UTF-8 iteration (`autofmt/src/indent.rs`). Both avoid raw byte slicing on strings.
**Fix:** Replace:
```rust
log::debug!("LNURL error body (status {}): {}", status, &body[..body.len().min(200)]);
```
With:
```rust
let truncated: String = body.chars().take(200).collect();
log::debug!("LNURL error body (status {}): {}", status, truncated);
```

### Fix 3: zap_dialog.rs — Log discarded invoice on non-web
**File:** `src/components/music/zap_dialog.rs:217`
**Issue:** `let _ = inv;` silently discards the invoice in the non-web branch. If this code path is ever hit on native, there's zero visibility.
**Validation:** Nostr SDK prefers explicit handling over silent discards — uses `unwrap_or_default()` or error types (`types/time/supplier.rs`). Dioxus uses compile-time `#[cfg]` gates for unsupported platforms (silent no-ops), but those paths don't compile at all. This is a *runtime* path that compiles for both targets, so `log::warn!` is appropriate per nostr SDK convention.
**Fix:** Replace:
```rust
#[cfg(not(feature = "web"))]
let _ = inv;
```
With:
```rust
#[cfg(not(feature = "web"))]
log::warn!("WebLN invoice payment not supported on native: {:?}", inv);
```

### Fix 4: ics.rs — Replace expect() with graceful error handling in download_ics
**File:** `src/utils/audio/ics.rs:403-421`
**Issue:** Five `expect()` calls that will panic if browser APIs fail. This is web-only code, but panics in WASM crash the entire app.
**Validation:** Nostr SDK avoids `expect()`/`unwrap()` in non-test code — uses `.ok_or()`, `let...else`, and `?` operator (`message/relay.rs:268`, `message/client.rs:202`). Dioxus uses `expect()` for `window()`/`document()` only in core DOM setup (`web/src/document.rs:172`), but uses `.ok().context()` for fallible operations (`web/src/files.rs:62`). Since `download_ics` is a user-triggered action (not core setup), the `let...else` + `log::error!` pattern follows nostr SDK convention.
**Fix:** Replace panicking `expect()` calls with `let Some(...) = ... else { log + return }` and `let Ok(...) = ... else { log + return }` pattern throughout the function, using `log::error!` for each failure. Also call `revoke_object_url` only after a successful `create_object_url`.

### Fix 5: calendar_event_new.rs — Add upper bound to timestamp_to_date_time loop
**File:** `src/routes/events/calendar_event_new.rs:1005-1019`
**Issue:** The year-iteration loop has no upper bound. A malformed timestamp causes unbounded iteration.
**Validation:** Nostr SDK uses the exact same constant in `types/time/mod.rs:152`: `if timestamp >= 253_402_300_800 { return String::from("Unavailable"); }` for year 9999 bounds checking. Also uses `saturating_add`/`saturating_sub` for all timestamp arithmetic (lines 266-307). Dioxus uses `.clamp()`, `.min()`, `.saturating_sub()` throughout (`cli/src/serve/output.rs`, `desktop/src/app.rs`).
**Fix:** Add timestamp clamp at function entry:
```rust
let ts = ts.min(253_402_300_799); // Cap at 9999-12-31T23:59:59Z
```

---

## Findings REJECTED (~20)

| # | File | Finding | Rejection Reason |
|---|------|---------|-----------------|
| 1 | Dioxus.toml | Add .icns/.ico/multiple PNG sizes | Icon generation needs Tauri CLI tooling — not a code bug. Separate task |
| 2 | zap_dialog.rs:366 | Check window.open() result | Fire-and-forget for `lightning:` URL is acceptable |
| 3 | download.rs | save_file returns Ok(()) on web | Web delegates to `download_blob`; mobile already logs eval failures (line 29-31) |
| 4 | download.rs | Rename _mime_type | Parameter IS used on web (line 6) and mobile (line 23). Underscore avoids desktop unused-var warning |
| 5 | download.rs | Mobile double-JSON-encode | Comment on line 20 explains: `serde_json::to_string` on `&str` produces a valid JS string literal. Correct approach |
| 6 | build-android.sh | --release vs assembleDebug | Already rejected in prior review (#7984). Dev script — debug APK is intentional |
| 7 | build-android.sh | ls for NDK discovery | Works for version-named directories (no spaces). Low-risk shell idiom |
| 8 | player.rs | Generation counter for seek | JS `Math.abs(...) > 0.5` guard (line 186) prevents redundant seeks. Race only affects `is_seeking` flag timing |
| 9 | pin/home.rs | u64 → u32 | Already uses `wrapping_add(1)`. u64 is fine and consistent with `oldest_timestamp` |
| 10 | ics.rs | Move inline chrono imports | Code style preference, not a bug |
| 11 | ics.rs | trim_end_matches strips multiple Z's | Invalid input would fail `parse_from_str` anyway. No real-world risk |
| 12 | timed_serializer.rs | Abort handle instead of spawn-per-call | Generation counter prevents stale callbacks. Dioxus `spawn` doesn't easily support abort handles |
| 13 | main.rs | Add target_arch to logger cfg | Already rejected prior review (#8154). Features are mutually exclusive; `try_init()` prevents double-init |
| 14 | android_signer.rs | Cache JNI thread attachment | Already rejected prior review (#8154). Permanent attachment leaks |
| 15 | android_signer.rs | spawn_blocking for JNI | Architectural change beyond scope. Mobile doesn't have multi-threaded tokio |
| 16 | android_signer.rs | debug_assert on args.len() | Comments already document cases; catch-all `_` logs error |
| 17 | http.rs | WASM timeout | Comment documents design: WASM uses browser fetch timeout. reqwest timeout unavailable on WASM |
| 18 | calendar_event_new.rs | Replace Function::new_with_args | CSP `unsafe-eval` already required for Dioxus `eval()`. Current try/catch is defensive |
| 19 | podcast_index.rs | Web timeout wrapper | Same as #17 — browser-managed by design |
| 20 | videos_live_tag.rs | Subtract 1 from cursor | Dedup filter (lines 92-98) prevents duplicate display. One wasted slot in limit(50) |

---

## Verification

After making all 5 fixes:
```bash
cargo check
cargo clippy -- -D warnings
cargo clippy --target wasm32-unknown-unknown -- -D warnings
cargo test
```
