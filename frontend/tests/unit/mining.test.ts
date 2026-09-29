import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	createMiningLoader,
	fetchOverview,
	miningQuery,
	range,
	shareWidth,
	validateOverview,
	type MiningOverview,
	type MiningState
} from '$lib/mining/overview';
import example from '../fixtures/mining-overview.json';

const requested = range('1', '4', '2');
const fixture = () => structuredClone(example) as MiningOverview;
afterEach(() => {
	vi.unstubAllGlobals();
	vi.useRealTimers();
});

describe('complete mining window validation', () => {
	it('accepts a maximum range and creates an exact replayable pin', () => {
		expect(range('1', '20160', '50')).toEqual({ from_height: 1, to_height: 20160, top: 50 });
		expect(miningQuery(range('1', '4', '2', 'a'.repeat(64)))).toBe(
			'from_height=1&to_height=4&top=2&end_block_id=' + 'a'.repeat(64)
		);
	});
	it.each([
		['0', '4', '2'],
		['01', '4', '2'],
		['5', '4', '2'],
		['1', '20161', '2'],
		['1', '4', '0'],
		['1', '4', '51'],
		['1', '4', '1.5'],
		['1', '4294967296', '2']
	])('rejects invalid selection %s/%s/%s', (from, to, top) => {
		expect(() => range(from, to, top)).toThrow();
	});
	it('keeps >JS-safe fee sums and distinct unknown/zero votes', () => {
		const data = validateOverview(fixture(), requested);
		expect(data.totals.fees).toBe('9007199254740996');
		expect(data.miner_keys.items[0].fees).toBe('9007199254740993');
		expect(data.votes.unknown_blocks).toBe(1);
		expect(data.votes.zero_vote_blocks).toBe(1);
		expect(data.miner_keys.other_block_count).toBe(1);
		expect(shareWidth(1, 3)).toBe(33.3);
	});
	it('supports an entirely unknown vote field without inventing a tuple', () => {
		const data = fixture();
		data.votes = {
			known_blocks: 0,
			unknown_blocks: 4,
			zero_vote_blocks: 0,
			distinct_tuples: 0,
			items: [],
			other_tuple_count: 0,
			other_block_count: 0
		};
		expect(validateOverview(data, requested).votes.items).toEqual([]);
	});
	it('retains the exact vote remainder when zero tuples fall outside the top list', () => {
		const data = fixture();
		data.top = 1;
		data.miner_keys.items.pop();
		data.miner_keys.other_key_count = 2;
		data.miner_keys.other_block_count = 2;
		data.miner_keys.other_fees = '3';
		data.votes.items.pop();
		data.votes.other_tuple_count = 1;
		data.votes.other_block_count = 1;
		expect(validateOverview(data, { ...requested, top: 1 }).votes.zero_vote_blocks).toBe(1);
	});
	it.each([
		[
			'incomplete',
			(d: MiningOverview) => {
				d.complete = false as true;
			}
		],
		[
			'range mismatch',
			(d: MiningOverview) => {
				d.from_height = 2;
			}
		],
		[
			'invalid anchor',
			(d: MiningOverview) => {
				d.anchor.block_id = 'bad';
			}
		],
		[
			'coerced anchor string',
			(d: MiningOverview) => {
				d.anchor.block_id = ['a'.repeat(64)] as unknown as string;
			}
		],
		[
			'coerced vote string',
			(d: MiningOverview) => {
				d.votes.items[0].votes = ['01ff00'] as unknown as string;
			}
		],
		[
			'single occurrence with different heights',
			(d: MiningOverview) => {
				d.miner_keys.items[1].last_height = 3;
				d.miner_keys.items[1].last_block_id = 'd'.repeat(64);
			}
		],
		[
			'tip behind selection',
			(d: MiningOverview) => {
				d.indexed_height = 3;
			}
		],
		[
			'unretained range',
			(d: MiningOverview) => {
				d.partial_from = 2;
			}
		],
		[
			'contradictory full index',
			(d: MiningOverview) => {
				d.full_history = true;
			}
		],
		[
			'fee rounding',
			(d: MiningOverview) => {
				d.totals.fees = '9007199254740995';
			}
		],
		[
			'noncanonical integer',
			(d: MiningOverview) => {
				d.totals.fees = '09007199254740996';
			}
		],
		[
			'overflow',
			(d: MiningOverview) => {
				d.totals.fees = (1n << 128n).toString();
			}
		],
		[
			'missing key',
			(d: MiningOverview) => {
				d.miner_keys.items.pop();
			}
		],
		[
			'duplicate key',
			(d: MiningOverview) => {
				d.miner_keys.items[1].public_key = d.miner_keys.items[0].public_key;
			}
		],
		[
			'bad key',
			(d: MiningOverview) => {
				d.miner_keys.items[0].public_key = 'unknown';
			}
		],
		[
			'missing remainder',
			(d: MiningOverview) => {
				d.miner_keys.other_block_count = 0;
			}
		],
		[
			'wrong distinct count',
			(d: MiningOverview) => {
				d.miner_keys.distinct_count = 2;
			}
		],
		[
			'range evidence mismatch',
			(d: MiningOverview) => {
				d.miner_keys.items[0].last_height = 5;
			}
		],
		[
			'end evidence mismatch',
			(d: MiningOverview) => {
				d.miner_keys.items[0].last_block_id = 'c'.repeat(64);
			}
		],
		[
			'same-height evidence mismatch',
			(d: MiningOverview) => {
				d.miner_keys.items[1].last_block_id = 'b'.repeat(64);
			}
		],
		[
			'unsorted ranking',
			(d: MiningOverview) => {
				d.miner_keys.items.reverse();
			}
		],
		[
			'version missing blocks',
			(d: MiningOverview) => {
				d.versions[0].block_count = 2;
			}
		],
		[
			'duplicate version',
			(d: MiningOverview) => {
				d.versions[1].version = 3;
			}
		],
		[
			'out of range version',
			(d: MiningOverview) => {
				d.versions[1].version = 256;
			}
		],
		[
			'unknown interpreted as zero',
			(d: MiningOverview) => {
				d.votes.zero_vote_blocks = 2;
			}
		],
		[
			'unaccounted vote coverage',
			(d: MiningOverview) => {
				d.votes.known_blocks = 4;
			}
		],
		[
			'duplicate tuple',
			(d: MiningOverview) => {
				d.votes.items[1].votes = '01ff00';
			}
		],
		[
			'invalid vote bytes',
			(d: MiningOverview) => {
				d.votes.items[0].votes = '00';
			}
		],
		[
			'wrong vote sum',
			(d: MiningOverview) => {
				d.votes.items[0].block_count = 3;
			}
		]
	])('rejects %s', (_name, mutate) => {
		const data = fixture();
		mutate(data);
		expect(() => validateOverview(data, requested)).toThrow();
	});
	it('rejects a different end-block pin even if every sum is correct', () => {
		expect(() =>
			validateOverview(fixture(), { ...requested, end_block_id: 'b'.repeat(64) })
		).toThrow();
	});
});
describe('explicit snapshot loader', () => {
	it('does not load implicitly and clears prior success before a failed refresh', async () => {
		const states: MiningState[] = [];
		const fetcher = vi
			.fn()
			.mockResolvedValueOnce(fixture())
			.mockRejectedValueOnce(new Error('Pinned block changed'));
		const loader = createMiningLoader((state) => states.push(state), fetcher);
		expect(fetcher).not.toHaveBeenCalled();
		await loader.load(requested);
		expect(states.at(-1)?.data?.block_count).toBe(4);
		await loader.load(requested);
		expect(states.at(-2)).toEqual({ loading: true, data: null, error: null });
		expect(states.at(-1)).toEqual({ loading: false, data: null, error: 'Pinned block changed' });
		loader.stop();
	});
	it('editing the range aborts and drops late results', async () => {
		let resolve!: (value: unknown) => void;
		let signal: AbortSignal | undefined;
		const states: MiningState[] = [];
		const loader = createMiningLoader(
			(state) => states.push(state),
			(_request, active) => {
				signal = active;
				return new Promise((done) => {
					resolve = done;
				});
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
	it('old errors cannot replace newer success or publish after unmount', async () => {
		let reject!: (value: Error) => void;
		const states: MiningState[] = [];
		const fetcher = vi
			.fn()
			.mockImplementationOnce(
				() =>
					new Promise((_done, fail) => {
						reject = fail;
					})
			)
			.mockResolvedValueOnce(fixture());
		const loader = createMiningLoader((state) => states.push(state), fetcher);
		const old = loader.load(requested);
		await loader.load(requested);
		const count = states.length;
		loader.stop();
		reject(new Error('late error'));
		await old;
		expect(states.length).toBe(count);
	});
	it('aborts stalled fetches at ten seconds and leaves retry available', async () => {
		vi.useFakeTimers();
		const states: MiningState[] = [];
		const loader = createMiningLoader(
			(state) => states.push(state),
			(_request, signal) =>
				new Promise((_done, reject) =>
					signal.addEventListener('abort', () => reject(new Error('abort')))
				)
		);
		const pending = loader.load(requested);
		await vi.advanceTimersByTimeAsync(10_001);
		await pending;
		expect(states.at(-1)?.loading).toBe(false);
		expect(states.at(-1)?.error).toContain('timed out');
		loader.stop();
	});
	it('only requests same-origin data with the explicit pin', async () => {
		const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(fixture())));
		vi.stubGlobal('fetch', fetcher);
		await fetchOverview(
			{ ...requested, end_block_id: 'a'.repeat(64) },
			new AbortController().signal
		);
		expect(fetcher.mock.calls[0][0]).toBe(
			'/v1/mining?from_height=1&to_height=4&top=2&end_block_id=' + 'a'.repeat(64)
		);
		expect(fetcher.mock.calls[0][1]).toMatchObject({ credentials: 'omit', redirect: 'error' });
	});
	it('rejects a streamed oversized response before JSON decoding', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(new Uint8Array(128 * 1024 + 1))));
		await expect(fetchOverview(requested, new AbortController().signal)).rejects.toThrow(
			'response limit'
		);
	});
});
