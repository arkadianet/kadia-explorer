import { describe, expect, it } from 'vitest';
import { minerColor, minerHue } from '$lib/format/minerHue';

const PK_A = '0274e729bb6615cbda94d9d176a2f1525068f12b330e38bbbf387232797dfd891f';
const PK_B = '02f5924b1f9b9b0c2a5f1d0a5b4d8c7e6f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d';

describe('minerHue', () => {
	it('is deterministic for the same key', () => {
		expect(minerHue(PK_A)).toBe(minerHue(PK_A));
		expect(minerColor(PK_A)).toBe(minerColor(PK_A));
	});

	it('stays inside the hue circle', () => {
		for (const pk of [PK_A, PK_B, '', '0', 'a'.repeat(66)]) {
			const h = minerHue(pk);
			expect(Number.isInteger(h)).toBe(true);
			expect(h).toBeGreaterThanOrEqual(0);
			expect(h).toBeLessThan(360);
		}
	});

	it('separates different keys', () => {
		expect(minerHue(PK_A)).not.toBe(minerHue(PK_B));
	});

	it('spreads a realistic set of keys over the circle', () => {
		// 200 synthetic keys must not collapse onto a handful of hues, or the strip would show
		// every miner in the same colour.
		const hues = new Set(
			Array.from({ length: 200 }, (_, i) => minerHue(`02${i.toString(16).padStart(64, '0')}`))
		);
		expect(hues.size).toBeGreaterThan(120);
	});

	it('renders a CSS colour with fixed saturation and lightness', () => {
		expect(minerColor(PK_A)).toBe(`hsl(${minerHue(PK_A)} 58% 56%)`);
	});
});
