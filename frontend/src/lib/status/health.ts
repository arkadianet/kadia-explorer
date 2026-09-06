import type { StatusDto } from '$lib/api/types';

export type HealthState = 'unknown' | 'healthy' | 'syncing' | 'behind' | 'stalled';

export interface Health {
	state: HealthState;
	/** The word shown to a reader: "Healthy", "Syncing", "Stalled"… */
	label: string;
	/** Short pill wording for the header: "Live" or "Syncing". */
	live: string;
	tone: 'ok' | 'warn' | 'danger' | 'neutral';
	/** Sentence explaining the state, used as the element's `title`. */
	detail: string;
}

/**
 * One reading of `/v1/status`, shared by the header pill and the home page's health card so
 * the two can never disagree.
 *
 * The thresholds: at most 3 blocks behind is normal jitter between the indexer and the node
 * (a block every two minutes), up to 100 is a catch-up in progress, beyond that the index is
 * far enough behind that every age and balance on the page should be read with suspicion.
 * `stalled` and `halted` come straight from the indexer and always win.
 */
export function health(s: StatusDto | null): Health {
	if (s === null) {
		return {
			state: 'unknown',
			label: 'Unknown',
			live: 'Offline',
			tone: 'neutral',
			detail: 'The indexer status endpoint has not answered yet.'
		};
	}
	if (s.stalled !== null) {
		return {
			state: 'stalled',
			label: 'Stalled',
			live: 'Stalled',
			tone: 'danger',
			detail: `Waiting on block ${s.stalled.height} for ${s.stalled.since_secs}s: ${s.stalled.reason}`
		};
	}
	if (s.halted !== null) {
		return {
			state: 'stalled',
			label: 'Halted',
			live: 'Halted',
			tone: 'danger',
			detail: `Indexing halted: ${s.halted}`
		};
	}
	const base = `Indexed ${s.indexed ?? 0} of the node's ${s.best} — ${s.lag_blocks} blocks behind, mode ${s.mode}.`;
	if (s.lag_blocks > 100) {
		return { state: 'behind', label: 'Behind', live: 'Syncing', tone: 'danger', detail: base };
	}
	if (s.lag_blocks > 3) {
		return { state: 'syncing', label: 'Syncing', live: 'Syncing', tone: 'warn', detail: base };
	}
	return { state: 'healthy', label: 'Healthy', live: 'Live', tone: 'ok', detail: base };
}
