# NUT-25: BOLT12

`optional`

`depends on: NUT-04, NUT-05, NUT-20`

---

This document describes minting and melting ecash with the `bolt12` payment method, which uses Lightning Network offers. It is an extension of [NUT-04][04] and [NUT-05][05] which cover the protocol steps of minting and melting ecash shared by any supported payment method.

## Mint Quote

For the `bolt12` method, the wallet includes the following specific `PostMintQuoteBolt12Request` data:

```json
{
  "amount": <int|null>,
  "unit": <str_enum[UNIT]>,
  "description": <str|null>,
  "pubkey": <str>
}
```

> **Note:** While a pubkey is optional as per [NUT-20][20] for [NUT-04][04] it is required in this NUT and the mint **MUST NOT** issue a mint quote if one is not included.

> **Privacy:** To prevent linking multiple mint quotes together, wallets **SHOULD** generate a unique public key for each mint quote request.

The mint responds with a `PostMintQuoteBolt12Response`:

```json
{
  "quote": <str>,
  "request": <str>,
  "amount": <int|null>,
  "unit": <str_enum[UNIT]>,
  "method": "bolt12",
  "expiry": <int|null>,
  "pubkey": <str>,
  "amount_paid": <int>,
  "amount_issued": <int>,
  "updated_at": <int>
}
```

Where:

- `quote` is the quote ID
- `request` is the bolt12 offer
- `amount` echoes the requested amount, or `null` for an amountless offer
- `expiry` is the Unix timestamp until which the mint quote is valid
- `amount_paid` is the amount that has been paid to the mint via the bolt12 offer
- `amount_issued` is the amount of ecash that has been issued for the given mint quote
- `updated_at` is defined in [NUT-04][04]

## Multiple Issuances

Unlike BOLT11 invoices, BOLT12 offers can be paid multiple times, allowing the wallet to mint multiple times for one quote. The wallet can call the check bolt12 endpoint, where the mint will return the `PostMintQuoteBolt12Response` including `amount_paid` and `amount_issued`. The difference between these values represents how much the wallet can mint by calling the mint endpoint. Wallets MAY mint any amount up to this available difference; in particular, they can mint less than the amount mintable. Mints MUST accept mint requests whose total output amount is less than or equal to (`amount_paid` - `amount_issued`).

## Mint Settings

A `description` option **SHOULD** be set to indicate whether the `bolt12` payment method backend supports providing an offer description.

## Melt Quote

For the `bolt12` method, the wallet includes the following specific `PostMintQuoteBolt12Request` data:

```json
{
  "request": <str>,
  "unit": <str_enum[UNIT]>,
  "options": { // Optional
    "amountless": {
      "amount_msat": <int>
    }
  }
}
```

Here, `request` is the bolt12 Offer to be paid and `unit` is the unit the wallet would like to pay with. For amount-less offers, the `options.amountless.amount_msat` field can be used to specify the amount in millisatoshis to pay to the offer. If `options.amountless.amount_msat` is defined and the offer has an amount, they **MUST** be equal.

The mint responds with a `PostMeltQuoteBolt12Response`:

```json
{
  "quote": <str>,
  "request": <str>,
  "amount": <int>,
  "unit": <str_enum[UNIT]>,
  "method": "bolt12",
  "fee_reserve": <int>,
  "state": <str_enum[STATE]>,
  "expiry": <int>,
  "payment_preimage": <str|null>
}
```

Where `fee_reserve` is the additional fee reserve required for the Lightning payment. The mint expects the wallet to include `Proofs` of _at least_ `total_amount = amount + fee_reserve + fee` where `fee` is calculated from the keyset's `input_fee_ppk` as described in [NUT-02][02].

`state` is an enum string field with possible values `"UNPAID"`, `"PENDING"`, `"PAID"`.

### Melting Tokens

For the `bolt12` method, the wallet can include an optional `outputs` field in the melt request to receive change for overpaid Lightning fees (see [NUT-08][08]). If the `outputs` field is included and there is excess from the `fee_reserve`, the mint will respond with a `change` field containing blind signatures for the overpaid amount.

[02]: 02.md
[04]: 04.md
[05]: 05.md
[08]: 08.md
[20]: 20.md
