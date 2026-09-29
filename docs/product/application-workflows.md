# Application workflows: pinned Spectrum v3 orders

Decoder v2 recognizes four pinned Spectrum v3 N2T contract forms: `SwapSell`
(ERG to token), `SwapBuy` (token to ERG), `Deposit` and `Redeem`. `SwapSell` retains
its mainnet SPF funding restriction; `SwapBuy` reads its fee-token ID from the
contract, including the upstream SigRSV example. All four require a P2PK recipient.
Matching order boxes expose an explicit
workflow inspector on box and transaction pages. Unknown scripts and other versions
keep their ordinary detail view. This is a deliberately narrow support boundary,
not an assertion that every Spectrum transaction or application is understood.

The inspector links the order creation transaction, rechecks the order box on request,
then reads at most its recorded spending transaction. It distinguishes no indexed
spend, a matching swap/deposit/redemption, a spent but unrecognized outcome, and unavailable evidence.
No recursive graph scan, background workflow polling, wallet connection, third-party
request or external price feed is introduced. A missing local spend is not a mempool
or execution guarantee. A spend alone is never labelled a cancellation.

Recognition checks the exact expression and all non-parameter serialized constants,
with a restricted bounded Sigma constant parser. It validates the published pool
script, NFT/token positions, pool/recipient scripts, order
membership, amounts, the pool fee register, miner-fee limit and the contract's price
and execution-fee conditions. Integer arithmetic stays exact; a signed Long
multiplication overflow is outside supported interpretation. Invalid references,
duplicate boxes/assets, missing input boxes and oversized evidence remain unrecognized.
The parser is not a general ErgoTree interpreter or consensus validation.

Swaps require an unchanged LP reserve. A token-to-ERG swap must add exactly the
declared input token quantity and remove the ERG quote from the pool; its recipient
ERG includes the original order-box funding. Deposits require positive reserve
increases within the offered quantities, the corresponding bounded LP reserve
release, and the exact released LP quantity in the recipient output. Redemptions
require the requested LP quantity to return to the pool and both reserve decreases
to agree with the recipient returns and proportional bounds. These checks recognize
a narrow observed layout; they do not establish payment paths or beneficial ownership.

The recipient output and contract-calculated quote or minimum return are shown
separately. LP returns are raw units of the pool's existing LP token, not newly minted
token supply or a price valuation. When SPF is
the quote token, the output also incorporates the execution-fee token balance; its
amount must not be described as the user's net gain. Token IDs establish the matched
asset; names/decimals are not an issuer-verification mechanism. The constant at index
19 remains the literal 1400 in the pinned expression and is distinct from the variable
maximum execution fee at index 11; the decoder follows the serialized expression.
Two more distinctions are preserved rather than substituted with source-level intent:

- The pinned `SwapBuy` serialized expression computes the fee term as quote times
  constant 8 divided by constant 9. Its source comments and backend parser call
  these constants denominator and numerator, respectively. The inspector follows
  the serialized expression, including its conditional remainder check; it does
  not assert the executor's fee policy or infer who paid a fee.
- The pinned `Deposit` expression uses a separate literal 20,000 at constant 16 in
  its token-change branch. It does not reuse the variable ERG amount at constant 1
  there. Both the literal and the branch conditions are checked exactly.

Token-to-token pools, other versions, arbitrary recipient scripts, oversized
transactions, and other settlement layouts remain unsupported. No cancellation
badge is inferred from an order's refund public key.

The box and transaction fetches are separate observations. Immutable order facts and
the recorded spending transaction/inclusion must agree. Refresh clears previous
interpretations before reading; failures never retain a success badge. Navigation,
changed order references and changed transaction inclusion discard in-flight results.
The UI reports the transaction's indexed height when available. It does not prove
which branch of a Sigma OR was used, infer common ownership, or certify pool legitimacy.

## Pinned sources and regression evidence

