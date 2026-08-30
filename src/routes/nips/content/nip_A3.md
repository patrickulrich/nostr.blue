# NIP-A3 — Payment Targets (`payto`)

nostr.blue implements NIP-A3 (draft): alternative payment addresses
declared on user profiles via `payto` tags on a replaceable kind 10133
event.

## Kind 10133 — Payment Targets

Each target is a `["payto", "<type>", "<address>"]` tag. The type is
lowercase; the address is an on-chain address, a lightning address, a
custodial handle, or any other network-specific identifier. The content
field is normally empty, and nostr.blue preserves any existing content
and unrelated tags when republishing (read-modify-write) and includes a
NIP-31 `alt` tag.

## Types

The canonical types from the NIP's commonly-used table are recognized
with rich rendering: `bip352`, `bip353`, `bitcoin`, `cashme`, `ethereum`,
`lightning`, `litecoin`, `monero`, `nano`, `paypal`, `revolut`, `solana`,
`venmo`, `zcash`. Common ticker aliases and alternate spellings observed
on the network (`btc`, `onchain`, `ln`, `lnurl`, `eth`, `xmr`, `zec`,
`ltc`, `xno`, `sol`, `cashapp`, …) are canonicalized to these on parse
and publish.

Unrecognized types are rendered generically (wallet icon, copy, and the
RFC-8905 `payto://<type>/<address>` fallback URI) rather than dropped —
the NIP's own example renders an unknown type.

## URI policy

- Native schemes where broadly deployed: `bitcoin:`, `lightning:`,
  `monero:`, `ethereum:`, `litecoin:`, `zcash:`, `nano:`, `solana:`.
- BIP-353 payment-instruction handles are handed off as `bitcoin:`
  URIs.
- Custodial handles open the provider's payment page
  (`https://cash.app/$…`, `https://venmo.com/…`,
  `https://paypal.me/…`, `https://revolut.me/…`).
- BIP-352 silent-payment codes have no registered URI scheme; they are
  shared by copying or scanning the raw code.
- Unknown types fall back to `payto://<type>/<address>` (RFC-8905).

## Surfaces

- **Profile chips**: one pill per target under the bio/website block
  (plus a lightning chip for the kind-0 lightning address). Click opens
  the payment — lightning-family targets route through the zap flow,
  everything else hands off to the platform URI opener with a
  "No payment app found" error when the OS has no handler. Long-press
  (touch) or right-click (desktop) copies the address.
- **Profile information modal**: every declared address listed with a
  copy button.
- **Zap modal**: method chips offer the declared generic targets
  alongside Lightning; a declared lightning target is used as the zap
  recipient when the profile has no kind-0 lightning address. Generic
  methods render a QR of the preferred URI, a copyable address, and an
  "Open in …" handoff.
- **Editor**: a "Payment Addresses" section in the profile editor —
  one row per method (curated type dropdown plus a custom free-text
  type), per-type address validation with method-named errors, saved
  with the profile (kind 0 + kind 10133 together through the publish
  queue; the replaceable kind coalesces queued updates).

## Fetching

Kind 10133 is fetched per-author, gossip-routed to the author's write
relays (`limit: 1`, latest wins), gated on signer/relay readiness for
authenticated users, and cached with a 10-minute freshness window
shared across surfaces.

## References

- NIP-A3: `A3.md` in the nostr-protocol/nips repository.
- RFC 8905: The 'payto' URI Scheme.
- BIP-352: Silent Payments; BIP-353: Bitcoin Payment Instructions.
