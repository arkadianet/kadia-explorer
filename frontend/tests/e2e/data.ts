/**
 * Concrete ids the specs navigate to, read out of the very same dataset the mock server
 * serves — so a change to the fixtures can never leave a hard-coded id behind.
 */

import { buildDataset, MOCK_ADDRESS } from './mock/fixtures.ts';

const d = buildDataset();

export { MOCK_ADDRESS };

/** The mock's tip height, and the three real fixture blocks. */
export const TIP = 1866002;
export const BLOCK_HEIGHTS = [1866000, 1866001, 1866002] as const;

/** Every block the mock serves (three real, the rest synthesised fillers). */
export const BLOCK_COUNT = d.blocks.length;

export const block = d.blockByHeight.get(1866001)!;
export const blockTxCount = (d.txIdsByHeight.get(1866001) ?? []).length;

/** A transaction with several inputs and outputs, from the middle of block 1866001. */
export const tx = d.txById.get((d.txIdsByHeight.get(1866001) ?? [])[1]!)!;

/** A box the mock reports as claimable right now, and one that is not. */
export const claimableBox = d.rentEligible[0].box;
export const maturingBox = d.rentUpcoming[0].box;

/** A box that was spent inside the fixture range. */
export const spentBox = [...d.boxById.values()].find((b) => b.spent_by !== null)!;

/** Richlist as the mock orders it (balance descending). */
export const richlist = d.richlist;

export const upcomingCount = d.rentUpcoming.length;
export const eligibleCount = d.rentEligible.length;

/** A well-formed but unknown 64-hex id and address, for the not-found paths. */
export const UNKNOWN_HEX = 'f'.repeat(64);
export const UNKNOWN_ADDRESS = '9unknownAddressNeverSeenQqWweeRrTtYyUuPpAaSsDdFf';
