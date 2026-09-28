import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../src/lib/api/client';
import {
	createGroupBalances,
	fetchGroupBalances,
	groupRequest,
	validateGroupBalances,
	type GroupBalances
} from '../../src/lib/addresses/groups';

const address = '9'.repeat(51);
const alias = '8'.repeat(51);
const token = 'b'.repeat(64);
const nano = '9007199254740993123';
function snapshot(): GroupBalances {
	const balance = { nano, tokens: [{ id: token, amount: '18446744073709551615' }] };
	return {
		scope: 'selected_address_scripts',
		consistency: 'single_reader',
		indexed_height: 12,
		anchor: { height: 12, block_id: 'c'.repeat(64) },
		full_history: true,
		partial_from: null,
		complete: true,
		requested_count: 2,
		resolved_script_count: 1,
		members: [
			{ address, tree_hash: 'a'.repeat(64), status: 'resolved', duplicate_of: null, balance },
			{
				address: alias,
				tree_hash: 'a'.repeat(64),
				status: 'duplicate',
				duplicate_of: 0,
				balance: null
			}
		],
		observed_totals: balance
	};
}
function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => (resolve = done));
	return { promise, resolve };
}

describe('selected address batch contract', () => {
	it('sends only selected address strings in an explicit POST', async () => {
		const fetcher = vi.fn(async () => new Response(JSON.stringify(snapshot())));
		await fetchGroupBalances([address, alias], undefined, fetcher);
		const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit];
		expect(url).toBe('/v1/addresses/balances');
		expect(init.method).toBe('POST');
		expect(init.headers).toEqual({ 'Content-Type': 'application/json' });
		expect(JSON.parse(init.body as string)).toEqual({ addresses: [address, alias] });
	});
	it('enforces count and UTF-8 byte limits before sending', () => {
		expect(() => groupRequest([])).toThrow('between 1 and 100');
		expect(() => groupRequest(Array(101).fill(address))).toThrow('between 1 and 100');
		expect(() => groupRequest(['é'.repeat(2049)])).toThrow('supported size');
		expect(() => groupRequest(Array(32).fill('a'.repeat(4096)))).toThrow('128,000-byte');
		expect(groupRequest(['invalid but bounded'])).toEqual({ addresses: ['invalid but bounded'] });
	});
	it('keeps exact u64 amounts and counts aliases once', () => {
		const value = validateGroupBalances(snapshot(), [address, alias]);
		expect(value.observed_totals?.nano).toBe(nano);
		expect(value.observed_totals?.tokens[0].amount).toBe('18446744073709551615');
		expect(value.resolved_script_count).toBe(1);
	});
	it('accepts sums beyond u64 without rounding', () => {
		const data = snapshot();
		data.members[1] = { ...data.members[0], address: alias, tree_hash: 'd'.repeat(64) };
		data.resolved_script_count = 2;
		data.observed_totals = {
			nano: '18014398509481986246',
			tokens: [{ id: token, amount: '36893488147419103230' }]
		};
		expect(validateGroupBalances(data, [address, alias])).toBe(data);
	});
	it('accepts partial observed totals and complete genesis without a block anchor', () => {
		const partial = { ...snapshot(), full_history: false, complete: false, partial_from: 10 };
		expect(validateGroupBalances(partial, [address, alias]).complete).toBe(false);
		const genesis = { ...snapshot(), indexed_height: null, anchor: null };
		expect(validateGroupBalances(genesis, [address, alias]).complete).toBe(true);
	});
	it('withholds totals for unseen scripts including their aliases', () => {
		const data = snapshot();
		data.members[0] = { ...data.members[0], status: 'unseen', balance: null };
		data.complete = false;
		data.resolved_script_count = 0;
		data.observed_totals = null;
		expect(validateGroupBalances(data, [address, alias]).observed_totals).toBeNull();
		data.observed_totals = { nano: '0', tokens: [] };
		expect(() => validateGroupBalances(data, [address, alias])).toThrow('inconsistent');
	});
	it('accepts invalid members only with explicitly unavailable totals', () => {
		const data = snapshot();
		data.members[1] = {
			address: alias,
			tree_hash: null,
			status: 'invalid',
			duplicate_of: null,
			balance: null
		};
		data.complete = false;
		data.observed_totals = null;
		expect(validateGroupBalances(data, [address, alias]).members[1].status).toBe('invalid');
	});
	it.each(['order', 'dedup', 'sum', 'anchor', 'integer', 'count'])(
		'rejects inconsistent %s evidence',
		(kind) => {
			const data = snapshot();
			if (kind === 'order') data.members.reverse();
			if (kind === 'dedup') data.members[1].duplicate_of = 1;
			if (kind === 'sum') data.observed_totals = { nano: '1', tokens: [] };
			if (kind === 'anchor') data.anchor!.height = 11;
			if (kind === 'integer') data.members[0].balance!.nano = '1e10';
			if (kind === 'count') data.resolved_script_count = 2;
			expect(() => validateGroupBalances(data, [address, alias])).toThrow('inconsistent');
		}
	);
	it('preserves stable API problem codes', async () => {
		const fetcher = vi.fn(
			async () =>
				new Response(JSON.stringify({ title: 'Busy', detail: 'Retry', code: 'group_deadline' }), {
					status: 422
				})
		);
		await expect(fetchGroupBalances([address], undefined, fetcher)).rejects.toMatchObject({
			status: 422,
			code: 'group_deadline'
		});
	});
	it('aborts a timed-out fetch and releases its timer', async () => {
		vi.useFakeTimers();
		try {
			const fetcher = vi.fn(
				(_url: RequestInfo | URL, init?: RequestInit) =>
					new Promise<Response>((_resolve, reject) => {
						init?.signal?.addEventListener('abort', () =>
							reject(new DOMException('Aborted', 'AbortError'))
						);
					})
			);
			const assertion = expect(
				fetchGroupBalances([address], undefined, fetcher)
			).rejects.toMatchObject({ name: 'AbortError' });
			await vi.advanceTimersByTimeAsync(10_000);
			await assertion;
			expect(vi.getTimerCount()).toBe(0);
		} finally {
			vi.useRealTimers();
		}
	});
});

