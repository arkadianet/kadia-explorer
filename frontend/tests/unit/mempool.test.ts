import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	createMempoolLoader,
	fetchMempool,
	filterPending,
	MempoolError,
	validateSnapshot,
	type MempoolSnapshot,
	type MempoolState
} from '$lib/mempool/observations';

function fixture(): MempoolSnapshot {
	return {
		scope: 'configured_node_mempool',
		source: 'configured_primary_node',
		checked_at_ms: 1_800_000_000_000,
		expires_at_ms: 1_800_000_005_000,
		cached: false,
		limit: 100,
		limit_reached: false,
		observed_count: 2,
		items: [
			{
				id: 'a'.repeat(64),
				input_count: 2,
				output_count: 3,
				data_input_count: 1,
				fee: '9007199254740993',
				size: 1234
			},
			{
				id: 'b'.repeat(64),
				input_count: 1,
				output_count: 1,
				data_input_count: 0,
				fee: null,
				size: null
			}
		]
	};
}
afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

describe('bounded node mempool observations', () => {
	it('preserves exact fees and nullable evidence, with no invented timestamp per transaction', () => {
		const data = validateSnapshot(fixture());
		expect(data.items[0].fee).toBe('9007199254740993');
		expect(data.items[1]).toMatchObject({ fee: null, size: null });
		const max = fixture();
		max.items[0].fee = ((1n << 128n) - 1n).toString();
		expect(validateSnapshot(max).items[0].fee).toHaveLength(39);
	});
	it('allows only a successful empty observation and an explicit capacity marker', () => {
		expect(validateSnapshot({ ...fixture(), items: [], observed_count: 0 }).items).toEqual([]);
		const data = fixture();
		data.items = Array.from({ length: 100 }, (_, index) => ({
			...data.items[0],
			id: index.toString(16).padStart(64, '0')
		}));
		data.observed_count = 100;
		data.limit_reached = true;
		expect(validateSnapshot(data).limit_reached).toBe(true);
	});
	it.each([
		(data: MempoolSnapshot) => {
			data.scope = 'global' as MempoolSnapshot['scope'];
		},
		(data: MempoolSnapshot) => {
			data.source = 'other' as MempoolSnapshot['source'];
		},
		(data: MempoolSnapshot) => {
			data.cached = 'yes' as unknown as boolean;
		},
		(data: MempoolSnapshot) => {
			data.limit_reached = true;
		},
		(data: MempoolSnapshot) => {
			data.observed_count = 3;
		},
		(data: MempoolSnapshot) => {
			data.limit = 101 as 100;
		},
		(data: MempoolSnapshot) => {
			data.expires_at_ms = data.checked_at_ms - 1;
		},
		(data: MempoolSnapshot) => {
			data.expires_at_ms = data.checked_at_ms + 60_001;
		},
		(data: MempoolSnapshot) => {
			data.checked_at_ms = data.expires_at_ms = Number.MAX_SAFE_INTEGER;
		},
		(data: MempoolSnapshot) => {
			data.items[1].id = data.items[0].id;
		},
		(data: MempoolSnapshot) => {
			data.items[0].id = 'invalid';
		},
		(data: MempoolSnapshot) => {
			data.items[0].input_count = 513;
		},
		(data: MempoolSnapshot) => {
			data.items[0].output_count = 0;
		},
		(data: MempoolSnapshot) => {
			data.items[0].output_count = 4097;
		},
		(data: MempoolSnapshot) => {
			data.items[0].data_input_count = -1;
		},
		(data: MempoolSnapshot) => {
			data.items[0].size = 0x1_0000_0000;
		},
		(data: MempoolSnapshot) => {
			data.items[0].size = 0;
		},
		(data: MempoolSnapshot) => {
			data.items[0].fee = 17 as unknown as string;
		},
		(data: MempoolSnapshot) => {
			data.items[0].fee = '-1';
		},
		(data: MempoolSnapshot) => {
			data.items[0].fee = '01';
		},
		(data: MempoolSnapshot) => {
			data.items[0].fee = (1n << 128n).toString();
		},
		(data: MempoolSnapshot) => {
			data.items = Array(101).fill(data.items[0]);
			data.observed_count = 101;
		}
	])('rejects malformed, incomplete or excessive evidence', (change) => {
		const data = fixture();
		change(data);
		expect(() => validateSnapshot(data)).toThrow(MempoolError);
	});
	it('filters only loaded IDs without changing the observation or count', () => {
		const data = fixture();
		expect(filterPending(data.items, ' AAA ')).toEqual([data.items[0]]);
		expect(filterPending(data.items, 'c')).toEqual([]);
		expect(data.observed_count).toBe(2);
	});
});

