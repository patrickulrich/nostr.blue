# Blossom

## Description

A library for interacting with the [Blossom protocol](https://github.com/hzrd149/blossom).

## Implemented BUDs

- **Retrieval and upload:** [BUD-01](https://github.com/hzrd149/blossom/blob/master/buds/01.md), [BUD-02](https://github.com/hzrd149/blossom/blob/master/buds/02.md)
- **Discovery and mirroring:** [BUD-03](https://github.com/hzrd149/blossom/blob/master/buds/03.md), [BUD-04](https://github.com/hzrd149/blossom/blob/master/buds/04.md)
- **Media and preflight:** [BUD-05](https://github.com/hzrd149/blossom/blob/master/buds/05.md), [BUD-06](https://github.com/hzrd149/blossom/blob/master/buds/06.md)
- **Payments and metadata:** [BUD-07](https://github.com/hzrd149/blossom/blob/master/buds/07.md), [BUD-08](https://github.com/hzrd149/blossom/blob/master/buds/08.md)
- **Reports and URIs:** [BUD-09](https://github.com/hzrd149/blossom/blob/master/buds/09.md), [BUD-10](https://github.com/hzrd149/blossom/blob/master/buds/10.md)
- **Authorization and management:** [BUD-11](https://github.com/hzrd149/blossom/blob/master/buds/11.md), [BUD-12](https://github.com/hzrd149/blossom/blob/master/buds/12.md)
- **Server:** Not implemented

Payment support is transport-agnostic: the client exposes server challenges and accepts
proofs for `X-Cashu`, `X-Lightning`, and future BUD-07 methods, while applications remain
responsible for wallet interaction and method-specific validation.

## Changelog

All notable changes to this library are documented in the [CHANGELOG.md](CHANGELOG.md).

## State

**This library is in an ALPHA state**, things that are implemented generally work but the API will change in breaking ways.

## Donations

Nostr Dev Kit is free and open-source. This means we do not earn any revenue by selling it. Instead, we rely on your financial support. If you actively use any of the libs/software/services, then please [donate](https://nostrdevkit.org/donate).

## License

This project is distributed under the MIT software license - see the [LICENSE](../../LICENSE) file for details
