import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../src/lib/api/client';
import {
	createHistoryInspector,
	historySelector,
	sumHistoryBoxes,
	type HistoryBalance,
	type HistoryBoxes,
	type HistoryBox
} from '../../src/lib/addresses/history';

const anchor = { height: 12, block_id: 'a'.repeat(64) };
const token = 'b'.repeat(64);
const box: HistoryBox = {
	box_id: 'c'.repeat(64),
	inclusion_height: 8,
	nano: '9007199254740993123',
	tokens: [{ token_id: token, amount: '900719925474099312345' }]
};
const balance: HistoryBalance = {
	address: 'address',
	at: anchor,
	indexed_height: 20,
	balance: sumHistoryBoxes([box]),
	complete: true
};
const boxes: HistoryBoxes = { at: anchor, items: [box], next_cursor: null };
function setup() {
	const api = { balance: vi.fn(async () => balance), boxes: vi.fn(async () => boxes) };
	const inspect = createHistoryInspector(() => {}, api);
	return { api, inspect };
}

describe('historical selectors and integer totals', () => {
	it('accepts genesis and canonicalizes a pinned block ID', () => {
		expect(historySelector(null)).toEqual({ value: null, error: null });
		expect(historySelector('0').value).toEqual({ height: 0 });
		expect(historySelector('00012', 'AB'.repeat(32)).value).toEqual({
			height: 12,
			block_id: 'ab'.repeat(32)
		});
		expect(historySelector('4294967295').value?.height).toBe(4294967295);
	});
	it.each(['', '-1', '1.5', '1e3', '4294967296', ' 1', 'x'])('rejects invalid height %s', (raw) =>
		expect(historySelector(raw).error).toBeTruthy()
	);
	it('rejects a block ID without height, invalid pins, and a genesis pin', () => {
		expect(historySelector(null, anchor.block_id).error).toBeTruthy();
		expect(historySelector('12', 'bad').error).toBeTruthy();
		expect(historySelector('0', anchor.block_id).error).toBeTruthy();
	});
	it('sums raw quantities beyond Number precision without metadata or zero loss', () => {
		const second = {
			...box,
			box_id: 'd'.repeat(64),
			nano: '1',
			tokens: [{ token_id: token, amount: '2' }]
		};
		expect(sumHistoryBoxes([box, second])).toEqual({
			nano: '9007199254740993124',
			box_count: 2,
			tokens: [{ token_id: token, amount: '900719925474099312347' }]
		});
		expect(sumHistoryBoxes([])).toEqual({ nano: '0', box_count: 0, tokens: [] });
	});
});

