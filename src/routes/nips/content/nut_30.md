# NUT-30: Payment Method: Onchain

`optional`

`depends on: NUT-04, NUT-05, NUT-20`

---

This document describes minting and melting ecash with the `onchain` payment method, which uses Bitcoin onchain payments. It is an extension of [NUT-04][04] and [NUT-05][05] which cover the protocol steps of minting and melting ecash shared by any supported payment method.

## Mint Quote

For the `onchain` method, the wallet includes the following specific `PostMintQuoteOnchainRequest` data:

```json
{
  "unit": <str_enum[UNIT]>,
  "pubkey": <str>
}
```

> **Note:** A [NUT-20][20] `pubkey` is required in this NUT and the mint **MUST NOT** issue a mint quote if one is not included.

The mint responds with a `PostMintQuoteOnchainResponse`:

```json
{
  "quote": <str>,
  "request": <str>,
  "unit": <str_enum[UNIT]>,
  "method": "onchain",
  "expiry": <int|null>,
  "pubkey": <str>,
  "amount_paid": <int>,
  "amount_issued": <int>,
  "updated_at": <int>
}
```

Where:

- `quote` is the quote ID
- `request` is the Bitcoin address to send funds to
- `expiry` is the Unix timestamp until which the mint quote is valid
- `pubkey` is the public key from the request
- `amount_paid` is the total confirmed amount paid to the request in UTXOs that are eligible for minting
- `amount_issued` is the amount of ecash that has been issued for the given mint quote
- `updated_at` is defined in [NUT-04][04]

If `expiry` is not `null`, the wallet **SHOULD NOT** send payments to the request after `expiry`. Mints **MUST** keep monitoring transactions they detected before `expiry` until the transaction reaches the required number of confirmations or is evicted or replaced. Payments first detected by the mint after `expiry` **MUST NOT** increase `amount_paid`.

## Multiple Deposits

Onchain addresses can receive multiple payments, allowing the wallet to mint multiple times for one quote. The wallet can call the check onchain endpoint, where the mint will return the `PostMintQuoteOnchainResponse` including `amount_paid` and `amount_issued`. The difference between these values represents how much the wallet can mint by calling the mint endpoint. Wallets **MAY** mint any amount up to this available difference; in particular, they can mint less than the amount mintable. Mints **MUST** accept mint requests whose total output amount is less than or equal to (`amount_paid` - `amount_issued`).

Only eligible UTXOs increase `amount_paid`. If a quote receives both eligible and ineligible UTXOs, only the eligible UTXOs count towards `amount_paid` and mintable balance. UTXOs that do not increase `amount_paid` are not recoverable through the mint quote protocol.

## Mint Settings

A `confirmations` option **SHOULD** be set to indicate the minimum depth in the blockchain for a transaction to be considered confirmed.

For the `onchain` mint method, `min_amount` indicates both the minimum mint operation amount and the minimum amount of an individual UTXO that the mint will credit to `amount_paid`. Wallets **SHOULD NOT** send onchain payments below `min_amount` to a quote address.

## Melt Quote

For the `onchain` method, the wallet includes the following specific `PostMeltQuoteOnchainRequest` data:

```json
{
  "request": <str>,
  "unit": <str_enum[UNIT]>,
  "amount": <int>
}
```

Where:

- `request` is the Bitcoin address that should receive the onchain payment
- `unit` is the unit the wallet would like to pay with
- `amount` is the amount to send in the specified unit

Unlike other melt methods, a single `onchain` melt quote can contain multiple fee options for the same payment. This allows the wallet to choose between different fee and confirmation estimates while preserving a single quote ID.

The mint responds with a `PostMeltQuoteOnchainResponse`:

```json
{
  "quote": <str>,
  "amount": <int>,
  "unit": <str_enum[UNIT]>,
  "method": "onchain",
  "state": <str_enum[STATE]>,
  "expiry": <int>,
  "request": <str>,
  "fee_options": [
    {
      "fee_index": <int>,
      "fee_reserve": <int>,
      "estimated_blocks": <int>
    }
  ],
  "selected_fee_index": <int|null>,
  "outpoint": <str|null>
}
```

Each item in `fee_options` represents one available fee reserve and confirmation estimate for the same payment. The wallet selects one of these options when executing the melt quote by including the option's `fee_index` value in the melt request. The mint **MUST** return at least one `fee_options` item. The returned `fee_options` are fixed for the lifetime of the quote.

For each fee option with `fee_index`, `fee_reserve` is the maximum onchain transaction fee the mint may charge for that option, and `estimated_blocks` is the estimated number of blocks until confirmation. `selected_fee_index` is `null` before the quote is executed and is set by the mint to the selected fee option once the wallet executes the quote. The mint expects the wallet to include `Proofs` of _at least_ `total_amount = amount + selected_fee_reserve + input_fee` where `selected_fee_reserve` is the `fee_reserve` from the selected `fee_index` item and `input_fee` is calculated from the keyset's `input_fee_ppk` as described in [NUT-02][02]. If the mint does not claim the full `selected_fee_reserve` as the actual fee, the mint returns the unclaimed amount as `change` to the wallet as described in [NUT-08][08].

`state` is an enum string field with possible values `"UNPAID"`, `"PENDING"`, `"PAID"`:

- `"UNPAID"` means that the transaction has not been broadcast yet.
- `"PENDING"` means that the transaction is being processed by the mint but has not reached the required number of confirmations.
- `"PAID"` means that the transaction has been mined and confirmed.

`outpoint` is the transaction ID and output index of the payment in the format `txid:vout`, present once the transaction has been broadcast.

### Melting Tokens

Onchain melt requests are always asynchronous. The mint **MUST** return a `"PENDING"` state after validating the melt request and then broadcast the Bitcoin transaction in the background. The wallet **MUST** monitor the quote state through the check quote endpoint until the transaction reaches the required number of confirmations.

For the `onchain` method, the wallet includes the following specific `PostMeltOnchainRequest` data. The wallet can include an optional `outputs` field in the melt request to receive change for overpaid onchain fees (see [NUT-08][08]):

```json
{
  "quote": <str>,
  "fee_index": <int>,
  "inputs": <Array[Proof]>,
  "outputs": <Array[BlindedMessage]> // Optional
}
```

Where `fee_index` is the selected fee of the quote's `fee_options`. The mint **MUST** reject a melt request with a `fee_index` that was not returned in the quote. Once `selected_fee_index` is set, the mint **MUST NOT** execute the quote again with a different `fee_index` value.

If the `outputs` field is included and the mint does not claim the full `selected_fee_reserve` as the actual fee, the mint will respond with a `change` field containing blind signatures for the unclaimed amount (see [NUT-08][08]). The `change` field is omitted if there are no blind signatures to return. Note that because onchain transactions may be batched or have unpredictable costs, the mint is entitled to claim the full `selected_fee_reserve` as the actual fee.

[02]: 02.md
[04]: 04.md
[05]: 05.md
[08]: 08.md
[20]: 20.md
