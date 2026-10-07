import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../src/lib/api/client';
import type {
	RentScheduleBatchDto,
	RentScheduleItemDto,
	RentSchedulePageDto
} from '../../src/lib/api/types';
import {
	createScheduleLoader,
	emptySchedule,
	scheduleBatches,
	scheduleQuery,
	scheduleTotals,
	validateSchedule
} from '../../src/lib/rent/schedule';

const id = (n: number) => n.toString(16).padStart(64, '0');
const anchor = { height: 200, block_id: id(10000) };
const time = Date.UTC(2026, 9, 8);
const query = { window: '24h', mode: 'all' } as const;
function item(n = 1): RentScheduleItemDto {
	return {
		box_id: id(n),
		tx_id: id(n + 10000),
		creation_height: 0,
		maturity_height: 201,
		estimated_maturity_ms: time + 120_000,
		value: '1000000',
		consensus_fee_nano: '125000000',
		collectible_due_nano: '1000000',
		collectible: true,
		full_claim: true,
		protocol_constrained: false,
		tokens: [{ id: id(9000), amount: '9007199254740993', name: 'Exact token', decimals: null }]
	};
}
function batch(hour = 1, amount = '1000000'): RentScheduleBatchDto {
	return {
		estimated_hour_start_ms: time + hour * 3_600_000,
		box_count: 1,
		collectible_due_nano: amount,
		full_claim_count: 1,
		full_claim_value_nano: amount,
		selected_token_full_claim_amount: null
	};
}
function response(start = 1, count = 1, more = false, continuation = false): RentSchedulePageDto {
	return {
		items: Array.from({ length: count }, (_, offset) => item(start + offset)),
		next_cursor: more ? `201:${start + count}` : null,
		next_snapshot: more ? `snapshot-${start}` : null,
		consistency: 'strict',
		anchor,
		observed_anchor: anchor,
		schedule_context: {
			scope: 'indexed_unspent_rent_window',
			anchor_timestamp_ms: time,
			from_height: 201,
			to_height: 920,
			target_block_seconds: 120,
			window: '24h',
			mode: 'all',
			token_id: null,
			full_history: true,
			partial_from: null,
			scanned: count,
			scan_limit: 2000,
			scan_limit_reached: false,
			complete: !more,
			batch_scanned: continuation ? null : 1000,
			batch_scan_limit: 50_000,
			batch_complete: continuation ? null : true,
			batch_stop_reason: null,
			rent_factor: 1_250_000,
			fee_arithmetic: 'wrapping_i32'
		},
		batches: continuation ? null : [batch()]
	};
}
function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => {
		resolve = done;
	});
	return { promise, resolve };
}

describe('rent calendar exactness and scope', () => {
	it('accepts all four linkable windows, normalizes token IDs and blocks malformed filters', () => {
		for (const window of ['24h', '7d', '30d', '90d'])
			expect(scheduleQuery(new URLSearchParams({ window })).query.window).toBe(window);
		expect(
			scheduleQuery(new URLSearchParams({ token_id: 'AA'.repeat(32), mode: 'full_claim' }))
		).toEqual({ query: { ...query, mode: 'full_claim', token_id: 'aa'.repeat(32) }, error: null });
		expect(scheduleQuery(new URLSearchParams('token_id=not-a-token')).error).toContain(
			'64-character'
		);
		expect(scheduleQuery(new URLSearchParams('window=__proto__&mode=toString')).query).toEqual(
			query
		);
	});
	it('uses exact BigInt sums and ranks complete UTC day buckets, retaining hourly evidence', () => {
		const hours = [
			batch(1, '9007199254740993'),
			batch(2, '9007199254740993'),
			batch(25, '18014398509481985')
		];
		const days = scheduleBatches(hours, '7d');
		expect(days[0]).toMatchObject({
			estimated_hour_start_ms: time,
			collectible_due_nano: '18014398509481986',
			box_count: 2
		});
		expect(days[0].hours).toHaveLength(2);
		expect(scheduleTotals(hours).due).toBe(36028797018963971n);
		expect(scheduleBatches(hours, '24h')).toHaveLength(3);
	});
	it('supports independent box/full-claim/token rankings without floating-point amounts', () => {
		const hours = [batch(1, '1'), batch(2, '2')];
		hours[0].box_count = 4;
		hours[0].full_claim_count = 3;
		hours[0].selected_token_full_claim_amount = '9007199254740993';
		hours[1].selected_token_full_claim_amount = '9007199254740992';
		for (const ranking of ['boxes', 'full_claims', 'token'] as const)
			expect(scheduleBatches(hours, '24h', ranking)[0].estimated_hour_start_ms).toBe(
				hours[0].estimated_hour_start_ms
			);
		expect(scheduleBatches(hours, '24h', 'rent')[0].estimated_hour_start_ms).toBe(
			hours[1].estimated_hour_start_ms
		);
	});
	it('accepts capped charges and constrained exclusions, but rejects false full-claim or fee assertions', () => {
		const data = response();
		expect(validateSchedule(data, query)).toBe(data);
		data.items[0].protocol_constrained = true;
		data.items[0].collectible = false;
		data.items[0].full_claim = false;
		data.items[0].collectible_due_nano = '0';
		expect(validateSchedule(data, query)).toBe(data);
		data.items[0].full_claim = true;
		expect(() => validateSchedule(data, query)).toThrow('inconsistent');
		data.items[0].full_claim = false;
		data.items[0].consensus_fee_nano = '9999999999';
		expect(() => validateSchedule(data, query)).toThrow('inconsistent');
	});
	it('does not accept malformed coverage as a complete ranking', () => {
		const mutate: ((page: RentSchedulePageDto) => void)[] = [
			(page) => {
				Object.assign(page.schedule_context, { full_history: 'false' });
			},
			(page) => {
				page.schedule_context.batch_scanned = 0;
			},
			(page) => {
				page.schedule_context.batch_complete = false;
			},
			(page) => {
				page.schedule_context.scanned = 2001;
			},
			(page) => {
				page.observed_anchor = { ...anchor, block_id: id(999) };
			},
			(page) => {
				page.items[0].estimated_maturity_ms++;
			},
			(page) => {
				page.items[0].tokens.push({ ...page.items[0].tokens[0] });
			}
		];
		for (const change of mutate) {
			const page = response();
			change(page);
			expect(() => validateSchedule(page, query)).toThrow('inconsistent');
		}
	});
	it('keeps valid partial overview separate from a complete item page', () => {
		const data = response();
		data.schedule_context.full_history = false;
		data.schedule_context.partial_from = 100;
		data.schedule_context.batch_complete = false;
		data.schedule_context.batch_stop_reason = 'deadline';
		expect(validateSchedule(data, query).schedule_context).toMatchObject({
			complete: true,
			batch_complete: false,
			full_history: false
		});
	});
});