describe('explicit anchored historical inspection', () => {
	it('makes no automatic request and pins the box page to the balance response', async () => {
		const { api, inspect } = setup();
		expect(api.balance).not.toHaveBeenCalled();
		expect(api.boxes).not.toHaveBeenCalled();
		await inspect.start('address', { height: 12 });
		expect(api.balance).toHaveBeenCalledWith('address', { height: 12 });
		expect(api.boxes).toHaveBeenCalledWith('address', anchor, undefined, 20);
		expect(inspect.state.balance).toEqual(balance);
		expect(inspect.state.boxes).toEqual([box]);
	});
	it('continues empty pages and carries the same anchor even after the indexed tip appends', async () => {
		const { api, inspect } = setup();
		api.balance.mockResolvedValue({ ...balance, indexed_height: 100 });
		api.boxes
			.mockResolvedValueOnce({ at: anchor, items: [], next_cursor: 'next' })
			.mockResolvedValueOnce(boxes);
		await inspect.start('address', { height: 12 });
		expect(inspect.state.boxesLoaded).toBe(true);
		expect(inspect.state.nextCursor).toBe('next');
		await inspect.more();
		expect(api.boxes).toHaveBeenLastCalledWith('address', anchor, 'next', 20);
		expect(inspect.state.boxes).toEqual([box]);
		expect(inspect.state.nextCursor).toBeNull();
		await inspect.more();
		expect(api.boxes).toHaveBeenCalledTimes(2);
	});
	it.each(['history_scan_limit', 'history_response_limit'])(
		'falls back to anchored pages for %s without claiming a scalar total',
		async (code) => {
			const { api, inspect } = setup();
			api.balance.mockRejectedValue(new ApiError(422, 'Limit', 'Limit', code));
			api.boxes.mockResolvedValue({ ...boxes, next_cursor: 'next' });
			await inspect.start('address', { height: 12 });
			expect(inspect.state.balance).toBeNull();
			expect(inspect.state.balanceIssue?.code).toBe(code);
			expect(inspect.state.anchor).toEqual(anchor);
			expect(inspect.state.nextCursor).toBe('next');
		}
	);
	it.each([
		[503, 'history_unavailable'],
		[400, 'height_not_indexed'],
		[404, ''],
		[503, 'history_busy']
	])('keeps %s %s unavailable instead of showing zero', async (status, code) => {
		const { api, inspect } = setup();
		api.balance.mockRejectedValue(
			new ApiError(Number(status), 'Unavailable', 'Unavailable', String(code))
		);
		await inspect.start('address', { height: 12 });
		expect(api.boxes).not.toHaveBeenCalled();
		expect(inspect.state.anchor).toBeNull();
		expect(inspect.state.balance).toBeNull();
		expect(inspect.state.issue).toBeTruthy();
	});
	it('discards every prior total on a changed continuation anchor and requires restart', async () => {
		const { api, inspect } = setup();
		api.boxes
			.mockResolvedValueOnce({ ...boxes, next_cursor: 'next' })
			.mockRejectedValueOnce(new ApiError(409, 'Changed', 'Changed', 'snapshot_changed'));
		await inspect.start('address', { height: 12 });
		await inspect.more();
		expect(inspect.state.restartRequired).toBe(true);
		expect(inspect.state.boxes).toEqual([]);
		expect(inspect.state.balance).toBeNull();
		expect(inspect.state.anchor).toBeNull();
		await inspect.more();
		expect(api.boxes).toHaveBeenCalledTimes(2);
	});
	it('rejects mismatched successful anchors before combining results', async () => {
		const { api, inspect } = setup();
		api.boxes.mockResolvedValue({ ...boxes, at: { height: 12, block_id: 'f'.repeat(64) } });
		await inspect.start('address', { height: 12 });
		expect(inspect.state.restartRequired).toBe(true);
		expect(inspect.state.balance).toBeNull();
		expect(inspect.state.boxes).toEqual([]);
	});
	it('preserves valid anchored results on a transient page failure and retries the same cursor', async () => {
		const { api, inspect } = setup();
		api.boxes
			.mockResolvedValueOnce({ ...boxes, next_cursor: 'next' })
			.mockRejectedValueOnce(new Error('offline'))
			.mockResolvedValueOnce({ ...boxes, items: [] });
		await inspect.start('address', { height: 12 });
		await inspect.more();
		expect(inspect.state.balance).toEqual(balance);
		expect(inspect.state.nextCursor).toBe('next');
		await inspect.more();
		expect(api.boxes).toHaveBeenLastCalledWith('address', anchor, 'next', 20);
		expect(inspect.state.issue).toBeNull();
	});
	it('invalidates old responses when the address or selector changes', async () => {
		const { api, inspect } = setup();
		let resolve!: (value: HistoryBalance) => void;
		api.balance.mockImplementationOnce(() => new Promise((done) => (resolve = done)));
		const old = inspect.start('old', { height: 12 });
		inspect.reset();
		resolve(balance);
		await old;
		expect(inspect.state.started).toBe(false);
		expect(api.boxes).not.toHaveBeenCalled();
		inspect.stop();
		await inspect.start('address', { height: 12 });
		expect(api.balance).toHaveBeenCalledTimes(1);
	});
	it('does not overlap page requests', async () => {
		const { api, inspect } = setup();
		let resolve!: (value: HistoryBoxes) => void;
		api.boxes.mockImplementationOnce(() => new Promise((done) => (resolve = done)));
		const request = inspect.start('address', { height: 12 });
		await Promise.resolve();
		await inspect.more();
		expect(api.boxes).toHaveBeenCalledTimes(1);
		resolve(boxes);
		await request;
	});
	it('rejects a page exceeding the bounded token view without silently accepting partial boxes', async () => {
		const { api, inspect } = setup();
		api.boxes.mockResolvedValue({
			...boxes,
			items: [{ ...box, tokens: Array(10_001).fill(box.tokens[0]) }]
		});
		await inspect.start('address', { height: 12 });
		expect(inspect.state.viewLimit).toBe(true);
		expect(inspect.state.boxes).toEqual([]);
		expect(inspect.state.balance).toEqual(balance);
	});
	it('allows height zero with a null anchor and no block_id query', async () => {
		const { api, inspect } = setup();
		const genesis = { height: 0, block_id: null };
		api.balance.mockResolvedValue({ ...balance, at: genesis });
		api.boxes.mockResolvedValue({ ...boxes, at: genesis });
		await inspect.start('address', { height: 0 });
		expect(api.boxes).toHaveBeenCalledWith('address', { height: 0 }, undefined, 20);
		expect(inspect.state.anchor).toEqual(genesis);
	});
});
