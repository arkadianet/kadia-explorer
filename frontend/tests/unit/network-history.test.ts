import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	chartHeights,
	createHistoryLoader,
	fetchHistory,
	historyQuery,
	range,
	validateHistory,
	type HistoryState,
	type NetworkHistory
} from '$lib/network/history';

const requested = range('1', '4', '2');
function fixture(): NetworkHistory {
	const first = {
		from_height: 1,
		to_height: 2,
		block_count: 2,
		transaction_count: '7',
		fees: '9007199254740993',
		size_bytes: '100',
		difficulty_min: '100',
		difficulty_max: '300',
		difficulty_end: '100',
		first_timestamp: 4000,
		last_timestamp: 1000,
		earliest_timestamp: 1000,
		latest_timestamp: 4000
	};
	const last = {
		from_height: 3,
		to_height: 4,
		block_count: 2,
		transaction_count: '13',
		fees: '11',
		size_bytes: '300',
		difficulty_min: '200',
		difficulty_max: '400',
		difficulty_end: '400',
		first_timestamp: 3000,
		last_timestamp: 2000,
		earliest_timestamp: 2000,
		latest_timestamp: 3000
	};
	return {
		scope: 'canonical_block_headers',
		consistency: 'single_reader',
		complete: true,
		full_history: false,
		partial_from: 1,
		indexed_height: 8,
		anchor: { height: 4, block_id: 'a'.repeat(64) },
		requested_buckets: 2,
		bucket_width: 2,
		totals: {
			...first,
			to_height: 4,
			block_count: 4,
			transaction_count: '20',
			fees: '9007199254741004',
			size_bytes: '400',
			difficulty_max: '400',
			difficulty_end: '400',
			last_timestamp: 2000
		},
		buckets: [first, last]
	};
}
afterEach(() => {
	vi.unstubAllGlobals();
	vi.useRealTimers();
});
describe('bounded exact network ranges', () => {
	it('accepts the full allowed height count and makes a replayable pinned query', () => {
		expect(range('1', '20160', '120')).toEqual({ from_height: 1, to_height: 20160, buckets: 120 });
		expect(historyQuery(range('1', '4', '2', 'a'.repeat(64)))).toContain(
			'end_block_id=' + 'a'.repeat(64)
		);
	});
	it.each([
		['0', '4', '2'],
		['01', '4', '2'],
		['5', '4', '2'],
		['1', '20161', '2'],
		['1', '4', '0'],
		['1', '4', '121'],
		['1', '4', '1.5'],
		['1', '4294967296', '2']
	])('rejects invalid selection %s/%s/%s', (from, to, buckets) => {
		expect(() => range(from, to, buckets)).toThrow();
	});
	it('preserves precise amounts, canonical height order and timestamp regressions', () => {
		const data = validateHistory(fixture(), requested);
		expect(data.totals.fees).toBe('9007199254741004');
		expect(data.totals.first_timestamp).toBeGreaterThan(data.totals.last_timestamp);
		expect(data.totals.earliest_timestamp).toBe(1000);
		expect(chartHeights(data.buckets, 'fees')).toEqual([100, 0]);
		expect(chartHeights(data.buckets, 'transaction_count')).toEqual([53.8, 100]);
	});
	it('keeps u128 difficulty exact and handles a zero-valued chart', () => {
		const data = fixture();
		const max = ((1n << 128n) - 1n).toString();
		data.buckets[1].difficulty_max = max;
		data.buckets[1].difficulty_end = max;
		data.totals.difficulty_max = max;
		data.totals.difficulty_end = max;
		expect(validateHistory(data, requested).totals.difficulty_end).toBe(max);
		expect(chartHeights(data.buckets, 'difficulty_end')).toEqual([0, 100]);
		data.buckets.forEach((bucket) => (bucket.fees = '0'));
		expect(chartHeights(data.buckets, 'fees')).toEqual([0, 0]);
	});
	it.each([
		(data: NetworkHistory) => {
			data.complete = false as true;
		},
		(data: NetworkHistory) => {
			data.anchor.height++;
		},
		(data: NetworkHistory) => {
			data.anchor.block_id = 'not-an-id';
		},
		(data: NetworkHistory) => {
			data.bucket_width++;
		},
		(data: NetworkHistory) => {
			data.buckets.pop();
		},
		(data: NetworkHistory) => {
			data.buckets[1].from_height++;
		},
		(data: NetworkHistory) => {
			data.totals.fees = '9007199254741005';
		},
		(data: NetworkHistory) => {
			data.totals.fees = '09007199254741004';
		},
		(data: NetworkHistory) => {
			data.totals.difficulty_min = '101';
		},
		(data: NetworkHistory) => {
			data.buckets[1].difficulty_end = '401';
		},
		(data: NetworkHistory) => {
			data.totals.latest_timestamp = 3000;
		},
		(data: NetworkHistory) => {
			data.partial_from = 2;
		},
		(data: NetworkHistory) => {
			data.full_history = true;
		},
		(data: NetworkHistory) => {
			data.indexed_height = 3;
		}
	])('rejects inconsistent or partial responses %#', (mutate) => {
		const data = fixture();
		mutate(data);
		expect(() => validateHistory(data, requested)).toThrow();
	});
	it('rejects a changed pinned end block', () => {
		expect(() =>
			validateHistory(fixture(), { ...requested, end_block_id: 'b'.repeat(64) })
		).toThrow();
	});
});
describe('explicit network loader', () => {
	it('does not fetch until asked and clears old results before a failed refresh', async () => {
		const states: HistoryState[] = [];
		const fetcher = vi
			.fn()
			.mockResolvedValueOnce(fixture())
			.mockRejectedValueOnce(new Error('Pinned block changed'));
		const loader = createHistoryLoader((state) => states.push(state), fetcher);
		expect(fetcher).not.toHaveBeenCalled();
		await loader.load(requested);
		expect(states.at(-1)?.data?.totals.fees).toBe('9007199254741004');
		await loader.load(requested);
		expect(states.at(-2)).toEqual({ loading: true, data: null, error: null });
		expect(states.at(-1)).toEqual({ loading: false, data: null, error: 'Pinned block changed' });
		loader.stop();
	});
	it('editing the range aborts work and discards late completion', async () => {
		let resolve!: (value: unknown) => void;
		let signal: AbortSignal | undefined;
		const states: HistoryState[] = [];
		const loader = createHistoryLoader(
			(state) => states.push(state),
			(_request, active) => {
				signal = active;
				return new Promise((done) => (resolve = done));
			}
		);
		const pending = loader.load(requested);
		loader.clear();
		expect(signal?.aborted).toBe(true);
		resolve(fixture());
		await pending;
		expect(states.at(-1)).toEqual({ loading: false, data: null, error: null });
		loader.stop();
	});
	it('a replaced request and unmounted page cannot publish an old result', async () => {
		let resolve!: (value: unknown) => void;
		const states: HistoryState[] = [];
		const fetcher = vi
			.fn()
			.mockImplementationOnce(() => new Promise((done) => (resolve = done)))
			.mockResolvedValueOnce(fixture());
		const loader = createHistoryLoader((state) => states.push(state), fetcher);
		const old = loader.load(requested);
		await loader.load(requested);
		const count = states.length;
		loader.stop();
		resolve(fixture());
		await old;
		expect(states.length).toBe(count);
	});
	it('bounds a request timeout and leaves retry available', async () => {
		vi.useFakeTimers();
		const states: HistoryState[] = [];
		const loader = createHistoryLoader(
			(state) => states.push(state),
			(_request, signal) =>
				new Promise((_done, reject) =>
					signal.addEventListener('abort', () => reject(new Error('aborted')))
				)
		);
		const pending = loader.load(requested);
		await vi.advanceTimersByTimeAsync(10_001);
		await pending;
		expect(states.at(-1)?.loading).toBe(false);
		expect(states.at(-1)?.error).toContain('timed out');
		loader.stop();
	});
	it('requests only same-origin data and preserves query pins', async () => {
		const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(fixture())));
		vi.stubGlobal('fetch', fetcher);
		await fetchHistory(
			{ ...requested, end_block_id: 'a'.repeat(64) },
			new AbortController().signal
		);
		expect(fetcher.mock.calls[0][0]).toBe(
			'/v1/network/history?from_height=1&to_height=4&buckets=2&end_block_id=' + 'a'.repeat(64)
		);
		expect(fetcher.mock.calls[0][1]).toMatchObject({ credentials: 'omit', redirect: 'error' });
	});
	it('reports a string detail from a failed response', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(Response.json({ detail: 'Pinned block changed' }, { status: 409 }))
		);
		await expect(fetchHistory(requested, new AbortController().signal)).rejects.toThrow(
			'Pinned block changed'
		);
	});
	it.each([
		null,
		'',
		'null',
		'<html>Unavailable</html>',
		'{',
		'{}',
		'{"detail":42}',
		new Uint8Array([255])
	])('uses the fallback for an unavailable failure detail (%s)', async (body) => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(body, { status: 503 })));
		await expect(fetchHistory(requested, new AbortController().signal)).rejects.toThrow(
			'Network history is unavailable.'
		);
	});
	it('rejects oversized streamed responses before JSON parsing', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(new Uint8Array(256 * 1024 + 1))));
		await expect(fetchHistory(requested, new AbortController().signal)).rejects.toThrow(
			'response limit'
		);
	});
});