describe('rent schedule anchored paging', () => {
	it('preserves independent first-page batches across empty scan-limited continuations', async () => {
		let current = emptySchedule();
		const next = response(2, 0, true, true);
		next.next_cursor = '202:3000';
		next.schedule_context.scanned = 2000;
		next.schedule_context.scan_limit_reached = true;
		const read = vi.fn().mockResolvedValue(next);
		const loader = createScheduleLoader((value) => {
			current = value;
		}, read);
		const first = response(1, 1, true);
		loader.seed(query, first, null);
		await loader.more();
		expect(current.first).toBe(first);
		expect(current.items).toHaveLength(1);
		expect(current.scanLimited).toBe(true);
		expect(current.cursor).toBe('202:3000');
		expect(read.mock.calls[0].slice(0, 3)).toEqual([query, first.next_cursor, first.next_snapshot]);
	});
	it('prevents overlapping loads and discards late responses after filters change', async () => {
		let current = emptySchedule();
		const delayed = deferred<RentSchedulePageDto>();
		const read = vi.fn().mockReturnValue(delayed.promise);
		const loader = createScheduleLoader((value) => {
			current = value;
		}, read);
		loader.seed(query, response(1, 1, true), null);
		const loading = loader.more();
		await loader.more();
		expect(read).toHaveBeenCalledTimes(1);
		loader.seed(query, response(9), null);
		delayed.resolve(response(2, 1, false, true));
		await loading;
		expect(current.items[0].box_id).toBe(id(9));
		expect((read.mock.calls[0][3] as AbortSignal).aborted).toBe(true);
	});
	it('retains old records explicitly stale after refresh failure, then clears warning on success', async () => {
		let current = emptySchedule();
		const read = vi
			.fn()
			.mockRejectedValueOnce(new ApiError(503, 'Unavailable', 'Try again'))
			.mockResolvedValueOnce(response(2));
		const loader = createScheduleLoader((value) => {
			current = value;
		}, read);
		loader.seed(query, response(), null);
		await loader.refresh();
		expect(current).toMatchObject({ stale: true, error: 'Try again' });
		expect(current.items[0].box_id).toBe(id(1));
		await loader.refresh();
		expect(current).toMatchObject({ stale: false, error: null });
		expect(current.items[0].box_id).toBe(id(2));
	});
	it.each(['http', 'anchor'] as const)(
		'clears evidence on %s snapshot conflict and never appends across anchors',
		async (kind) => {
			let current = emptySchedule();
			const changed = response(2, 1, false, true);
			changed.anchor = changed.observed_anchor = { ...anchor, block_id: id(555) };
			const read =
				kind === 'http'
					? vi.fn().mockRejectedValue(new ApiError(409, 'Changed', 'Changed'))
					: vi.fn().mockResolvedValue(changed);
			const loader = createScheduleLoader((value) => {
				current = value;
			}, read);
			loader.seed(query, response(1, 1, true), null);
			await loader.more();
			expect(current.first).toBeNull();
			expect(current.items).toHaveLength(0);
			expect(current.error).toContain('indexed chain changed');
		}
	);
	it('requests only remaining capacity and stops at exactly500 rendered records', async () => {
		let current = emptySchedule();
		let start = 52;
		const read = vi.fn(async (_query, _cursor, _snapshot, _signal, limit: number) => {
			const next = response(start, limit, true, true);
			start += limit;
			return next;
		});
		const loader = createScheduleLoader((value) => {
			current = value;
		}, read);
		loader.seed(query, response(1, 51, true), null);
		for (let i = 0; i < 5; i++) await loader.more();
		expect(current.items).toHaveLength(500);
		expect(read.mock.calls.at(-1)![4]).toBe(49);
		await loader.more();
		expect(read).toHaveBeenCalledTimes(5);
	});
});
