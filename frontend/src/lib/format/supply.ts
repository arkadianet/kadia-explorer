import { NANO } from './amount';

/** Blocks paying the fixed 75 ERG/block reward before the reward starts decreasing. */
const FIXED_RATE_BLOCKS = 525_600;
const FIXED_RATE_REWARD = 75n;

/** After the fixed-rate period, the reward drops by 3 ERG every EPOCH_BLOCKS blocks. */
const EPOCH_BLOCKS = 64_800n;
const EPOCH_DECREASE = 3n;
const MIN_REWARD = 3n;

/**
 * Cumulative Ergo emission (in nanoERG) for blocks `1..height`, per the pre-EIP-27 schedule:
 * 75 ERG/block for the first 525,600 blocks, then a reward that drops by 3 ERG every 64,800
 * blocks (72, 69, 66, …) down to a floor of 3 ERG/block.
 *
 * This ignores EIP-27's re-emission (which returns collected storage-rent and unspendable
 * "no-premine" funds back into circulation on a different schedule) and treats reissued supply
 * as if it never left circulation — so the result is an approximation of circulating supply,
 * not an exact figure. Good enough for the "≈ % of supply" richlist column.
 */
export function circulatingAt(height: number): bigint {
	if (height <= 0) return 0n;

	const h = BigInt(height);

	if (h <= BigInt(FIXED_RATE_BLOCKS)) {
		return h * FIXED_RATE_REWARD * NANO;
	}

	let total = BigInt(FIXED_RATE_BLOCKS) * FIXED_RATE_REWARD * NANO;
	let remaining = h - BigInt(FIXED_RATE_BLOCKS);
	let reward = FIXED_RATE_REWARD - EPOCH_DECREASE;

	while (remaining > 0n) {
		const blocksThisEpoch = remaining < EPOCH_BLOCKS ? remaining : EPOCH_BLOCKS;
		const effectiveReward = reward < MIN_REWARD ? MIN_REWARD : reward;
		total += blocksThisEpoch * effectiveReward * NANO;
		remaining -= blocksThisEpoch;
		reward -= EPOCH_DECREASE;
	}

	return total;
}
