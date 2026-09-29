import type { GroupBalances } from './groups';

export const WATCH_INTERVAL_MS = 60_000;
export const MAX_WATCH_EVENTS = 20;
export interface BalanceChange {
	nano: string;
	tokens: { id: string; delta: string }[];
}
export interface WatchEvent {
	at: number;
	kind: 'baseline' | 'change' | 'reset' | 'unavailable' | 'error';
	message: string;
	from: GroupBalances['anchor'];
	to: GroupBalances['anchor'];
	change: BalanceChange | null;
}
export interface WatchState {
	active: boolean;
	phase: 'stopped' | 'reading' | 'waiting' | 'paused';
	lastChecked: number | null;
	nextCheck: number | null;
	message: string;
	events: WatchEvent[];
}
export const emptyWatch = (): WatchState => ({
	active: false,
	phase: 'stopped',
	lastChecked: null,
	nextCheck: null,
	message: 'Not watching. Start explicitly to check while this page is open.',
	events: []
});
const comparable = (snapshot: GroupBalances) =>
	snapshot.complete &&
	snapshot.full_history &&
	snapshot.anchor !== null &&
	snapshot.observed_totals !== null &&
	snapshot.members.every((member) => member.status === 'resolved' || member.status === 'duplicate');
const scripts = (snapshot: GroupBalances) =>
	snapshot.members
		.filter((member) => member.status === 'resolved')
		.map((member) => member.tree_hash)
		.sort()
		.join(',');

/** A change between complete observed snapshots, never a sum of partial member values. */
export function compareBalances(before: GroupBalances, after: GroupBalances): BalanceChange | null {
	if (!comparable(before) || !comparable(after) || scripts(before) !== scripts(after)) return null;
	const previous = new Map(
		before.observed_totals!.tokens.map((token) => [token.id, BigInt(token.amount)])
	);
	const current = new Map(
		after.observed_totals!.tokens.map((token) => [token.id, BigInt(token.amount)])
	);
	const ids = new Set([...previous.keys(), ...current.keys()]);
	return {
		nano: (BigInt(after.observed_totals!.nano) - BigInt(before.observed_totals!.nano)).toString(),
		tokens: [...ids].sort().flatMap((id) => {
			const delta = (current.get(id) ?? 0n) - (previous.get(id) ?? 0n);
			return delta === 0n ? [] : [{ id, delta: delta.toString() }];
		})
	};
}