describe('explicit group loading and stale response protection', () => {
	it('does not load on membership changes and prevents overlapping loads', async () => {
		const pending = deferred<GroupBalances>();
		const load = vi.fn(() => pending.promise);
		const controller = createGroupBalances(() => {}, load);
		controller.setAddresses([address, alias]);
		expect(load).not.toHaveBeenCalled();
		const first = controller.load();
		await controller.load();
		expect(load).toHaveBeenCalledTimes(1);
		pending.resolve(snapshot());
		await first;
		expect(controller.state.result?.complete).toBe(true);
	});
	it('retains a previous snapshot as stale on refresh failure and recovers explicitly', async () => {
		const load = vi
			.fn()
			.mockResolvedValueOnce(snapshot())
			.mockRejectedValueOnce(new ApiError(503, 'Busy', 'Retry'))
			.mockResolvedValueOnce(snapshot());
		const controller = createGroupBalances(() => {}, load);
		controller.setAddresses([address, alias]);
		await controller.load();
		await controller.load();
		expect(controller.state.stale).toBe(true);
		expect(controller.state.result?.observed_totals?.nano).toBe(nano);
		expect(controller.state.error).toContain('temporarily unavailable');
		await controller.load();
		expect(controller.state).toMatchObject({ stale: false, error: null, busy: false });
	});
	it('clears balances when membership changes and ignores an old in-flight response', async () => {
		const pending = deferred<GroupBalances>();
		const load = vi
			.fn()
			.mockResolvedValueOnce(snapshot())
			.mockImplementationOnce(() => pending.promise);
		const controller = createGroupBalances(() => {}, load);
		controller.setAddresses([address, alias]);
		await controller.load();
		const old = controller.load();
		const signal = load.mock.calls[1][1] as AbortSignal;
		controller.setAddresses([address]);
		expect(signal.aborted).toBe(true);
		expect(controller.state.result).toBeNull();
		pending.resolve(snapshot());
		await old;
		expect(controller.state).toEqual({ result: null, busy: false, stale: false, error: null });
		expect(load).toHaveBeenCalledTimes(2);
	});
	it('preserves results for the same addresses and stops publishing after navigation', async () => {
		const pending = deferred<GroupBalances>();
		const load = vi
			.fn()
			.mockResolvedValueOnce(snapshot())
			.mockImplementationOnce(() => pending.promise);
		const publish = vi.fn();
		const controller = createGroupBalances(publish, load);
		controller.setAddresses([address, alias]);
		await controller.load();
		controller.setAddresses([address, alias]);
		expect(controller.state.result).not.toBeNull();
		const old = controller.load();
		controller.stop();
		const calls = publish.mock.calls.length;
		pending.resolve(snapshot());
		await old;
		expect(publish).toHaveBeenCalledTimes(calls);
		await controller.load();
		expect(load).toHaveBeenCalledTimes(2);
	});
});
