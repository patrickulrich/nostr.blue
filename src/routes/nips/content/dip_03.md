# DIP-03
## Private Zaps

`draft` `optional` `author:jb55` `depends: NIP-57, NIP-04`

Private lightning zaps hide the sender's identity from everyone except the
recipient. Only the receiver can decrypt the sender's pubkey and an optional
private message.

Upstream spec: <https://github.com/damus-io/dips/blob/master/03.md>

## How it works

The sender builds a **private zap event** (kind `9733`) with the standard zap
request structure — `p` tag for the recipient, optional `e`/`a` tags for the
zapped event, and the private message in `content` — and signs it with their
**real key**. This inner event is what carries the sender's identity.

The inner event JSON is then encrypted with NIP-04-compatible cryptography
(ECDH shared secret + AES-256-CBC) using a fresh **ephemeral keypair** as the
sender side and the recipient's pubkey as the receiver side. Instead of the
NIP-04 base64 wire format, the ciphertext and IV are bech32-encoded with the
HRPs `pzap` and `iv` and joined with an underscore:

```
pzap1<ciphertext>_iv1<iv>
```

Finally a normal-looking **zap request** (kind `9734`) is signed by the
ephemeral key and carries the encrypted payload in an `anon` tag. The outer
event reveals nothing about the real sender: the pubkey is ephemeral and the
content is empty.

The kind 9734 is passed to the LNURL callback exactly like a standard NIP-57
zap request; the wallet embeds it verbatim in the kind 9735 receipt's
`description` tag, which is how the recipient obtains the `anon` payload.

## Variants

- **Anonymous zap**: the kind 9734 carries a bare `["anon"]` tag and no
  encrypted payload. Nobody — not even the recipient — learns the sender.
- **Private zap**: the `anon` tag payload decrypts to a signed kind 9733
  event; the recipient learns the sender identity and private message.

## nostr.blue implementation notes

- Sending uses a random ephemeral keypair (`Keys::generate`), so private zaps
  work on every platform — including WASM with NIP-07/46/55 signers, where
  the real private key is never accessible. The inner kind 9733 is signed
  through the unified signer abstraction; the outer kind 9734 by the
  ephemeral key.
- Receiving decrypts via local keys directly, or by re-encoding the bech32
  payload as a NIP-04 content string and calling `nip04_decrypt` on remote
  signers (the cipher is identical; only the encoding differs). Decryption
  results are cached per payload to avoid repeated signer prompts.
- Deterministic ephemeral keys (self-recovery of sent private zaps) are not
  implemented: they require raw private-key access and the sent-zaps tab
  filters by the receipt's `P` tag, which is the ephemeral key either way.
- Zap amounts remain public in the bolt11 invoice of the kind 9735 receipt,
  so zap goal totals are unaffected.
