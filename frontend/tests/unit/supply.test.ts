import { describe, it, expect } from 'vitest';
import { circulatingAt, EMISSION_END_HEIGHT, TOTAL_SUPPLY_NANO } from '$lib/format/supply';
import { NANO } from '$lib/format/amount';

describe('circulatingAt', () => {
	it('is zero before the chain starts', () => {
		expect(circulatingAt(0)).toBe(0n);
	});

	it('pays 75 ERG for the first block', () => {
		expect(circulatingAt(1)).toBe(75n * NANO);
	});

	it('sums the fixed-rate period exactly', () => {
		expect(circulatingAt(525_600)).toBe(39_420_000n * NANO);
	});

	it('drops to 72 ERG/block for the first block after the fixed-rate period', () => {
		expect(circulatingAt(525_601)).toBe((39_420_000n + 72n) * NANO);
	});

	it('sums a full 64,800-block step at 72 ERG/block', () => {
		expect(circulatingAt(590_400)).toBe(circulatingAt(525_600) + 72n * 64_800n * NANO);
	});

	it('ends emission at height 2,080,800 with the full 97,740,000 ERG', () => {
		expect(EMISSION_END_HEIGHT).toBe(2_080_800);
		expect(TOTAL_SUPPLY_NANO).toBe(97_740_000n * NANO);
		expect(circulatingAt(2_080_800)).toBe(97_740_000n * NANO);
	});

	it('pays the 3 ERG floor for the last emitting block and nothing after', () => {
		expect(circulatingAt(2_080_800) - circulatingAt(2_080_799)).toBe(3n * NANO);
		expect(circulatingAt(2_080_801)).toBe(circulatingAt(2_080_800));
		expect(circulatingAt(5_000_000)).toBe(circulatingAt(2_080_800));
		expect(circulatingAt(5_000_000)).toBe(97_740_000n * NANO);
	});

	it('is monotonically non-decreasing', () => {
		let prev = circulatingAt(0);
		for (const h of [
			1, 2, 525_600, 525_601, 590_400, 590_401, 1_000_000, 2_080_799, 2_080_800, 2_080_801,
			5_000_000
		]) {
			const cur = circulatingAt(h);
			expect(cur >= prev).toBe(true);
			prev = cur;
		}
	});
});
