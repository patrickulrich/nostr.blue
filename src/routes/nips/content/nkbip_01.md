# NKBIP-01
## Minimum Specifications for Knowledge Base on Nostr

`draft` `mandatory` `author:liminal`

This NKBIP defines the minimum specification required for a nostr knowledge base  (KNB). A reference for event creation can be found [here](https://github.com/limina1/upload_files/blob/main/src/upload.ts)

## Kinds
The most basic article can be constructed through two kinds with an analog to the standard `kind 0` and `kind 1` defined in NIP-01 and influence taken from NIP-23 and NIP-51: 

- `30040`: **Article Header**: the `content` is set to a stringified JSON object `{title:<article title>, author:<string>, ...}` which describes the main title of the article and the name of the author of this article is. The author may be the "petname" of the original pubkey posting, or if remixing another existing article that exists somewhere, the original author of the content. Additionally, `kind:30040` is *REQUIRED* to have events listed in tags *in the order** that they appear in the article. The events may be any existing note on nostr (including nested kind 30040s).

```json
{
  "id": ...,
  "pubkey":...,
  "created_at": <unix timestamp in seconds>,
  "kind": 30040,
  "tags": [
      ['e', <event0-id>, <relay-url>],
      ['e', <event1-id>, <relay-url>],
      ['e', <event2-id>, <relay-url>],
        ...
    ],
    ...
  ],
  "content": `{'title':<article-title>, 'author':<string>, ...}`,
}
```

- `30041`: **Article Note**: MUST contain a `title` tag denoting the title of article section ("Introduction", "References", "1.0", etc). The `content` section MAY be Markdown syntax or plaintext. Being parametric replaceable events, they may also have a tag of `published at` to denote their original publications. Relays operators have the option to keep previous versions.

```json

  "kind": 30041,
  "created_at": ...,
  "content": "The simplest open protocol that is able to create a censorship-resistant global "social" network once and for all.\n\nIt doesn't rely on any trusted central server, hence it is resilient; it is based on cryptographic keys and signatures, so it is tamperproof; it does not rely on P2P techniques, and therefore it works."
  "tags": [
    ["title": "nostr - Notes and Other Stuff Transmitted by Relays"],
    ["published_at", ...],
  ],
  "pubkey": "...",
  "id": "..."

```
