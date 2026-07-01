# Vector-Embedding Specification

NKBIP-02

![Vector embedding graph, 400](https://i.nostr.build/ijZF6rJ3MM0wiVjZ.png)

**Embedded Content**

Allowing rich content to be embedded within Nostr events opens up new possibilities for applications built on the protocol. This is highly synergistic with NKBIP-01/NIP-62, where we can decompose existing events into their most semantically closed forms as defined either by the author or the analyst for precise semantic embeddings, but this process can be applied to any other event. Embeddings are also typically computationally expensive, which makes it beneficial to compute them once and store them for reuse.

**Motivation**

Hashtags and labels already exist as one way to classify content, embeddings allow for a fine grained resolution of content. Every embedding model has tradeoffs, so if you need something more specific than what is available, use your model.

Nostr events are public, therefore it is to be expected that any event can be analyzed. Given this fact, it only makes sense to formalize and allow anyone to work with it. Embeddings are useful for various applications, including:

1. Semantic search across Nostr events
2. Content recommendation systems
3. Clustering similar content
4. Multimodal AI application

**Specification**

The `1987` event are embedding specific labels for an event.

Required tags:

- `e` - The id of the original event being embedded
- `model` - The name and version of the model used to create the embedding and if possible, a link to download/source
- `type` - The type of content being embedded (e.g., "text", "image", "audio", "video")
- `vector` - The embedding vector, represented as a comma-separated list of floating-point numbers

Optional tags:

- `dims` - The dimensions of the embedding vector (integer)
- `norm` - Boolean attribute whether the vector is normalized to range [0,1]
- If there is a download link, use the tags `hash` and `hash_type` to verify the model being used.
- `source` - Original data source that is being embedded. Mainly suited for text. This allows the original data to be attained without fetching the original event.
- All other tags and labeling conventions may also apply such as specified in NKBIP-01 and NIP-32

Example event:

```json
{
  "id": "<event_id>",
  "pubkey": "<pubkey>",
  "created_at": 1234567890,
  "kind": 1987,
  "tags": [
    ["e", "<original_event_id>", "<relay_uri>"],
    ["model", "openai/text-embedding-ada-002", "<optional download link>"],
    ["type", "text"],
    ["vector", [-0.006929, -0.005135, 0.003592, ..., 0.004543]],
    ["dims", 1536],
    ["norm", true],
    ["source", "This is a snippet of the original text content"],
    ["hash", "<string>"],
    ["hash_type", "<string>"]
  ],
  "content": "",
  "sig": "<signature>"
}
```

**Guidelines**

1. For non-text embeddings (images, audio, video), the `source` tag may contain a URL or identifier for the original content.
2. The `model` tag should be as specific as possible, including version information and location to download if available. This ensures that clients can properly interpret and compare embeddings.
3. Multiple embedding events can reference the same original event, allowing for different embedding models or content types for the same source material.
