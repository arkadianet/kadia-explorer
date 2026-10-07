# Network overview and observed rewards

`GET /v1/network/summary` aggregates at most 720 latest indexed headers in one reader snapshot. Decimal amount strings remain exact; hourly chart values are only presentation aids. The response includes its height range, canonical tip ID, retained-history boundary, up to ten recent block DTOs and fixed-length chart arrays. Header decoding has the shared 2 MiB / 4 s admission budget. It never scans transaction boxes.

Totals describe the **latest indexed blocks**, not an assumed complete 24-hour window. Mining timestamps can be out of order and block intervals vary. Charts place only sampled blocks into 24 hourly or 36 ten-minute buckets ending at the tip timestamp; timestamps beyond that anchor or at/below the open lower edge are excluded from those charts, but remain in the block-scope totals. Empty buckets are not evidence that the rest of the chain had no activity.

The homepage requests six transaction summaries and five upcoming-rent boxes rather than expanded transaction graphs and 500 rent boxes. Rent totals remain labelled lower bounds whenever the API does not establish complete coverage. Block detail also uses the existing paged strict transaction-summary endpoint, so a transaction expansion rejection cannot hide an entire block.

## Reward definition

`GET /v1/blocks/{height_or_id}/rewards` recognizes the two-output mainnet EIP-27 original-emission transaction shape between activation height 777,217 and the original-emission boundary 2,080,800 (exclusive). It requires the known emission NFT in the reserve output, correct transaction/output associations and creation heights, and byte equality with the 720-block delayed reward script for the header's miner key. The miner output must contain the known re-emission liability token.

The observed liability token quantity denotes nanoERG owed when the reward box is spent. It is not a payment to the re-emission contract in the same block. Gross box value minus this obligation is the miner subsidy; transaction fees are separate and storage-rent income is not included. The retained real-chain fixture at height 1,866,000 demonstrates 12 ERG gross minus 9 ERG obligation equals 3 ERG subsidy.

Source: [implemented EIP-27](https://github.com/ergoplatform/eips/blob/master/eip-0027.md), especially the mainnet token IDs and the rule describing liability tokens in miner reward boxes. The recognizer intentionally does not estimate unsupported contract forms or future re-emission withdrawals from height alone. Unsupported evidence returns null monetary breakdowns, never zero. This is a supported-shape observation, not a consensus interpreter or proof of total miner income.

The historical `BlockDto.reward` field remains wire-compatible and retains its old first-transaction emission estimate semantics. The block list calls it an emission estimate; the homepage displays transaction fees, and block detail uses the observed breakdown instead. No database migration or retroactive rewriting of that legacy field is required.
