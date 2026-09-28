# Application workflows: Spectrum v3 ERG-to-token orders

The first decoder recognizes the pinned Spectrum v3 N2T `SwapSell` contract with
mainnet SPF fee funding and a P2PK recipient. Matching order boxes expose an explicit
workflow inspector on box and transaction pages. Unknown scripts and other versions
keep their ordinary detail view. This is a deliberately narrow support boundary,
not an assertion that every Spectrum transaction or application is understood.

The inspector links the order creation transaction, rechecks the order box on request,
then reads at most its recorded spending transaction. It distinguishes no indexed
spend, a matching exchange, a spent but unrecognized outcome, and unavailable evidence.
No recursive graph scan, background workflow polling, wallet connection, third-party
request or external price feed is introduced. A missing local spend is not a mempool
or execution guarantee. A spend alone is never labelled a cancellation.

Recognition checks the exact expression and all non-parameter serialized constants,
with a restricted bounded Sigma constant parser. It validates the published pool
script, NFT/token positions, unchanged LP reserve, pool/recipient scripts, order
membership, amounts, the pool fee register, miner-fee limit and the contract's price
and execution-fee conditions. Integer arithmetic stays exact; a signed Long
multiplication overflow is outside supported interpretation. Invalid references,
duplicate boxes/assets, missing input boxes and oversized evidence remain unrecognized.
The parser is not a general ErgoTree interpreter or consensus validation.

The recipient output and contract-calculated quote are shown separately. When SPF is
the quote token, the output also incorporates the execution-fee token balance; its
amount must not be described as the user's net gain. Token IDs establish the matched
asset; names/decimals are not an issuer-verification mechanism. The constant at index
19 remains the literal 1400 in the pinned expression and is distinct from the variable
maximum execution fee at index 11; the decoder follows the serialized expression.

The box and transaction fetches are separate observations. Immutable order facts and
the recorded spending transaction/inclusion must agree. Refresh clears previous
interpretations before reading; failures never retain a success badge. Navigation,
changed order references and changed transaction inclusion discard in-flight results.
The UI reports the transaction's indexed height when available. It does not prove
which branch of a Sigma OR was used, infer common ownership, or certify pool legitimacy.

## Pinned sources and regression evidence

- [Spectrum SwapSell contract, commit 8fe94e1](https://github.com/spectrum-finance/ergo-dex/blob/8fe94e1f8b4e1ab9402fca6fbc3955e3ad1415f7/contracts/amm/cfmm/v3/n2t/SwapSell.sc)
- [N2T pool source at the same commit](https://github.com/spectrum-finance/ergo-dex/blob/8fe94e1f8b4e1ab9402fca6fbc3955e3ad1415f7/contracts/amm/cfmm/v1/n2t/Pool.sc)
- [Published pool bytes, backend commit 5dc8e9d](https://github.com/spectrum-finance/ergo-dex-backend/blob/5dc8e9d983f2361308d317be7514260795ef92c9/modules/dex-core/src/main/scala/org/ergoplatform/dex/protocol/amm/AMMContracts.scala)
- [Upstream order example](https://github.com/spectrum-finance/ergo-dex-backend/blob/5dc8e9d983f2361308d317be7514260795ef92c9/modules/utxo-tracker/src/test/scala/org/ergoplatform/dex/tracker/parsers/amm/V3Orders.scala)

`tests/fixtures/apps/spectrum-v3-swap.json` records a public mainnet order and its
settlement, retrieved on 2026-09-28 from the URLs stored in its provenance object.
The order spends 300,000,000 nanoERG; the recipient output contains 5,429,060 SPF raw
units and the contract-calculated quote is 5,405,915 raw units. Facts are projected
into Kadia's field names and all quantities stay decimal strings. The fixture omits
unrelated size/rent metadata; browser mocks add explicitly synthetic display fields.
The recorded observations are not a live view or a fixture for rent accuracy.

Unit tests mutate literal constants, script bytes, pool/token ordering, recipient,
fee/price bounds, references and spend anchors. Browser cases cover explicit loading,
transaction discovery, unknown/unspent/error outcomes, exact values and all four
appearances in light/dark modes at mobile, tablet and desktop widths. Original uses
a compact two-column inspector; Prism uses a raised request card; Atelier uses a
ledger margin; Aurora groups the two stages beneath a centered introduction.
