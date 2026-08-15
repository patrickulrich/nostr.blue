# Security Policy

nostr.blue is a Nostr social client (Web, Android, Linux desktop) that handles
signer-backed identities, encrypted messages, ecash, and user media. We take
security reports seriously and appreciate responsible disclosure.

## Supported Versions

Only the latest release receives security fixes; we do not backport patches
to older versions. Fixes also land on `main` ahead of the next release.

| Version        | Supported |
| -------------- | --------- |
| Latest release | ✅        |
| `main`         | ✅        |
| Older          | ❌        |

This covers all artifacts built from this repository: the web app
(nostr.blue / GitHub Pages), the Android APK/AAB, and the Linux AppImage.

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Use GitHub's private vulnerability reporting instead:

👉 [Report a vulnerability](https://github.com/patrickulrich/nostr.blue/security/advisories/new)

This keeps details private until a fix is ready and coordinates disclosure
between you and the maintainer.

### What to include

- A clear description of the vulnerability and its impact (what an attacker
  could achieve — e.g. key material exposure, DM confidentiality, funds,
  integrity, availability).
- Affected area(s): signer (NIP-07/46/49/55), encryption (NIP-04/17/44/59),
  Cashu wallet, backups (Google Drive key backup), NIP-78 preference blobs,
  NIP-98 HTTP auth, relay pool, media upload, or general UI.
- Affected version(s), commit SHA, platform (Web / Android / Linux), and OS.
- Steps to reproduce, a proof of concept, or a failing test.
- Any suggested remediation.

### What to expect

- We aim to acknowledge reports promptly and will keep you informed of
  progress.
- We will coordinate a release and disclosure timeline with you.
- Credit will be given in the security advisory and release notes unless you
  prefer to remain anonymous.

## Scope

In scope:

- Source code in this repository across all platforms and feature areas.
- Released binaries (web build, APK, AAB, AppImage) built from this repository.
- Cryptographic handling: signing, NIP-04/17/44 encryption, gift wrapping
  (NIP-59), key storage and external signer flows (NIP-07/46/49/55).
- Wallet behavior that could expose ecash tokens or payment secrets.
- Relay client behavior that could leak private data or bypass authorization.

Out of scope:

- Vulnerabilities in third-party relays, media/Blossom servers, or other
  clients not built from this repository.
- Weaknesses inherent to the Nostr protocol itself — please report these
  upstream at <https://github.com/nostr-protocol/nips>.
- Bugs in upstream dependencies (rust-nostr, Dioxus, CDK) — we will help
  route them upstream, but they are tracked in their own repositories.
- Issues requiring a rooted device, a compromised host, or physical access
  to an unlocked device.
- Denial-of-service from a malicious relay the user has explicitly connected
  to.
- Social engineering and phishing that does not exploit an app-level flaw.

## Disclosure Policy

We follow a coordinated disclosure model. We ask that you:

- Give us reasonable time to investigate and release a fix before any public
  disclosure.
- Avoid accessing or modifying other users' data during research.
- Only interact with accounts and data you own or have explicit permission
  to test.
- Act in good faith.

We commit to responding promptly and treating all reports seriously. Thank
you for helping keep nostr.blue and its users safe.
