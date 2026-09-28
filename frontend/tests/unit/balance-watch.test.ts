import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GroupBalances } from '../../src/lib/addresses/groups';
import {
	compareBalances,
	createBalanceWatch,
	MAX_WATCH_EVENTS,
	WATCH_INTERVAL_MS
} from '../../src/lib/addresses/watch';

function snapshot(height = 10, nano = '9007199254740993123'): GroupBalances {
	return {
		scope: 'selected_address_scripts',
		consistency: 'single_reader',
		indexed_height: height,
		anchor: { height, block_id: height.toString(16).padStart(64, '0') },
		full_history: true,
		partial_from: null,
		complete: true,
		requested_count: 1,
		resolved_script_count: 1,
		members: [
			{
				address: 'address',
				tree_hash: 'a'.repeat(64),
				status: 'resolved',
				duplicate_of: null,
				balance: { nano, tokens: [] }
			}
		],
		observed_totals: { nano, tokens: [{ id: 'b'.repeat(64), amount: '18446744073709551615' }] }
	};
}
function setup() {
	let visible = true;
	const read = vi.fn(async () => snapshot());
	const onChange = vi.fn();
	const watch = createBalanceWatch({ read, onChange, visible: () => visible });
	watch.setSelection(['address']);
	return {
		read,
		onChange,
		watch,
		hide: () => {
			visible = false;
			watch.visibilityChanged();
		},
		show: () => {
			visible = true;
			watch.visibilityChanged();
		}
	};
}
const settle = async () => {
	await vi.advanceTimersByTimeAsync(0);
};
beforeEach(() => {
	vi.useFakeTimers();
	vi.setSystemTime(1_000_000);
});
afterEach(() => {
	vi.useRealTimers();
});

describe('exact comparable watch snapshots', () => {
	it('subtracts raw integers beyond Number precision and includes added and removed tokens', () => {
		const before = snapshot();
		const after = snapshot(11, '9007199254740993124');
		after.observed_totals!.tokens = [{ id: 'c'.repeat(64), amount: '18446744073709551615' }];
		expect(compareBalances(before, after)).toEqual({
			nano: '1',
			tokens: [
				{ id: 'b'.repeat(64), delta: '-18446744073709551615' },
				{ id: 'c'.repeat(64), delta: '18446744073709551615' }
			]
		});
	});
	it.each(['partial', 'unseen', 'unanchored', 'scripts'])(
		'withholds comparison for %s evidence',
		(kind) => {
			const after = snapshot(11);
			if (kind === 'partial') {
				after.complete = false;
				after.full_history = false;
			}
			if (kind === 'unseen') {
				after.members[0].status = 'unseen';
				after.observed_totals = null;
			}
			if (kind === 'unanchored') after.anchor = null;
			if (kind === 'scripts') after.members[0].tree_hash = 'c'.repeat(64);
			expect(compareBalances(snapshot(), after)).toBeNull();
		}
	);
});

describe('page-open explicit watch lifecycle', () => {
	it('does not run before opt-in and enforces at least sixty seconds between starts', async () => {
		const { watch, read } = setup();
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 3);
		expect(read).not.toHaveBeenCalled();
		watch.start();
		await settle();
		expect(read).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS - 1);
		expect(read).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(1);
		expect(read).toHaveBeenCalledTimes(2);
		watch.stop();
		watch.start();
		await settle();
		expect(read).toHaveBeenCalledTimes(2);
	});
	it('pauses hidden checks and resumes only when due', async () => {
		const { watch, read, hide, show } = setup();
		watch.start();
		await settle();
		hide();
		await vi.advanceTimersByTimeAsync(30_000);
		show();
		await settle();
		expect(read).toHaveBeenCalledTimes(1);
		hide();
		await vi.advanceTimersByTimeAsync(90_000);
		expect(read).toHaveBeenCalledTimes(1);
		expect(watch.state.phase).toBe('paused');
		show();
		await settle();
		expect(read).toHaveBeenCalledTimes(2);
	});
	it('does not overlap a slow request or start on hidden opt-in', async () => {
		let finish!: (value: GroupBalances) => void;
		const { watch, read, hide, show } = setup();
		read.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
		hide();
		watch.start();
		await settle();
		expect(read).not.toHaveBeenCalled();
		show();
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 2);
		expect(read).toHaveBeenCalledTimes(1);
		finish(snapshot());
		await settle();
		expect(read).toHaveBeenCalledTimes(2);
	});
	it('records an exact observed change, leaving unchanged checks out of the bounded timeline', async () => {
		const { watch, read } = setup();
		read.mockResolvedValueOnce(snapshot()).mockResolvedValue(snapshot(11, '9007199254740993124'));
		watch.start();
		await settle();
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS);
		expect(watch.state.events[0].change?.nano).toBe('1');
		expect(watch.state.events[0].from?.height).toBe(10);
		expect(watch.state.events[0].to?.height).toBe(11);
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS);
		expect(watch.state.events).toHaveLength(2);
		expect(watch.state.message).toContain('No observed balance change');
	});
	it.each(['lower', 'replacement', 'same-anchor-values'])(
		'resets rather than claims a change after %s evidence',
		async (kind) => {
			const { watch, read } = setup();
			const after = snapshot(kind === 'lower' ? 9 : 10, '9007199254740993124');
			if (kind === 'replacement') after.anchor!.block_id = 'd'.repeat(64);
			read.mockResolvedValueOnce(snapshot()).mockResolvedValueOnce(after);
			watch.start();
			await settle();
			await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS);
			expect(watch.state.events[0].kind).toBe('reset');
			expect(watch.state.events[0].change).toBeNull();
		}
	);
	it.each(['partial', 'failure'])('clears baseline across %s gaps', async (kind) => {
		const { watch, read } = setup();
		read.mockResolvedValueOnce(snapshot());
		if (kind === 'partial')
			read.mockResolvedValueOnce({ ...snapshot(11), complete: false, full_history: false });
		else read.mockRejectedValueOnce(new Error('Busy'));
		read.mockResolvedValue(snapshot(12, '9007199254740993999'));
		watch.start();
		await settle();
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 2);
		expect(watch.state.events[0].kind).toBe('baseline');
		expect(watch.state.events.some((event) => event.change)).toBe(false);
	});
	it('stops and clears on membership change, ignores old responses and never resumes automatically', async () => {
		let finish!: (value: GroupBalances) => void;
		const { watch, read } = setup();
		read.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
		watch.start();
		watch.setSelection(['other']);
		finish(snapshot());
		await settle();
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 2);
		expect(watch.state.active).toBe(false);
		expect(watch.state.events).toHaveLength(0);
		expect(read).toHaveBeenCalledTimes(1);
	});
	it('caps observations and leaves no timer or late publication after navigation', async () => {
		const { watch, read, onChange } = setup();
		let height = 10;
		read.mockImplementation(async () => snapshot(height++, String(height)));
		watch.start();
		await settle();
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * (MAX_WATCH_EVENTS + 3));
		expect(watch.state.events).toHaveLength(MAX_WATCH_EVENTS);
		watch.destroy();
		const count = onChange.mock.calls.length;
		await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 3);
		expect(onChange).toHaveBeenCalledTimes(count);
		expect(vi.getTimerCount()).toBe(0);
	});
});
