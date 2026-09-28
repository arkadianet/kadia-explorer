import { describe, expect, it } from 'vitest';
import { exactShare, holderConcentration } from '$lib/token/concentration';
import type { TokenHolderContext, TokenHolderDto } from '$lib/api/types';

const context: TokenHolderContext = {
	supply: '100',
	holder_count: 20,
	definition: 'indexed_emission_minus_burned'
};
const rows = (amounts: string[]): TokenHolderDto[] =>
	amounts.map((amount, index) => ({
		address: null,
		tree_hash: index.toString(16).padStart(64, '0'),
		amount,
		share_pct: '0.00'
	}));

describe('holder concentration', () => {
	it('uses exact units and the holder snapshot supply instead of rounded percentages', () => {
		const result = holderConcentration(rows(['70', '20']), context)!;
		expect(exactShare(result.largest, result.supply)).toBe('70.00%');
		expect(exactShare(result.sum, result.supply)).toBe('90.00%');
		expect(result).toMatchObject({ loaded: 2, total: 20, topCount: 2 });
	});
	it('caps the top group at ten while counting all loaded rows', () => {
		const result = holderConcentration(rows(Array(15).fill('1')), context)!;
		expect(result).toMatchObject({ top: 10n, sum: 15n, loaded: 15, topCount: 10 });
	});
	it('preserves precision above the safe integer range and never calls dust zero', () => {
		const supply = 18_446_744_073_709_551_615n;
		expect(exactShare(supply - 1n, supply)).toBe('99.99%');
		expect(exactShare(1n, supply)).toBe('<0.01%');
		expect(exactShare(0n, supply)).toBe('0%');
		expect(exactShare(0n, 0n)).toBe('—');
	});
	it('withholds concentration for absent or inconsistent snapshots', () => {
		expect(holderConcentration(rows(['10']), null)).toBeNull();
		expect(holderConcentration(rows(['101']), context)).toBeNull();
		expect(holderConcentration(rows(['1', '2']), context)).toBeNull();
		expect(holderConcentration(rows(['-1']), context)).toBeNull();
		expect(holderConcentration(rows(['1']), { ...context, holder_count: 0 })).toBeNull();
		const duplicate = rows(['2', '1']);
		duplicate[1].tree_hash = duplicate[0].tree_hash;
		expect(holderConcentration(duplicate, context)).toBeNull();
	});
});