describe('one-shot refresh lifecycle', () => {
	it('does not poll, clears an old success before refresh, and keeps errors distinct from an empty list', async () => {
		vi.useFakeTimers();
		const states: MempoolState[] = [];
		const fetcher = vi.fn(async () => fixture());
		const loader = createMempoolLoader((state) => states.push(state), fetcher);
		expect(fetcher).not.toHaveBeenCalled();
		await loader.refresh();
		expect(states.at(-1)?.snapshot?.observed_count).toBe(2);
		await vi.advanceTimersByTimeAsync(60_000);
		expect(fetcher).toHaveBeenCalledTimes(1);
		fetcher.mockRejectedValueOnce(new MempoolError('mempool_unavailable', 'Node unavailable'));
		await loader.refresh();
		expect(states.at(-2)).toEqual({ loading: true, snapshot: null, error: null });
		expect(states.at(-1)).toEqual({
			loading: false,
			snapshot: null,
			error: { code: 'mempool_unavailable', message: 'Node unavailable' }
		});
		expect(fetcher).toHaveBeenCalledTimes(2);
	});
	it('deduplicates active refreshes and discards late results after leaving', async () => {
		let resolve!: (value: MempoolSnapshot) => void;
		let signal!: AbortSignal;
		const states: MempoolState[] = [];
		const fetcher = vi.fn((next: AbortSignal) => {
			signal = next;
			return new Promise<MempoolSnapshot>((done) => {
				resolve = done;
			});
		});
		const loader = createMempoolLoader((state) => states.push(state), fetcher);
		const pending = loader.refresh();
		await loader.refresh();
		expect(fetcher).toHaveBeenCalledTimes(1);
		loader.stop();
		expect(signal.aborted).toBe(true);
		resolve(fixture());
		await pending;
		await loader.refresh();
		expect(states).toEqual([{ loading: true, snapshot: null, error: null }]);
	});
	it('times out without keeping a loaded observation', async () => {
		vi.useFakeTimers();
		const states: MempoolState[] = [];
		const loader = createMempoolLoader(
			(state) => states.push(state),
			(signal) =>
				new Promise((_resolve, reject) => {
					signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')));
				})
		);
		const pending = loader.refresh();
		await vi.advanceTimersByTimeAsync(10_000);
		await pending;
		expect(states.at(-1)).toMatchObject({
			loading: false,
			snapshot: null,
			error: { code: 'mempool_unavailable', message: expect.stringContaining('timed out') }
		});
	});
});

describe('same-origin bounded HTTP observations', () => {
	it('retains an unavailable/not-configured response as a typed problem', async () => {
		const fetch = vi.fn(
			async () =>
				new Response(
					JSON.stringify({ code: 'mempool_not_configured', detail: 'No node configured.' }),
					{ status: 503 }
				)
		);
		vi.stubGlobal('fetch', fetch);
		await expect(fetchMempool(new AbortController().signal)).rejects.toMatchObject({
			code: 'mempool_not_configured'
		});
		expect(fetch).toHaveBeenCalledWith(
			'/v1/mempool',
			expect.objectContaining({ credentials: 'omit', redirect: 'error' })
		);
	});
	it('rejects oversized, invalid-UTF8 and malformed JSON responses before publishing facts', async () => {
		for (const body of ['x'.repeat(256 * 1024 + 1), new Uint8Array([0xff]), '{}broken']) {
			vi.stubGlobal(
				'fetch',
				vi.fn(async () => new Response(body))
			);
			await expect(fetchMempool(new AbortController().signal)).rejects.toMatchObject({
				code: 'mempool_invalid_response'
			});
		}
	});
});
