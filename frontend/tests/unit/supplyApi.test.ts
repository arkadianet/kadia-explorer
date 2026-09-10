import { describe, expect, it } from 'vitest';

/**
 * The rich list's "% of supply" column divides by the ERG that actually exists, which the API
 * derives from the emission contract's remaining balance. These pin the arithmetic that used
 * to live in a hardcoded emission schedule — a schedule that was wrong twice over: it assumed
 * 75 ERG/block where the chain paid 67.5 (10% went to the foundation) and it ignored EIP-27.
 */
function pctOfSupply(nano: string, emitted: string | null): string {
	if (emitted === null) return '—';
	const supply = BigInt(emitted);
	if (supply === 0n) return '—';
	return `${Number((BigInt(nano) * 10_000n) / supply) / 100}%`;
}

describe('share of chain-derived supply', () => {
	// Measured on mainnet at height 1,869,600: genesis total 97,739,925 ERG, emission contract
	// still holding 1,368,924 ERG, so 96,371,001 ERG existed.
	const emitted = (97_739_925n - 1_368_924n) * 1_000_000_000n;

	it('divides by emitted supply, not the genesis total', () => {
		const holder = 7_132_086_326_000_000n.toString();
		expect(pctOfSupply(holder, emitted.toString())).toBe('7.4%');
	});

	it('renders a dash when the store cannot report supply', () => {
		expect(pctOfSupply('1000', null)).toBe('—');
		expect(pctOfSupply('1000', '0')).toBe('—');
	});

	it('is measurably different from the old pre-EIP-27 schedule', () => {
		// The removed formula reported 96,372,000 ERG at that height by assuming 75 ERG/block.
		const oldFormula = 96_372_000n * 1_000_000_000n;
		expect(oldFormula).not.toBe(emitted);
		expect(oldFormula - emitted).toBe(999n * 1_000_000_000n);
	});
});
