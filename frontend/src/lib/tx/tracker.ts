import { ApiError } from '../api/client';
import type { TxDto, TxStatusDto } from '../api/types';

export interface TrackingState {
	tx: TxDto | null;
	status: TxStatusDto | null;
	checking: boolean;
	error: string | null;
	unsupported: boolean;
}

export function sameInclusion(tx: TxDto, status: TxStatusDto): boolean {
	return (
		status.state === 'confirmed' &&
		status.inclusion !== null &&
		tx.id === status.id &&
		tx.height === status.inclusion.height &&
		tx.block_id === status.inclusion.block_id
	);
}

/** Poll small snapshots; only fetch expanded boxes when their block anchor changes.
 * Visibility and disposal are explicit so no obsolete request can publish to a new route. */
export function createTransactionTracker(options: {
	id: string;
	tx: TxDto | null;
	status: TxStatusDto | null;
	getStatus: () => Promise<TxStatusDto>;
	getTx: () => Promise<TxDto>;
	onChange: (state: TrackingState) => void;
}) {
	let state: TrackingState = {
		tx: options.tx,
		status: options.status,
		checking: false,
		error: null,
		unsupported: false
	};
	let visible = false;
	let stopped = false;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let running: Promise<void> | null = null;
	const publish = (patch: Partial<TrackingState>) => {
		if (stopped) return;
		state = { ...state, ...patch };
		options.onChange(state);
	};
	const clear = () => {
		clearTimeout(timer);
		timer = undefined;
	};
	const schedule = () => {
		clear();
		if (!stopped && visible && !state.unsupported) timer = setTimeout(() => void refresh(), 5000);
	};

	async function update(forceDetails: boolean) {
		publish({ checking: true });
		let failure =
			'Live check failed. The last successful observation is shown; it may be out of date.';
		try {
			const status = await options.getStatus();
			if (stopped) return;
			if (status.id !== options.id || (status.state === 'confirmed' && !status.inclusion)) {
				throw new Error('Invalid transaction status');
			}
			// A successful snapshot supersedes any prior inclusion immediately, even when
			// the mempool could not be checked. Never leave an orphaned receipt on screen.
			if (status.state !== 'confirmed') {
				publish({ status, tx: null, error: null, unsupported: false });
				return;
			}
			let tx = state.tx;
			if (forceDetails || !tx || !sameInclusion(tx, status)) {
				const retained = tx && sameInclusion(tx, status) ? tx : null;
				publish({ status, tx: retained, error: null, unsupported: false });
				failure = retained
					? 'Receipt refresh failed. Box details are from the earlier snapshot shown below.'
					: 'Receipt details are unavailable. Live inclusion is shown; checking again shortly.';
				tx = await options.getTx();
				if (stopped) return;
				if (!sameInclusion(tx, status)) {
					publish({ tx: null });
					throw new Error('Transaction inclusion changed during refresh');
				}
			}
			publish({
				status,
				tx,
				error: null,
				unsupported: false
			});
		} catch (error) {
			if (stopped) return;
			if (error instanceof ApiError && error.status === 404 && state.status === null) {
				// Compatibility with an older API: the existing receipt remains useful,
				// but a missing endpoint does not establish a transaction's current state.
				publish({ unsupported: true, error: null });
				if (forceDetails && state.tx) {
					try {
						const tx = await options.getTx();
						if (tx.id === options.id) publish({ tx });
					} catch {
						publish({ error: 'Receipt refresh failed. The saved snapshot may be out of date.' });
					}
				}
			} else {
				publish({ error: failure });
			}
		} finally {
			publish({ checking: false });
		}
	}

	function refresh(forceDetails = false): Promise<void> {
		if (stopped) return Promise.resolve();
		if (running) return running;
		clear();
		running = update(forceDetails).finally(() => {
			running = null;
			schedule();
		});
		return running;
	}

	return {
		refresh,
		setVisible(value: boolean) {
			if (stopped || visible === value) return;
			visible = value;
			clear();
			if (visible) void refresh();
		},
		stop() {
			stopped = true;
			clear();
		}
	};
}
