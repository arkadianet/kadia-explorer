import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../src/lib/api/client';
import type { TxDto, TxStatusDto } from '../../src/lib/api/types';
import { createTransactionTracker, type TrackingState } from '../../src/lib/tx/tracker';

const id = 'a'.repeat(64);
const tx: TxDto = {
	id,
	height: 100,
	indexed_height: 100,
	block_id: 'b'.repeat(64),
	index: 0,
	timestamp: 0,
	size: 10,
	fee: '0',
	inputs: [],
	outputs: [],
	data_inputs: []
};
function observation(state: TxStatusDto['state'] = 'pending'): TxStatusDto {
	return {
		id,
		state,
		checked_at_ms: 1000,
		indexed_height: 102,
		inclusion:
			state === 'confirmed' ? { block_id: tx.block_id!, height: 100, confirmations: 3 } : null,
		previous_inclusion: null,
		mempool: {
			observation: state === 'pending' ? 'present' : 'absent',
			checked_at_ms: 1000,
			first_seen_at_ms: 900,
			last_seen_at_ms: 1000,
			error: null
		},
		pending: null,
		conflicts: [],
		history_scope: 'process_local_requested_transactions',
		retention_seconds: 3600
	};
}

describe('transaction tracking', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());
	function setup(initial: TxDto | null = null) {
		let value: TrackingState | undefined;
		const getStatus = vi.fn<() => Promise<TxStatusDto>>().mockResolvedValue(observation());
		const getTx = vi.fn<() => Promise<TxDto>>().mockResolvedValue(tx);
		const tracker = createTransactionTracker({
			id,
			tx: initial,
			status: null,
			getStatus,
			getTx,
			onChange: (next) => {
				value = next;
			}
		});
		return { tracker, getStatus, getTx, latest: () => value! };
	}
	it('follows pending to confirmed and refreshes confirmations without expanded boxes', async () => {
		const t = setup();
		await t.tracker.refresh();
		expect(t.latest().status?.state).toBe('pending');
		expect(t.latest().tx).toBeNull();
		t.getStatus.mockResolvedValue(observation('confirmed'));
		await t.tracker.refresh();
		expect(t.latest().status?.inclusion?.confirmations).toBe(3);
		const later = observation('confirmed');
		later.indexed_height = 103;
		later.inclusion!.confirmations = 4;
		t.getStatus.mockResolvedValue(later);
		await t.tracker.refresh();
		expect(t.latest().status?.inclusion?.confirmations).toBe(4);
		expect(t.latest().tx?.indexed_height).toBe(100);
		expect(t.getTx).toHaveBeenCalledTimes(1);
		t.tracker.stop();
	});
	it('manual refresh fetches details once at an unchanged inclusion', async () => {
		const t = setup(tx);
		t.getStatus.mockResolvedValue(observation('confirmed'));
		await t.tracker.refresh();
		expect(t.getTx).not.toHaveBeenCalled();
		t.getTx.mockResolvedValue({ ...tx, indexed_height: 102 });
		await t.tracker.refresh(true);
		expect(t.getTx).toHaveBeenCalledTimes(1);
		expect(t.latest().tx?.indexed_height).toBe(102);
		t.tracker.stop();
	});
	it('keeps polling inclusion without retrying rejected expansion at the same anchor', async () => {
		const t = setup();
		t.getStatus.mockResolvedValue(observation('confirmed'));
		t.getTx.mockRejectedValue(
			new ApiError(422, 'Unprocessable Entity', 'Expansion limit exceeded')
		);
		t.tracker.setVisible(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(t.getTx).toHaveBeenCalledTimes(1);
		expect(t.latest().tx).toBeNull();
		expect(t.latest().error).toContain('expansion limit');
		const later = observation('confirmed');
		later.indexed_height = 104;
		later.inclusion!.confirmations = 5;
		t.getStatus.mockResolvedValue(later);
		await vi.advanceTimersByTimeAsync(10000);
		expect(t.getStatus).toHaveBeenCalledTimes(3);
		expect(t.getTx).toHaveBeenCalledTimes(1);
		expect(t.latest().status?.inclusion?.confirmations).toBe(5);
		expect(t.latest().error).toContain('Check now');
		expect(t.latest().checking).toBe(false);
		t.tracker.stop();
	});
	it.each(['block', 'height'] as const)(
		'retries rejected details after the inclusion %s changes',
		async (change) => {
			const t = setup();
			t.getStatus.mockResolvedValue(observation('confirmed'));
			t.getTx.mockRejectedValueOnce(
				new ApiError(422, 'Unprocessable Entity', 'Expansion limit exceeded')
			);
			await t.tracker.refresh();
			const next = observation('confirmed');
			if (change === 'block') next.inclusion!.block_id = 'c'.repeat(64);
			else next.inclusion!.height = 101;
			t.getStatus.mockResolvedValue(next);
			t.getTx.mockResolvedValue({
				...tx,
				block_id: next.inclusion!.block_id,
				height: next.inclusion!.height
			});
			await t.tracker.refresh();
			expect(t.getTx).toHaveBeenCalledTimes(2);
			expect(t.latest().tx?.block_id).toBe(next.inclusion!.block_id);
			expect(t.latest().tx?.height).toBe(next.inclusion!.height);
			expect(t.latest().error).toBeNull();
			t.tracker.stop();
		}
	);
	it('forgets a rejection when inclusion disappears before the same anchor returns', async () => {
		const t = setup();
		t.getStatus.mockResolvedValue(observation('confirmed'));
		t.getTx.mockRejectedValueOnce(
			new ApiError(422, 'Unprocessable Entity', 'Expansion limit exceeded')
		);
		await t.tracker.refresh();
		t.getStatus.mockResolvedValue(observation('pending'));
		await t.tracker.refresh();
		expect(t.latest().tx).toBeNull();
		expect(t.latest().error).toBeNull();
		t.getStatus.mockResolvedValue(observation('confirmed'));
		await t.tracker.refresh();
		expect(t.getTx).toHaveBeenCalledTimes(2);
		expect(t.latest().tx).toEqual(tx);
		t.tracker.stop();
	});
	it('manual refresh bypasses a rejection and keeps the previous receipt marked stale until success', async () => {
		const t = setup(tx);
		t.getStatus.mockResolvedValue(observation('confirmed'));
		t.getTx.mockRejectedValueOnce(
			new ApiError(422, 'Unprocessable Entity', 'Expansion limit exceeded')
		);
		await t.tracker.refresh(true);
		await t.tracker.refresh();
		expect(t.getTx).toHaveBeenCalledTimes(1);
		expect(t.latest().tx).toEqual(tx);
		expect(t.latest().error).toContain('earlier snapshot');
		t.getTx.mockResolvedValue({ ...tx, indexed_height: 102 });
		await t.tracker.refresh(true);
		expect(t.getTx).toHaveBeenCalledTimes(2);
		expect(t.latest().tx?.indexed_height).toBe(102);
		expect(t.latest().error).toBeNull();
		t.tracker.stop();
	});
	it.each([
		{ label: 'without a receipt', initial: null },
		{ label: 'with a retained receipt', initial: tx }
	])(
		'retries transient failures after a forced rejected-detail retry $label',
		async ({ initial }) => {
			const t = setup(initial);
			t.getStatus.mockResolvedValue(observation('confirmed'));
			let finish!: (value: TxDto) => void;
			t.getTx
				.mockRejectedValueOnce(
					new ApiError(422, 'Unprocessable Entity', 'Expansion limit exceeded')
				)
				.mockRejectedValueOnce(new ApiError(503, 'Service Unavailable', 'Busy'))
				.mockRejectedValueOnce(new ApiError(503, 'Service Unavailable', 'Still busy'))
				.mockImplementationOnce(
					() =>
						new Promise((resolve) => {
							finish = resolve;
						})
				);
			await t.tracker.refresh(true);
			await t.tracker.refresh(true);
			expect(t.latest().error).not.toBeNull();
			expect(t.latest().tx).toEqual(initial);
			await t.tracker.refresh();
			expect(t.getTx).toHaveBeenCalledTimes(3);
			expect(t.latest().error).not.toBeNull();
			expect(t.latest().tx).toEqual(initial);
			const retry = t.tracker.refresh();
			await vi.advanceTimersByTimeAsync(0);
			expect(t.latest().checking).toBe(true);
			if (initial) expect(t.latest().error).toContain('earlier snapshot');
			expect(t.latest().tx).toEqual(initial);
			finish({ ...tx, indexed_height: 105 });
			await retry;
			expect(t.getTx).toHaveBeenCalledTimes(4);
			expect(t.latest().tx?.indexed_height).toBe(105);
			expect(t.latest().error).toBeNull();
			await t.tracker.refresh();
			expect(t.getTx).toHaveBeenCalledTimes(4);
			t.tracker.stop();
		}
	);
	it.each([
		new Error('offline'),
		new ApiError(503, 'Service Unavailable', 'Busy'),
		new ApiError(429, 'Too Many Requests', 'Retry later'),
		new ApiError(404, 'Not Found', 'Index changed')
	])('retries transient detail failures: %s', async (error) => {
		const t = setup();
		t.getStatus.mockResolvedValue(observation('confirmed'));
		t.getTx.mockRejectedValueOnce(error);
		await t.tracker.refresh();
		expect(t.latest().tx).toBeNull();
		expect(t.latest().error).not.toBeNull();
		await t.tracker.refresh();
		expect(t.getTx).toHaveBeenCalledTimes(2);
		expect(t.latest().tx).toEqual(tx);
		expect(t.latest().error).toBeNull();
		t.tracker.stop();
	});
	it('manual refresh still fetches receipt details from legacy servers', async () => {
		const t = setup(tx);
		t.getStatus.mockRejectedValue(new ApiError(404, 'Not Found', ''));
		t.getTx.mockResolvedValue({ ...tx, indexed_height: 102 });
		await t.tracker.refresh(true);
		expect(t.latest().unsupported).toBe(true);
		expect(t.latest().tx?.indexed_height).toBe(102);
		expect(t.getTx).toHaveBeenCalledTimes(1);
		t.tracker.stop();
	});
	it('removes confirmed receipts immediately when local inclusion disappears, including source outage', async () => {
		for (const state of [
			'pending',
			'not_observed',
			'no_longer_observed',
			'unavailable',
			'conflicted'
		] as const) {
			const t = setup(tx);
			t.getStatus.mockResolvedValue(observation(state));
			await t.tracker.refresh();
			expect(t.latest().tx).toBeNull();
			expect(t.latest().status?.state).toBe(state);
			t.tracker.stop();
		}
	});
	it('keeps the last receipt explicitly stale after network failure', async () => {
		const t = setup(tx);
		t.getStatus.mockRejectedValue(new Error('offline'));
		await t.tracker.refresh();
		expect(t.latest().tx).toEqual(tx);
		expect(t.latest().error).toContain('out of date');
		t.tracker.stop();
	});
	it('keeps older server receipts when the status endpoint is absent', async () => {
		const t = setup(tx);
		t.getStatus.mockRejectedValue(new ApiError(404, 'Not Found', ''));
		await t.tracker.refresh();
		expect(t.latest().tx).toEqual(tx);
		expect(t.latest().unsupported).toBe(true);
		t.tracker.stop();
	});
	it('discards old anchored details before fetching a changed inclusion', async () => {
		const t = setup(tx);
		const next = observation('confirmed');
		next.inclusion!.block_id = 'c'.repeat(64);
		t.getStatus.mockResolvedValue(next);
		t.getTx.mockRejectedValue(new Error('index changed'));
		await t.tracker.refresh();
		expect(t.latest().tx).toBeNull();
		expect(t.latest().error).not.toBeNull();
		t.tracker.stop();
	});
	it('polls only while visible and checks immediately on return', async () => {
		const t = setup();
		t.tracker.setVisible(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(t.getStatus).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(5000);
		expect(t.getStatus).toHaveBeenCalledTimes(2);
		t.tracker.setVisible(false);
		await vi.advanceTimersByTimeAsync(30000);
		expect(t.getStatus).toHaveBeenCalledTimes(2);
		t.tracker.setVisible(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(t.getStatus).toHaveBeenCalledTimes(3);
		t.tracker.stop();
	});
	it('never overlaps requests and ignores responses after a route is disposed', async () => {
		const t = setup();
		let finish!: (s: TxStatusDto) => void;
		t.getStatus.mockImplementation(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				})
		);
		t.tracker.setVisible(true);
		void t.tracker.refresh();
		await vi.advanceTimersByTimeAsync(30000);
		expect(t.getStatus).toHaveBeenCalledTimes(1);
		t.tracker.stop();
		const before = t.latest();
		finish(observation('confirmed'));
		await vi.advanceTimersByTimeAsync(10000);
		expect(t.latest()).toBe(before);
		expect(t.getTx).not.toHaveBeenCalled();
	});
	it('rejects details from an inclusion newer than the status snapshot', async () => {
		const t = setup();
		t.getStatus.mockResolvedValue(observation('confirmed'));
		t.getTx.mockResolvedValue({ ...tx, block_id: 'd'.repeat(64) });
		await t.tracker.refresh();
		expect(t.latest().tx).toBeNull();
		expect(t.latest().error).not.toBeNull();
		t.tracker.stop();
	});
});
