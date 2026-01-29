# Kind 30817: Custom NIP Proposals

Custom NIPs are community-driven event types for proposing and discussing protocol extensions. This event type uses the addressable event pattern (NIP-33) for deduplication and updates.

## Event Schema

| Field | Value |
|-------|-------|
| **kind** | `30817` (addressable, parameterized replaceable) |
| **content** | Markdown body of the NIP proposal |

## Required Tags

| Tag | Description |
|-----|-------------|
| `d` | Unique identifier for this NIP (e.g., `"my-custom-nip"` or UUID). Cannot be empty. |
| `title` | Human-readable title of the proposal |

## Optional Tags

| Tag | Description |
|-----|-------------|
| `k` | Related event kind(s) this NIP affects. Can appear multiple times. |
| `alt` | NIP-31 fallback description for clients that don't support this kind |

## Example Event

```json
{
  "kind": 30817,
  "content": "# My Custom NIP\n\nThis NIP defines a new event type for...\n\n## Specification\n\n...",
  "tags": [
    ["d", "my-custom-nip"],
    ["title", "My Custom NIP"],
    ["k", "1"],
    ["k", "6"],
    ["alt", "Custom NIP proposal: My Custom NIP"]
  ],
  "pubkey": "<author-pubkey>",
  "created_at": 1234567890,
  "id": "<event-id>",
  "sig": "<signature>"
}
```

## Addressing (naddr)

As an addressable event, kind 30817 events can be referenced using NIP-19 `naddr` encoding:

- **Coordinate**: `(kind=30817, pubkey, d-tag)`
- **Updates**: Publishing a new event with the same `d` tag replaces the previous version

## References

- [NIP-33: Parameterized Replaceable Events](https://github.com/nostr-protocol/nips/blob/master/33.md)
- [NIP-31: Alt tag for unknown event kinds](https://github.com/nostr-protocol/nips/blob/master/31.md)
- [NIP-19: bech32-encoded entities](https://github.com/nostr-protocol/nips/blob/master/19.md)
