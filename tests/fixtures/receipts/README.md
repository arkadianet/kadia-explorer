# Receipt regression evidence

Captured from public mainnet endpoints on 2026-09-28:

- `storage-rent-tx.json`: https://explorer.kadia.io/v1/txs/a474884a8a6ae615fd230cd177177c7436afc74f3d9a3506489800ccab749814
- `storage-rent-block.json`: https://node.kadia.io/blocks/f874382423e88b927a92425dac4e01262899d2692212caafcb7d222b95053e04

JSON is pretty-printed. The transaction fixture adds the block ID from the node
and a fixed indexed-height snapshot of 1,882,694 for deterministic confirmations.
Spending evidence is taken from the node, not fabricated by the classifier.
The repository owner identified this as a bot-operated rent claim. Automation and
operator identity are intentionally not inferred by the production receipt.

The expired P2PK input has an empty proof, selector `127: 0300`, age 1,051,202 blocks
and no owner-script return. Its 110,800 nanoERG and 2,000 tokens are consumed. The
other input contributes 1 ERG and has a nonempty proof. That address receives
999,010,800 nanoERG and 2,000 tokens; the fee output is 1,100,000 nanoERG.

These are trusted node/explorer responses, not independently verified commitments.
