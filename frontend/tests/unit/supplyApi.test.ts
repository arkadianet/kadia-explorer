import { describe, expect, it } from 'vitest';
import { pctOfGenesis } from '$lib/format/supply';

describe('share of total genesis allocation', () => {
	it('includes the emission reserve in the denominator just as the rich list does', () => {
		expect(pctOfGenesis('93409132500000000', '97739925000000000')).toBe('95.56%');
	});
	it('formats all holdings as 100 percent and unknown supply as a dash', () => {
		expect(pctOfGenesis('97739925000000000', '97739925000000000')).toBe('100.00%');
		expect(pctOfGenesis('1000', null)).toBe('—');
		expect(pctOfGenesis('1000', '0')).toBe('—');
	});
});
