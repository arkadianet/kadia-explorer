import { describe, expect, it } from 'vitest';
import {
	buckets,
	chainWindow,
	formatHashrate,
	hashrateHs,
	HOUR_MS,
	sumNano
} from '../../src/lib/home/series.ts';
import type { BlockDto } from '../../src/lib/api/types.ts';

/** Blocks newest first, as the API serves them, `n` hours apart. */
function chain(hoursAgo: number[], txPerBlock = 1): BlockDto[] {
	const newest = 1_700_000_000_000;
	return hoursAgo.map((hours, i) => ({
		id: `b${i}`,
		height: 1000 - i,
		parent_id: 'p',
		timestamp: newest - hours * HOUR_MS,
		difficulty: '120',
		miner_pk: 'm',
		tx_count: txPerBlock,
		size: 1,
		fees: '0',
		reward: '1000000000',
		version: 4
	}));
}

describe('chainWindow', () => {
	it('is empty for an empty chain', () => {
		expect(chainWindow([], HOUR_MS)).toEqual([]);
	});

	it('keeps the blocks inside the span, measured from the newest block', () => {
		const blocks = chain([0, 1, 5, 30]);
		expect(chainWindow(blocks, 24 * HOUR_MS).map((b) => b.height)).toEqual([1000, 999, 998]);
	});
});

describe('buckets', () => {
	it('returns zeroes when there is nothing to bucket', () => {
		expect(buckets([], HOUR_MS, 3)).toEqual([0, 0, 0]);
	});

	it('puts the newest block in the last bucket and walks backwards from there', () => {
		expect(buckets(chain([0, 1, 2]), HOUR_MS, 3)).toEqual([1, 1, 1]);
		expect(buckets(chain([0, 2]), HOUR_MS, 3)).toEqual([1, 0, 1]);
	});

	it('sums the mapped value rather than counting', () => {
		expect(buckets(chain([0, 0, 1]), HOUR_MS, 2, (b) => b.tx_count * 10)).toEqual([10, 20]);
	});

	it('drops blocks older than the whole window', () => {
		expect(buckets(chain([0, 99]), HOUR_MS, 4)).toEqual([0, 0, 0, 1]);
	});
});

describe('sumNano', () => {
	it('adds decimal-string amounts through BigInt', () => {
		expect(sumNano(chain([0, 1, 2]), 'reward')).toBe(3_000_000_000n);
	});
});

describe('hashrate', () => {
	it('divides difficulty by the 120 s target block time', () => {
		expect(hashrateHs('120000000')).toBe(1_000_000);
	});

	it('scales the unit and says nothing when there is nothing to say', () => {
		expect(formatHashrate(1_000_000)).toBe('1.00 MH/s');
		expect(formatHashrate(3_240_000_000_000)).toBe('3.24 TH/s');
		expect(formatHashrate(0)).toBe('—');
	});
});