- [Spectrum SwapSell contract, commit 8fe94e1](https://github.com/spectrum-finance/ergo-dex/blob/8fe94e1f8b4e1ab9402fca6fbc3955e3ad1415f7/contracts/amm/cfmm/v3/n2t/SwapSell.sc)
- [SwapBuy at the same commit](https://github.com/spectrum-finance/ergo-dex/blob/8fe94e1f8b4e1ab9402fca6fbc3955e3ad1415f7/contracts/amm/cfmm/v3/n2t/SwapBuy.sc)
- [Deposit at the same commit](https://github.com/spectrum-finance/ergo-dex/blob/8fe94e1f8b4e1ab9402fca6fbc3955e3ad1415f7/contracts/amm/cfmm/v3/n2t/Deposit.sc)
- [Redeem at the same commit](https://github.com/spectrum-finance/ergo-dex/blob/8fe94e1f8b4e1ab9402fca6fbc3955e3ad1415f7/contracts/amm/cfmm/v3/n2t/Redeem.sc)
- [N2T pool source at the same commit](https://github.com/spectrum-finance/ergo-dex/blob/8fe94e1f8b4e1ab9402fca6fbc3955e3ad1415f7/contracts/amm/cfmm/v1/n2t/Pool.sc)
- [Published pool bytes, backend commit 5dc8e9d](https://github.com/spectrum-finance/ergo-dex-backend/blob/5dc8e9d983f2361308d317be7514260795ef92c9/modules/dex-core/src/main/scala/org/ergoplatform/dex/protocol/amm/AMMContracts.scala)
- [Upstream order example](https://github.com/spectrum-finance/ergo-dex-backend/blob/5dc8e9d983f2361308d317be7514260795ef92c9/modules/utxo-tracker/src/test/scala/org/ergoplatform/dex/tracker/parsers/amm/V3Orders.scala)
- [Upstream SwapBuy, Deposit and Redeem examples](https://github.com/spectrum-finance/ergo-dex-backend/blob/5dc8e9d983f2361308d317be7514260795ef92c9/modules/utxo-tracker/src/test/scala/org/ergoplatform/dex/tracker/parsers/amm/N2TV3ParserSpec.scala)
- [Backend v3 parameter parsing](https://github.com/spectrum-finance/ergo-dex-backend/blob/5dc8e9d983f2361308d317be7514260795ef92c9/modules/utxo-tracker/src/main/scala/org/ergoplatform/dex/tracker/parsers/amm/v3/N2TOrdersV3Parser.scala)

`tests/fixtures/apps/spectrum-v3-swap.json` records a public mainnet order and its
settlement, retrieved on 2026-09-28 from the URLs stored in its provenance object.
The order spends 300,000,000 nanoERG; the recipient output contains 5,429,060 SPF raw
units and the contract-calculated quote is 5,405,915 raw units. Facts are projected
into Kadia's field names and all quantities stay decimal strings. The fixture omits
unrelated size/rent metadata; browser mocks add explicitly synthetic display fields.
The recorded observations are not a live view or a fixture for rent accuracy.

Three additional captured settlements are projected without floating-point conversion:

| Fixture | Recorded output | Separate contract quantity |
| --- | --- | --- |
| `spectrum-v3-swap-buy.json` | 10,457,609 nanoERG | 10,147,609 nanoERG quote; order carried 310,000 nanoERG |
| `spectrum-v3-deposit.json` | 4,206 raw LP units | 4,206 minimum LP units |
| `spectrum-v3-redeem.json` | 33,654,950 nanoERG and 5 token units | Minimum 33,344,950 nanoERG and 5 token units; order carried 310,000 nanoERG |

Their provenance records the upstream examples, source API URLs and pinned contract
commit. `indexed_height` is conservatively set to the recorded inclusion height,
not the remote explorer's live tip. Later output spends are deliberately omitted.
The fixture fee is the observed miner-contract output amount. Unrelated rent/size
display fields are synthesized only in browser tests.

Unit tests mutate literal constants, script bytes, pool/token ordering, recipient,
fee/price bounds, references and spend anchors. Browser cases cover explicit loading,
transaction discovery, unknown/unspent/error outcomes, exact values and all four
appearances in light/dark modes at mobile, tablet and desktop widths. Original uses
a compact two-column inspector; Prism uses a raised request card; Atelier uses a
ledger margin; Aurora groups the two stages beneath a centered introduction.
