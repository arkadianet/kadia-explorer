/**
 * Concrete ids the specs navigate to, read out of the very same dataset the mock server
 * serves — so a change to the fixtures can never leave a hard-coded id behind.
 */

import { buildDataset, FIXTURE_HEIGHTS, MOCK_ADDRESS, SYNTHETIC_TOKEN } from './mock/fixtures.ts';

const d = buildDataset();

export { MOCK_ADDRESS };

/** The mock's tip height, and the real fixture blocks — both straight from the dataset. */
export const TIP = d.status.best;
export const BLOCK_HEIGHTS = FIXTURE_HEIGHTS;

/** Every block the mock serves (three real, the rest synthesised fillers). */
export const BLOCK_COUNT = d.blocks.length;

/** The middle fixture block, used for the block-detail spec. */
const MID_HEIGHT = FIXTURE_HEIGHTS[1];
export const block = d.blockByHeight.get(MID_HEIGHT)!;
export const blockTxCount = (d.txIdsByHeight.get(MID_HEIGHT) ?? []).length;

/** A transaction with several inputs and outputs, from the middle of that block. */
export const tx = d.txById.get((d.txIdsByHeight.get(MID_HEIGHT) ?? [])[1]!)!;

/** A box the mock reports as claimable right now, and one that is not. */
export const claimableBox = d.rentEligible[0].box;
export const maturingBox = d.rentUpcoming[0].box;

/** A box that was spent inside the fixture range. */
export const spentBox = [...d.boxById.values()].find((b) => b.spent_by !== null)!;

/** Richlist as the mock orders it (balance descending). */
export const richlist = d.richlist;

export const upcomingCount = d.rentUpcoming.length;
export const eligibleCount = d.rentEligible.length;

/** Every transaction the mock serves, newest first — the `/txs` list. */
export const txCount = d.txs.length;
export const newestTx = d.txs[0];

/** A well-formed but unknown 64-hex id and address, for the not-found paths. */
export const UNKNOWN_HEX = 'f'.repeat(64);
export const UNKNOWN_ADDRESS = '9unknownAddressNeverSeenQqWweeRrTtYyUuPpAaSsDdFf';

// ------------------------------------------------------------------ tokens and templates

export { SYNTHETIC_TOKEN };

/** Every token the mock serves, so the /tokens spec knows how far the list can scroll. */
export const tokenCount = d.tokens.length;

/**
 * The first row under each sort. The synthetic token is minted into the newest block, so it
 * heads `newest`; a fixture token held by several trees heads `holders` — the two differ,
 * which is what makes the sort toggle observable.
 */
export const newestToken = d.tokens[0];
export const mostHeldToken = d.tokensByHolders[0];

/** Holder rows of `mostHeldToken`, amount descending, as the holders tab lists them. */
export const tokenHolders = d.tokenHolders.get(mostHeldToken.id)!;

/** Boxes carrying `mostHeldToken` — all of them, and the unspent ones the tab defaults to. */
const heldBoxes = (d.boxIdsByToken.get(mostHeldToken.id) ?? []).map((id) => d.boxById.get(id)!);
export const tokenBoxCount = heldBoxes.length;
export const tokenUnspentBoxCount = heldBoxes.filter((b) => b.spent_by === null).length;

/**
 * The one template with a resolvable example address (the richest tree's), so the template
 * page's "Example address" fact has something to link to.
 */
export const template = [...d.templates.values()].find((t) => t.example_address !== null)!;

/**
 * A register lookup that returns boxes: the R4 value carried by the most boxes, capped at
 * one mock page so the whole result is on screen without scrolling. Sorted by key first, so
 * the choice does not depend on Map insertion order.
 */
export const registerLookup = (() => {
	const [key, ids] = [...d.boxIdsByRegister.entries()]
		.filter(([k, v]) => k.startsWith('R4:') && v.length <= 5)
		.sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]))[0];
	return { reg: 'R4', value: key.slice('R4:'.length), boxIds: ids };
})();

/** A well-formed register value no fixture box carries — the empty-result path. */
export const UNKNOWN_REGISTER_VALUE = '0e08' + 'ab'.repeat(8);
