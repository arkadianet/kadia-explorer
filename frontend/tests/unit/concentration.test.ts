import { describe, expect, it } from 'vitest';
import {
	exactShare,
	holderConcentration,
	holdersCsv,
	holderCsvIssue,
	MAX_HOLDER_CSV_ROWS,
	type HolderExportContext
} from '$lib/token/concentration';
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

const exportContext: HolderExportContext = {
	id: 'a'.repeat(64),
	decimals: 2,
	context,
	anchor: { height: 123, block_id: 'b'.repeat(64) }
};
describe('loaded holder CSV', () => {
	it('preserves exact raw amounts, supply, ordinal and snapshot without claiming all holders', () => {
		const csv = holdersCsv(rows(['9007199254740993', '1']), {
			...exportContext,
			context: { ...context, supply: '18446744073709551615' }
		});
		const lines = csv
			.trim()
			.split('\r\n')
			.map((line) => line.split(',').map((value) => value.slice(1, -1)));
		const first = Object.fromEntries(lines[0].map((column, index) => [column, lines[1][index]]));
		expect(lines).toHaveLength(3);
		expect(first).toMatchObject({
			token_id: exportContext.id,
			ordinal: '1',
			address: '',
			tree_hash: '0'.repeat(64),
			amount_raw: '9007199254740993',
			decimals: '2',
			indexed_supply_raw: '18446744073709551615',
			supply_definition: 'indexed_emission_minus_burned',
			snapshot_height: '123',
			snapshot_block_id: 'b'.repeat(64),
			export_scope: 'loaded_holder_scripts_only',
			loaded_script_count: '2',
			indexed_holder_script_count: '20'
		});
		expect(lines[2][1]).toBe('2');
		expect(
			holdersCsv(rows(['1']), { ...exportContext, decimals: null })
				.split('\r\n')[1]
				.split(',')[5]
		).toBe('""');
	});
	it.each(['=SUM(1,2)', '+3', '-2', '@name', '  =1', '\ttext', '\rtext', '\ntext'])(
		'neutralizes formula-like text %j while quoting delimiters and line breaks',
		(address) => {
			const row = { ...rows(['1'])[0], address: address + ',"quoted"' };
			const csv = holdersCsv([row], exportContext);
			expect(csv).toContain(`"'${row.address.replaceAll('"', '""')}"`);
		}
	);
	it('rejects inconsistent supply, invalid amounts, repeated scripts and missing anchors', () => {
		for (const invalid of [
			rows(['101']),
			rows(['2', '3']),
			rows(['-1']),
			[rows(['1'])[0], rows(['1'])[0]]
		])
			expect(() => holdersCsv(invalid, exportContext)).toThrow('consistent holder snapshot');
		for (const options of [
			{ ...exportContext, context: null },
			{ ...exportContext, anchor: null },
			{ ...exportContext, anchor: { height: -1, block_id: 'b'.repeat(64) } },
			{ ...exportContext, anchor: { height: 1, block_id: 'invalid' } }
		])
			expect(() => holdersCsv(rows(['1']), options)).toThrow('consistent holder snapshot');
	});
	it('validates metadata and refuses to silently truncate row or UTF-8 byte limits', () => {
		expect(holderCsvIssue([], exportContext)).toContain('Load holder rows');
		expect(() => holdersCsv(rows(Array(MAX_HOLDER_CSV_ROWS + 1).fill('1')), exportContext)).toThrow(
			'5,000'
		);
		expect(() => holdersCsv(rows(['1']), { ...exportContext, decimals: 256 })).toThrow(
			'invalid export fields'
		);
		expect(() => holdersCsv(rows(['1']), { ...exportContext, id: '=invalid' })).toThrow(
			'invalid export fields'
		);
		const oversized = rows(Array(180).fill('1')).map((row) => ({
			...row,
			address: '界'.repeat(4096)
		}));
		expect(() =>
			holdersCsv(oversized, {
				...exportContext,
				context: { ...context, supply: '1000', holder_count: 180 }
			})
		).toThrow('2 MiB');
	});
});