export function createBalanceWatch(options: {
	read: () => Promise<GroupBalances>;
	onChange: (state: WatchState) => void;
	visible: () => boolean;
	now?: () => number;
}) {
	let state = emptyWatch();
	let baseline: GroupBalances | null = null;
	let generation = 0;
	let lastAttempt: number | null = null;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let busy = false;
	let disposed = false;
	let selection = '';
	const now = options.now ?? Date.now;
	const publish = (next: WatchState) => {
		state = next;
		options.onChange(next);
	};
	const cancel = () => {
		if (timer !== null) clearTimeout(timer);
		timer = null;
	};
	function record(
		kind: WatchEvent['kind'],
		message: string,
		from: GroupBalances['anchor'] = null,
		to: GroupBalances['anchor'] = null,
		change: BalanceChange | null = null
	) {
		publish({
			...state,
			message,
			events: [{ at: now(), kind, message, from, to, change }, ...state.events].slice(
				0,
				MAX_WATCH_EVENTS
			)
		});
	}
	function schedule() {
		cancel();
		if (!state.active || disposed || busy) return;
		if (!options.visible()) {
			publish({
				...state,
				phase: 'paused',
				nextCheck: null,
				message: 'Paused while this page is hidden. No background checks.'
			});
			return;
		}
		const nextCheck = (lastAttempt ?? now()) + WATCH_INTERVAL_MS;
		publish({ ...state, phase: 'waiting', nextCheck });
		timer = setTimeout(() => void poll(), Math.max(0, nextCheck - now()));
	}
	async function poll() {
		if (!state.active || disposed || busy) return;
		if (!options.visible()) {
			schedule();
			return;
		}
		if (lastAttempt !== null && now() - lastAttempt < WATCH_INTERVAL_MS) {
			schedule();
			return;
		}
		const request = generation;
		lastAttempt = now();
		busy = true;
		publish({
			...state,
			phase: 'reading',
			nextCheck: null,
			message: 'Checking the selected addresses together…'
		});
		try {
			const snapshot = await options.read();
			if (disposed || request !== generation || !state.active) return;
			publish({ ...state, lastChecked: now() });
			if (!comparable(snapshot)) {
				baseline = null;
				record(
					'unavailable',
					'No change comparison: this read is incomplete, has unresolved members, or has no canonical block anchor. A future complete read starts a new baseline.',
					null,
					snapshot.anchor
				);
			} else if (baseline === null) {
				baseline = snapshot;
				record(
					'baseline',
					'Complete anchored baseline established. Changes will be observed from a later complete read.',
					null,
					snapshot.anchor
				);
			} else {
				const before = baseline;
				const change = compareBalances(before, snapshot);
				const movedBack = snapshot.anchor!.height < before.anchor!.height;
				const replaced =
					snapshot.anchor!.height === before.anchor!.height &&
					snapshot.anchor!.block_id !== before.anchor!.block_id;
				const sameAnchor =
					snapshot.anchor!.height === before.anchor!.height &&
					snapshot.anchor!.block_id === before.anchor!.block_id;
				const changed = change !== null && (change.nano !== '0' || change.tokens.length > 0);
				baseline = snapshot;
				if (movedBack || replaced || change === null || (sameAnchor && changed))
					record(
						'reset',
						'Comparison baseline reset: the indexed anchor, canonical script set or same-anchor evidence changed. No monetary change is claimed across this reset.',
						before.anchor,
						snapshot.anchor
					);
				else if (changed)
					record(
						'change',
						'Observed change between complete indexed snapshots.',
						before.anchor,
						snapshot.anchor,
						change
					);
				else
					publish({ ...state, message: 'No observed balance change between complete snapshots.' });
			}
		} catch (error) {
			if (disposed || request !== generation || !state.active) return;
			baseline = null;
			record(
				'error',
				`Check failed; comparison baseline cleared. ${error instanceof Error ? error.message : 'Try again when the index is available.'}`
			);
		} finally {
			if (request === generation) {
				busy = false;
				schedule();
			}
		}
	}
	return {
		get state() {
			return state;
		},
		setSelection(addresses: readonly string[]) {
			const next = JSON.stringify(addresses);
			if (next === selection) return;
			selection = next;
			generation++;
			cancel();
			busy = false;
			baseline = null;
			lastAttempt = null;
			publish({
				...emptyWatch(),
				message: 'Selection changed. Watching is stopped; start explicitly for these addresses.'
			});
		},
		start() {
			if (disposed || state.active || !selection || selection === '[]') return;
			generation++;
			baseline = null;
			busy = false;
			publish({ ...state, active: true, message: 'Starting a new page-open watch.' });
			void poll();
		},
		stop() {
			generation++;
			cancel();
			busy = false;
			baseline = null;
			publish({
				...state,
				active: false,
				phase: 'stopped',
				nextCheck: null,
				message: 'Watching stopped. No further checks will run.'
			});
		},
		visibilityChanged() {
			if (!state.active || disposed) return;
			if (!options.visible()) {
				cancel();
				publish({
					...state,
					phase: 'paused',
					nextCheck: null,
					message: 'Paused while this page is hidden. No background checks.'
				});
			} else if (busy)
				publish({
					...state,
					phase: 'reading',
					message: 'Finishing the current selected-address check…'
				});
			else if (lastAttempt === null || now() - lastAttempt >= WATCH_INTERVAL_MS) void poll();
			else schedule();
		},
		destroy() {
			disposed = true;
			generation++;
			cancel();
			busy = false;
			baseline = null;
		}
	};
}
