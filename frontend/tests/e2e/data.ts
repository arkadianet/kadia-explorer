/**
 * Concrete ids the specs navigate to, read out of the very same dataset the mock server
 * serves — so a change to the fixtures can never leave a hard-coded id behind.
 */

import { buildDataset, FIXTURE_HEIGHTS, MOCK_ADDRESS } from './mock/fixtures.ts';

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
