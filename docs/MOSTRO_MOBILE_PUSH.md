# Mobile Push Notifications: Status & Path Forward

## Current State

**Web (WASM)**: ✅ Full Web Push support via the browser's Push API + service worker.
- VAPID key fetched from `/api/vapid` on the configured Mostro push server.
- Token registered via `POST /api/register` on app login.
- Background events surface as OS-level notifications via `public/sw.js`.
- Peer wake (`POST /api/notify`) fires after each P2P chat message send.

**Desktop (Linux)**: ❌ Not supported. Returns `None` from `acquire_push_token`.
Desktop users rely on `notify-rust` foreground notifications + the 60s
visibility-backfill poll.

**Mobile (Android via WebView)**: ❌ **DEFERRED — see below.**

`src/services/mostro_push.rs::acquire_push_token` returns `None` on
non-web targets. The registration / peer-wake HTTP calls are no-ops on
mobile because there's no token to register.

## Impact

Android users will **not** receive OS-level notifications for Mostro
events that arrive while the WebView is suspended (app backgrounded,
device asleep, etc.). Specifically:

- `PayInvoice` (the seller's hold invoice to pay)
- `PayBondInvoice` (anti-abuse bond)
- `AddInvoice` (buyer payout invoice request)
- `FiatSentOk` (counterparty marked fiat sent)
- `Released` / `HoldInvoicePaymentSettled` (sats released)
- `DisputeInitiatedByPeer` (counterparty opened a dispute)
- `AdminTookDispute` (solver assigned)
- `BondSlashed` (your bond was slashed)
- Incoming P2P trade chat messages
- Incoming dispute chat messages

### Mitigations in place

1. **60s visibility-backfill poll** (`src/components/mostro_toast_drainer.rs`):
   every 60 seconds while the app is open, runs
   `mostro::client::backfill_active_trades` if there are active trades.
   Catches up on any events missed while the tab was hidden.
2. **Foreground toasts** via `MOSTRO_BACKGROUND_TOASTS` queue: any event
   that arrives while the user is on a non-trade route surfaces as a
   toast at the app level.
3. **Persistent notification history** (`src/stores/mostro/notification_store.rs`):
   B2 — every notification that fires (whether the user sees it or not)
   is persisted to a 200-entry ring buffer + NIP-78 kind 30078 record,
   syncable across devices. The user can review what they missed at
   `/mostro/notifications`.

These mitigations cover the case where the user returns to the app
within 60 seconds. They do **not** cover long-backgrounded scenarios
(e.g., user backgrounds the app for an hour during a trade).

## Why It's Deferred

The user requested Firebase Cloud Messaging (FCM) integration be
deferred — see the original plan review. The Mostro push server
already speaks the FCM protocol for Android targets (its
`/api/register` endpoint accepts `platform: "android"` and routes
accordingly), so the gap is purely client-side:

- No Firebase project / `google-services.json` configured
- No `FirebaseMessagingService` Kotlin service in `android/kotlin/`
- No JNI bridge between Kotlin and Rust's `acquire_push_token`
- No foreground local-notification fallback for mobile
  (`src/stores/mostro/notifications.rs:13` stub)

## Path Forward — Options

### Option A: Firebase Cloud Messaging (FCM)
- **Effort**: ~2–3 days
- **Pros**: Direct compatibility with the Mostro push server (it already
  speaks FCM for `platform: "android"`). Matches the Mobile reference
  client's approach (`lib/services/fcm_service.dart`).
- **Cons**: Requires a Firebase project + `google-services.json`. Google
  dependency. ~30 MB extra APK size.
- **Implementation plan**: see the B1 section of the original gap-closure
  plan (Kotlin `FirebaseMessagingService` + JNI bridge + Rust-side
  `acquire_fcm_token`). Decide between Kotlin-side or Rust-side `fcm`
  crate based on maintenance status at implementation time.

### Option B: UnifiedPush / ntfy.sh
- **Effort**: ~3–4 days
- **Pros**: Google-free. Self-hostable. Growing adoption in the
  Bitcoin/Nostr community.
- **Cons**: Requires changes to the Mostro push server (currently only
  speaks FCM for Android). Users would need to install a distributor app
  (e.g., ntfy) — extra UX step.

### Option C: Aggressive polling
- **Effort**: ~0.25 day (tweak the existing 60s poll)
- **Cons**: Battery cost on mobile. Even at 15s polling, may miss
  time-sensitive events (e.g., 5-min hold-invoice payment window).
- **Recommendation**: not a substitute for true push; could be a
  stopgap alongside Option A or B.

### Option D: Stay deferred, document for users
- The mitigations above (notification history, foreground toasts,
  backfill poll) make the gap tolerable for desktop-class usage patterns
  (users who keep the app open or return frequently).
- For Android, the in-app Settings → Mostro page should surface a clear
  warning: "Push notifications are not supported on Android yet. Keep
  the app in the foreground or check the notifications page periodically
  during active trades."

## Reference Implementations

- **Mobile (Flutter)**: `lib/services/fcm_service.dart`,
  `lib/services/push_notification_service.dart`,
  `lib/background/mobile_background_service.dart`,
  `lib/features/notifications/services/background_notification_service.dart`.
  Uses `firebase_messaging` + `flutter_background_service` +
  `flutter_local_notifications`.
- **mostro-cli**: relies on terminal foreground, no push.
- **mostrix**: same as mostro-cli.

## Decision Tracking

When this is revisited, also implement:
- D7 — Mobile foreground local notifications via JNI to Android
  NotificationManager (works without push, complements Option A or B).
- The `notifications.rs:13` mobile stub currently logs but doesn't
  surface foreground local notifications.
