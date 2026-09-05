import { NANO } from './amount';

/** Blocks paying the fixed 75 ERG/block reward before the reward starts decreasing. */
const FIXED_RATE_BLOCKS = 525_600;
const FIXED_RATE_REWARD = 75n;

/** After the fixed-rate period, the reward drops by 3 ERG every EPOCH_BLOCKS blocks. */
const EPOCH_BLOCKS = 64_800n;
const EPOCH_DECREASE = 3n;
const MIN_REWARD = 3n;

/**
 * Last height that pays a block reward. The reward never falls below 3 ERG: after the
 * fixed-rate period it steps 72, 69, … 3 over 24 epochs of 64,800 blocks, and emission then
 * stops — 525,600 + 24 × 64,800 = 2,080,800. Blocks after this height accrue nothing.
 */
export const EMISSION_END_HEIGHT = 525_600 + 24 * 64_800;

/** Total ever emitted: 525,600 × 75 + 64,800 × (72 + 69 + … + 3) = 97,740,000 ERG. */
export const TOTAL_SUPPLY_NANO = 97_740_000n * NANO;

/**
 * Cumulative Ergo emission (in nanoERG) for blocks `1..height`, per the pre-EIP-27 schedule:
 * 75 ERG/block for the first 525,600 blocks, then a reward that drops by 3 ERG every 64,800
 * blocks (72, 69, 66, …) down to the last 3 ERG epoch, after which emission ends at height
 * `EMISSION_END_HEIGHT` with `TOTAL_SUPPLY_NANO` emitted in total.
 *
 * This ignores EIP-27's re-emission (which returns collected storage-rent and unspendable
 * "no-premine" funds back into circulation on a different schedule) and treats reissued supply
 * as if it never left circulation — so the result is an approximation of circulating supply,
 * not an exact figure. Good enough for the "≈ % of supply" richlist column.
 */
export function circulatingAt(height: number): bigint {
	if (height <= 0) return 0n;

	// Emission stops at EMISSION_END_HEIGHT; later heights add nothing.
	const h = BigInt(Math.min(height, EMISSION_END_HEIGHT));

	if (h <= BigInt(FIXED_RATE_BLOCKS)) {
		return h * FIXED_RATE_REWARD * NANO;
	}

	let total = BigInt(FIXED_RATE_BLOCKS) * FIXED_RATE_REWARD * NANO;
	let remaining = h - BigInt(FIXED_RATE_BLOCKS);
	let reward = FIXED_RATE_REWARD - EPOCH_DECREASE;

	while (remaining > 0n) {
		const blocksThisEpoch = remaining < EPOCH_BLOCKS ? remaining : EPOCH_BLOCKS;
		// The clamp above keeps `reward` at or above MIN_REWARD; the guard is belt and braces.
		const effectiveReward = reward < MIN_REWARD ? MIN_REWARD : reward;
		total += blocksThisEpoch * effectiveReward * NANO;
		remaining -= blocksThisEpoch;
		reward -= EPOCH_DECREASE;
	}

	return total;
}
