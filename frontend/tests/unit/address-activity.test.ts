import { describe, expect, it } from 'vitest';
import {
	activityCsv,
	loadedActivitySummary,
	parseActivityFilters
} from '../../src/lib/addresses/activity';
import type { AddressActivityDto } from '../../src/lib/api/types';

const anchor = { height: 100, block_id: 'b'.repeat(64) };
const item: AddressActivityDto = {
	id: 'a'.repeat(64),
	height: 99,
	block_id: 'c'.repeat(64),
	timestamp: 1_800_000_000_000,
	index: 0,
	fee: '1100000',
	input_count: 2,
	output_count: 2,
	coverage: { complete: true, resolved_inputs: 2, total_inputs: 2 },
	erg_delta: '-900719925474099312345',
	tokens: [{ id: 'd'.repeat(64), name: '=1+2', decimals: null, delta: '900719925474099312346' }],
	direction: 'mixed',
	asset_match: 'definite'
};
const csv = (items = [item], label = '') =>
	activityCsv(
		items,
		'9address',
		label,
		anchor,
		{ asset: 'all', direction: 'all' },
		{ partialFrom: 10, matchingScanComplete: false }
	);
describe('loaded activity summary', () => {
	it('keeps exact signed arithmetic, every direction count, and the loaded timestamp range', () => {
		const rows: AddressActivityDto[] = [
			item,
			{
				...item,
				direction: 'received',
				erg_delta: '900719925474099312350',
				timestamp: item.timestamp - 5000
			},
			{ ...item, direction: 'sent', erg_delta: '-2' },
			{ ...item, direction: 'neutral', erg_delta: '0', timestamp: item.timestamp + 1000 }
		];
		expect(loadedActivitySummary(rows)).toEqual({
			count: 4,
			nano: '3',
			unresolved: 0,
			directions: { received: 1, sent: 1, mixed: 1, neutral: 1, unknown: 0 },
			fromMs: item.timestamp - 5000,
			toMs: item.timestamp + 1000
		});
	});
	it.each([
		{
			...item,
			coverage: { complete: false, resolved_inputs: 1, total_inputs: 2 },
			direction: 'unknown' as const
		},
		{ ...item, coverage: { complete: true, resolved_inputs: 1, total_inputs: 2 } },
		{ ...item, erg_delta: null }
	])('withholds the entire ERG aggregate for incomplete or missing values', (incomplete) => {
		const summary = loadedActivitySummary([item, incomplete]);
		expect(summary.nano).toBeNull();
		expect(summary.unresolved).toBe(1);
		expect(summary.count).toBe(2);
	});
	it('does not invent a date range for no loaded transactions', () => {
		expect(loadedActivitySummary([])).toMatchObject({
			count: 0,
			nano: '0',
			fromMs: null,
			toMs: null
		});
	});
});
describe('address activity filters', () => {
	it('uses inclusive UTC start and exclusive midnight after the through date', () => {
		expect(
			parseActivityFilters(
				new URLSearchParams('from=2024-02-29&to=2024-02-29&asset=erg&direction=sent')
			)
		).toEqual({
			filters: {
				asset: 'erg',
				direction: 'sent',
				from_ms: Date.UTC(2024, 1, 29),
				to_ms: Date.UTC(2024, 2, 1)
			},
			error: null
		});
	});
	it.each([
		'from=2023-02-29',
		'from=1969-12-31',
		'from=2024-13-01',
		'from=2024-01-03&to=2024-01-02',
		'asset=other',
		'direction=invalid'
	])('rejects invalid filters %s', (query) => {
		expect(parseActivityFilters(new URLSearchParams(query)).error).toBeTruthy();
	});
	it('normalizes token ID case without coercing it to a number', () => {
		expect(
			parseActivityFilters(new URLSearchParams({ asset: 'AB'.repeat(32) })).filters.asset
		).toBe('ab'.repeat(32));
	});
});
describe('bounded activity CSV', () => {
	it('exports exact signed integers, snapshot and coverage, with quoted formula-safe metadata', () => {
		const result = csv([item], ' =SUM("A1")');
		expect(result).toContain('"-900719925474099312345"');
		expect(result).toContain('"900719925474099312346"');
		expect(result).toContain('"\' =SUM(""A1"")"');
		expect(result).toContain('"\'=1+2"');
		expect(result).toContain('"100","' + anchor.block_id + '"');
		expect(result).toContain('"loaded_transactions_only","1","10","false"');
		expect(result.split('\r\n')).toHaveLength(4);
	});
	it('exports unknown coverage with blank deltas, even if a bad response supplies amounts', () => {
		const result = csv([
			{
				...item,
				coverage: { complete: false, resolved_inputs: 1, total_inputs: 2 },
				direction: 'unknown',
				asset_match: 'uncertain'
			}
		]);
		expect(result).not.toContain('90071992547409931234');
		expect(result).toContain('"unknown","false","1","2"');
		expect(result).toContain('"uncertain"');
	});
	it.each(['=1', '+1', '-name', '@SUM(A1)', '\t=1', '\r=1', '\uFEFF=1'])(
		'neutralizes formula-like text %s',
		(name) => {
			expect(csv([{ ...item, tokens: [{ ...item.tokens[0], name }] }])).toContain(`"'${name}"`);
		}
	);
	it('rejects unsafe amount strings and explicit export caps', () => {
		expect(() => csv([{ ...item, erg_delta: '=1' }])).toThrow('exact integer');
		expect(() => csv([])).toThrow('1 and 500');
		expect(() => csv(Array(501).fill(item))).toThrow('1 and 500');
		expect(() => csv([{ ...item, tokens: Array(5000).fill(item.tokens[0]) }])).toThrow('5,000');
		expect(() =>
			csv([{ ...item, tokens: [{ ...item.tokens[0], name: 'x'.repeat(2 * 1024 * 1024) }] }])
		).toThrow('2 MB');
	});
});
