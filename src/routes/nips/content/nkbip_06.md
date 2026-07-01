# NKBIP-06
## Nostr MIME Types

`draft` `optional` `author:liminal` `depends: NIP-94`

This NEP defines the MIME types for Nostr events, so that they can be quickly found or recognized by content type, rather than by kind number.

In addition to the existing `m` tag, as defined in the referenced NIP, we are adding a `M` tag, for this new use case.

The `m` tag contains the standard *type/subtype* information, as listed in the [MIME documentation](https://developer.mozilla.org/en-US/docs/Web/HTTP/MIME_types).

The `M` tag contains the Nostr-specific categorization of the event, in the format category/use-case/replaceability.

Examples:

```json
{
  "id": "<event id>",
  "kind": 1,
  "pubkey": "<user pubkey>",
  "created_at": 1234567890,
  "tags": [
    ["m", "text/plain"],
    ["M", "note/microblog/nonreplaceable"]
  ],
  "content": "This is a typical kind 01 note with a short message.",
  "sig": "<signature matching user pubkey>"
}
```

```json
{
  "id": "<event id>",
  "kind": 1111,
  "pubkey": "<user pubkey>",
  "created_at": 1234567890,
  "tags": [
    ["m", "text/plain"],
    ["M", "note/comment/nonreplaceable"],
    ["E", "768ac8720cdeb59227cf95e98b66560ef03d8bc9a90d721779e76e68fb42f5e6", "wss://example.relay", "3721e07b079525289877c366ccab47112bdff3d1b44758ca333feb2dbbbbe5bb"],
    ["K", "1063"],
    ["P", "3721e07b079525289877c366ccab47112bdff3d1b44758ca333feb2dbbbbe5bb"],
    ["e", "768ac8720cdeb59227cf95e98b66560ef03d8bc9a90d721779e76e68fb42f5e6", "wss://example.relay", "3721e07b079525289877c366ccab47112bdff3d1b44758ca333feb2dbbbbe5bb"],
    ["k", "1063"],
    ["p", "3721e07b079525289877c366ccab47112bdff3d1b44758ca333feb2dbbbbe5bb"]
  ],
  "content": "This is a typical generic-reply note with a short message.",
  "sig": "<signature matching user pubkey>"
}
```

```json
{
  "id": "<event id>",
  "kind": 30040,
  "pubkey": "<user pubkey>",
  "created_at": 1234567890,
  "tags": [
    ["m", "application/json"],
    ["M", "meta-data/index/replaceable"],
    ["d", "aesops-fables-by-aesop"],
    ["title", "Aesop's Fables"],
    ["author", "Aesop"],
    ["i", "isbn:9780765382030"],
    ["t", "fables"],
    ["t", "classical"],
    ["t", "literature"],
    ["published_on", "2003-05-13"],
    ["published_by", "public domain"],
    ["image", "https://imageserver.com/piclink.jpg"],
    ["summary", "Collection of selected fables from the ancient Greek philosopher, known as Aesop."],
    ["a", "30041:<user pubkey>:aesops-fables-by-aesop-chapter-1", "wss://examplerelay.com", "<event id>"],
    ["a", "30041:<user pubkey>:aesops-fables-by-aesop-chapter-2", "wss://examplerelay.com", "<event id>"],
    ["auto-update", "ask>"],
    ["p", "<pubkey_0>"],
    ["E", "<original_event_id>", "wss://examplerelay.com", "<pubkey>"]
  ],
  "content": "",
  "sig": "<signature matching user pubkey>"
}
```

```json
{
  "id": "<event id>",
  "kind": 30041,
  "pubkey": "<user pubkey>",
  "created_at": 1234567890,
  "tags": [
    ["m", "application/json"],
    ["M", "article/publication-content/replaceable"],
    ["title", "The Farmer and The Snake"],
    ["d", "aesop's-fables-by-aesop-the-farmer-and-the-snake"],
    ["wikilink", "fable", "<pubkey>", "wss://thecitadel.nostr1.com", "<event id>"]
  ],
  "content": "The Farmer and The Snake\nA link:wikilink:fable[fable], by Aesop.\nONE WINTER a Farmer found a Snake stiff and frozen with cold. He had compassion on it, and taking it up, placed it in his bosom. The Snake was quickly revived by the warmth, and resuming its natural instincts, bit its benefactor, inflicting on him a mortal wound. 'Oh,' cried the Farmer with his last breath, 'I am rightly served for pitying a scoundrel.'\nThe greatest kindness will not bind the ungrateful.",
  "sig": "<signature matching user pubkey>"
}
```
