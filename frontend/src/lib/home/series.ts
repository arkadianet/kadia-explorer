import type { BlockDto } from '$lib/api/types';

/**
 * Bucketing for the home page's shapes.
 *
 * Every series here is anchored on the **newest block the indexer has**, not on the wall
 * clock. While the index is catching up those two are far apart, and a series anchored on
 * `Date.now()` would be an empty chart with a misleading axis; anchored on the chain it is a
 * true picture of the stretch of chain the page is showing. The wording on screen says
 * "of the indexed chain" for exactly that reason.
 */

export const HOUR_MS = 3_600_000;

/** The blocks within `spanMs` of the newest block, newest first (the API's own order). */
export function chainWindow(blocks: BlockDto[], spanMs: number): BlockDto[] {
	if (blocks.length === 0) return [];
	const newest = blocks[0]!.timestamp;
	return blocks.filter((b) => b.timestamp > newest - spanMs);
}

/**
 * `count` buckets of `bucketMs`, oldest first, ending at the newest block. `value` maps a
 * block to what is being summed — 1 for "blocks per bucket", `tx_count` for transactions.
 */
export function buckets(
	blocks: BlockDto[],
	bucketMs: number,
	count: number,
	value: (b: BlockDto) => number = () => 1
): number[] {
	const out = new Array<number>(count).fill(0);
	if (blocks.length === 0) return out;
	const end = blocks[0]!.timestamp;
	const start = end - bucketMs * count;
	for (const b of blocks) {
		if (b.timestamp <= start || b.timestamp > end) continue;
		// The newest block belongs in the last bucket, so index from the end.
		const i = count - 1 - Math.floor((end - b.timestamp) / bucketMs);
		if (i >= 0 && i < count) out[i] += value(b);
	}
	return out;
}

/** Sums a decimal-string field (nanoERG) across blocks, as a BigInt. */
export function sumNano(blocks: BlockDto[], field: 'fees' | 'reward'): bigint {
	let total = 0n;
	for (const b of blocks) total += BigInt(b[field]);
	return total;
}

/**
 * Hashrate implied by a header's difficulty: `difficulty / block time`, with Ergo's 120 s
 * target as the block time. This is the standard estimator — it says what hashrate would, on
 * average, produce this difficulty at the target interval — and it is stated as such wherever
 * the number is shown.
 */
export const TARGET_BLOCK_SECONDS = 120;

export function hashrateHs(difficulty: string): number {
	return Number(BigInt(difficulty) / BigInt(TARGET_BLOCK_SECONDS));
}

const UNITS = ['H/s', 'kH/s', 'MH/s', 'GH/s', 'TH/s', 'PH/s', 'EH/s'];

export function formatHashrate(hs: number): string {
	if (!Number.isFinite(hs) || hs <= 0) return '—';
	let i = 0;
	let v = hs;
	while (v >= 1000 && i < UNITS.length - 1) {
		v /= 1000;
		i++;
	}
	return `${v.toFixed(v >= 100 ? 0 : 2)} ${UNITS[i]}`;
}
