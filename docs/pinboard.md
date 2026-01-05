 NIP-XX
 ======

 Pinboards
 ---------

 `draft` `optional`

 This NIP defines a Pinterest-style pinboard system for Nostr, enabling users to create visual collections of curated content. Pinboards use a two-event architecture that separates board metadata from individual pins, allowing for mixed-content curation, collaborative boards, cross-board pinning, and profile-level pins.

 ## Motivation

 NIP-51 defines list kinds for content curation, but these have structural limitations that Pinboards address.

 ### Content-Type Segregation

 NIP-51 defines separate curation set kinds for specific content types:

 | Kind | Content Type | Expected Tags |
 |------|--------------|---------------|
 | 30004 | Articles/notes | `a` (kind:30023), `e` (kind:1) |
 | 30005 | Videos | `e` (kind:21) |
 | 30006 | Pictures | `e` (kind:20) |

 This forces users to maintain separate lists for different media. A "Japan Trip 2024" collection requires:
 - A kind 30006 set for photos
 - A kind 30005 set for videos
 - A kind 30004 set for blog articles
 - A kind 30003 set for external links

 **Pinboards allow heterogeneous content in a single collection.** One "Japan Trip 2024" pinboard can contain photos, videos, articles, notes, external links, books, podcasts—organized together thematically.

 ### Additional NIP-51 Limitations

 | Limitation | NIP-51 Lists | Pinboards |
 |------------|--------------|-----------|
 | Content types | Segregated by kind | Mixed in one board |
 | Item storage | All embedded in one event | Separate events per pin |
 | Collaboration | Single author only | Optional multi-author |
 | Cross-posting | Items belong to one list | Pins reference multiple boards |
 | Attribution | List owner only | Per-pin author tracking |
 | Scalability | Limited by event size | Unlimited pins |

 ## Event Kinds

 ### Kind 30067: Pinboard

 A parameterized replaceable event containing board metadata. Pins are stored separately.

 **Required Tags:**

 | Tag | Description |
 |-----|-------------|
 | `d` | Unique identifier (used to generate `naddr`) |
 | `title` | Human-readable board title |

 **Optional Tags:**

 | Tag | Description |
 |-----|-------------|
 | `description` | Extended description of the board |
 | `image` | Cover image URL |
 | `t` | Hashtag for categorization (repeatable) |
 | `collaborative` | Presence-only flag; when present, anyone can pin to this board |

 **Content:** Empty string or optional description text.

 **Example:**
 ```json
 {
   "kind": 30067,
   "pubkey": "<board-creator-pubkey>",
   "created_at": 1704067200,
   "content": "",
   "tags": [
     ["d", "japan-trip-2024"],
     ["title", "Japan Trip 2024"],
     ["description", "Photos, videos, and memories from my trip to Japan"],
     ["image", "https://example.com/mt-fuji.jpg"],
     ["t", "japan"],
     ["t", "travel"],
     ["collaborative"]
   ]
 }
 ```

 **Board Address Format:** `30067:<pubkey>:<d-tag>`

 ### Kind 39067: Pin

 A regular event representing a single pinned item. Unlike NIP-51 lists where items are embedded as tags, each pin is its own event—enabling per-item authorship, comments, and flexible board membership.

 #### Board Reference Tags (Optional)

 | Tag | Description |
 |-----|-------------|
 | `A` | Board coordinate: `30067:<pubkey>:<d-tag>` (repeatable for multi-board pinning) |

 A pin with no board `A` tags is a "profile pin" displayed on the author's profile.

 #### Content Reference Tags (Exactly One Required)

 Pins reference content using two methods:

 **For Nostr events** — use `e` or `a` tags:

 | Tag | Description |
 |-----|-------------|
 | `e` | Event reference for notes (kind:1), pictures (kind:20), videos (kind:21/22), etc. |
 | `a` | Coordinate reference for articles (kind:30023), bookmarks (kind:39701), recipes, etc. |

 **For external content** — use NIP-73 `i` tags:

 | Tag | Description |
 |-----|-------------|
 | `i` | External content ID per [NIP-73](73.md) |
 | `k` | External content kind per [NIP-73](73.md) |

 Supported external content types include:

 | Type | `i` tag format | `k` tag |
 |------|----------------|---------|
 | Web URLs | `https://example.com/page` | `web` |
 | Books | `isbn:9780765382030` | `isbn` |
 | Podcasts | `podcast:guid:<guid>` | `podcast:guid` |
 | Podcast Episodes | `podcast:item:guid:<guid>` | `podcast:item:guid` |
 | Movies | `isan:0000-0000-401A-0000-7` | `isan` |
 | Academic Papers | `doi:10.1000/xyz123` | `doi` |
 | Locations | `geo:<geohash>` | `geo` |

 #### Metadata Tags (Optional)

 | Tag | Description |
 |-----|-------------|
 | `title` | Custom title for this pin |
 | `t` | Hashtag (repeatable) |

 **Content:** Optional comment about the pinned item.

 ## Examples

 ### Mixed-content board

 A single "Japan Trip 2024" pinboard containing photos, videos, articles, notes, and external links:

 ```json
 // Photo (references kind:20 picture event)
 {
   "kind": 39067,
   "content": "Sunrise at Mt. Fuji",
   "tags": [
     ["A", "30067:<pubkey>:japan-trip-2024"],
     ["e", "<picture-event-id>", "wss://relay.example.com"]
   ]
 }

 // Video (references kind:21 video event)
 {
   "kind": 39067,
   "content": "The bullet train experience",
   "tags": [
     ["A", "30067:<pubkey>:japan-trip-2024"],
     ["e", "<video-event-id>", "wss://relay.example.com"]
   ]
 }

 // Article (references kind:30023 long-form content)
 {
   "kind": 39067,
   "content": "Great tips for first-time visitors",
   "tags": [
     ["A", "30067:<pubkey>:japan-trip-2024"],
     ["a", "30023:<author>:tokyo-guide", "wss://relay.example.com"]
   ]
 }

 // Note (references kind:1 note)
 {
   "kind": 39067,
   "content": "Best ramen I've ever had",
   "tags": [
     ["A", "30067:<pubkey>:japan-trip-2024"],
     ["e", "<note-event-id>", "wss://relay.example.com"]
   ]
 }

 // External URL (NIP-73)
 {
   "kind": 39067,
   "content": "Where I stayed in Tokyo",
   "tags": [
     ["A", "30067:<pubkey>:japan-trip-2024"],
     ["i", "https://booking.example.com/tokyo-hotel"],
     ["k", "web"],
     ["title", "Tokyo Hotel Recommendation"]
   ]
 }

 // Book recommendation (NIP-73)
 {
   "kind": 39067,
   "content": "Read this before visiting",
   "tags": [
     ["A", "30067:<pubkey>:japan-trip-2024"],
     ["i", "isbn:9784805311981"],
     ["k", "isbn"],
     ["title", "Japan Travel Guide"]
   ]
 }
 ```

 ### Pin to multiple boards

 ```json
 {
   "kind": 39067,
   "content": "",
   "tags": [
     ["A", "30067:<pubkey>:japan-trip-2024"],
     ["A", "30067:<pubkey>:best-photos-2024"],
     ["A", "30067:<pubkey>:travel-memories"],
     ["e", "<picture-event-id>"]
   ]
 }
 ```

 ### Profile pin (no board)

 ```json
 {
   "kind": 39067,
   "content": "My favorite photo ever",
   "tags": [
     ["e", "<picture-event-id>"],
     ["t", "photography"]
   ]
 }
 ```

 ### Pin a pinboard to another pinboard (meta-curation):

  ```json
  {
    "kind": 39067,
    "content": "Amazing travel photography collection",
    "tags": [
      ["A", "30067:<owner>:best-of-2024"],
      ["a", "30067:<curator>:japan-trip-2024"]
    ]
  }
 ```

 ### Using NIP-B0 bookmark (optional)

 Users who prefer to manage their web bookmarks as events can create a NIP-B0 bookmark first, then pin it:

 ```json
 {
   "kind": 39067,
   "content": "Great resource",
   "tags": [
     ["A", "30067:<pubkey>:dev-resources"],
     ["a", "39701:<pubkey>:github.com/nostr-protocol/nips"]
   ]
 }
 ```

 This is optional—users can also pin URLs directly using NIP-73 `i` tags.

 ## Collaboration Model

 ### Private Boards (Default)

 Without the `collaborative` tag, only the board owner's pins are displayed.

 **Filter for private board pins:**
 ```json
 {
   "kinds": [39067],
   "authors": ["<board-owner-pubkey>"],
   "#A": ["30067:<board-owner-pubkey>:<d-tag>"]
 }
 ```

 ### Collaborative Boards

 With the `collaborative` tag present, all pins referencing the board are displayed regardless of author.

 **Filter for collaborative board pins:**
 ```json
 {
   "kinds": [39067],
   "#A": ["30067:<board-owner-pubkey>:<d-tag>"]
 }
 ```

 ## Content Type Inference

 Clients infer content type from the reference to provide appropriate rendering:

 **For `e` tag references:** Fetch the event and use its `kind`:

 | Kind | Content Type |
 |------|--------------|
 | 1 | Note |
 | 20 | Picture |
 | 21, 22 | Video |

 **For `a` tag references:** Parse kind from coordinate (`kind:pubkey:d-tag`):

 | Kind | Content Type |
 |------|--------------|
 | 30023 | Long-form article |
 | 30078 | Application data (recipes, etc.) |
 | 30311 | Live event |
 | 31922, 31923 | Calendar event |
 | 34550 | Community |
 | 30067 | Nested pinboard |
 | 39701 | Web bookmark (NIP-B0) |

 **For `i` tag references:** Use the `k` tag value:

 | `k` value | Content Type |
 |-----------|--------------|
 | `web` | Web link |
 | `isbn` | Book |
 | `podcast:guid` | Podcast |
 | `podcast:item:guid` | Podcast episode |
 | `isan` | Movie |
 | `doi` | Academic paper |
 | `geo` | Location |

 ## Client Behavior

 ### Displaying Boards

 1. Fetch board event (kind 30067) by coordinate
 2. Fetch pins using appropriate filter based on `collaborative` flag
 3. For each pin:
    - If `e` or `a` tag: fetch referenced Nostr event
    - If `i` tag: render based on content type, optionally fetch metadata
 4. Display in masonry/grid layout with content-type-aware rendering

 ### Creating Pins

 1. Allow selection of target board(s) or "profile pin"
 2. For Nostr content: use `e` or `a` tags
 3. For external content: use NIP-73 `i`/`k` tags
 4. Accept optional title, comment, and hashtags
 5. Publish kind 39067 with appropriate tags

 ### Profile Pins

 Display pins by user that have no board `A` tag:
 ```json
 {"kinds": [39067], "authors": ["<pubkey>"]}
 ```
 Filter client-side for pins without `30067:` coordinate references.

 ## Engagement

 Boards support NIP-25 reactions and NIP-57 zaps:

 ```json
 {
   "kind": 7,
   "content": "+",
   "tags": [
     ["a", "30067:<pubkey>:<d-tag>"],
     ["p", "<board-author-pubkey>"]
   ]
 }
 ```

 ## Comparison with NIP-51

 | Feature | NIP-51 Curation Sets | Pinboards |
 |---------|---------------------|-----------|
 | Content types per list | Single (30004=articles, 30005=videos, 30006=pictures) | **Any type in one
 board** |
 | External content | Limited | **Full NIP-73 support** (URLs, books, podcasts, movies, papers) |
 | Item storage | Embedded in list event | Separate pin events |
 | Collaboration | No | Yes (`collaborative` flag) |
 | Multi-list membership | No | Yes (multiple `A` tags) |
 | Profile-level items | No | Yes (pins without board) |
 | Per-item attribution | No (list owner only) | Yes (pin author tracked) |
 | Per-item comments | No | Yes (pin content field) |
 | Scalability | Event size limited | Unlimited |

 ### When to Use Which

 **NIP-51 Lists:** Simple, private, single-content-type collections; encrypted private items needed.

 **Pinboards:** Mixed-content thematic collections; external content (books, podcasts, URLs); collaborative  curation; large collections; Pinterest-style visual presentation; per-item attribution needed.

 ## Security Considerations

 - Validate URLs in `i` tags to prevent `javascript:` or `data:` scheme attacks
 - Sanitize user content before rendering
 - Collaborative boards may receive spam; consider reputation-based filtering

 ## References

 - [NIP-51: Lists](51.md)
 - [NIP-73: External Content IDs](73.md)
 - [NIP-B0: Web Bookmarks](B0.md)
 - [NIP-25: Reactions](25.md)
 - [NIP-22: Comment](22.md)
 - [NIP-57: Lightning Zaps](57.md)
 