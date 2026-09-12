import { untrack } from 'svelte';
import { ApiError } from '$lib/api/client';
import type { PageDto } from '$lib/api/types';

export interface Pager<T> {
	readonly items: T[];
	readonly loading: boolean;
	readonly error: unknown;
	readonly done: boolean;
	readonly restartRequired: boolean;
	restart(): Promise<void>;
	loadMore(): Promise<void>;
	reset(): void;
}

/**
 * Carries exclusive cursor/snapshot pairs. Ordinary errors retain rows for retry;
 * 409 discards the entire chain view and latches until an explicit restart.
 * Reset also invalidates in-flight work so old filters cannot populate a new view.
 */
export function createPager<T>(
	fetchPage: (cursor?: string, snapshot?: string) => Promise<PageDto<T>>
): Pager<T> {
	let items = $state<T[]>([]);
	let cursor = $state<string | null>(null);
	let snapshot: string | undefined;
	let generation = 0;
	let restartRequired = $state(false);
	let loading = $state(false);
	let error = $state<unknown>(null);
	let done = $state(false);

	async function loadMore(): Promise<void> {
		// `untrack`: callers drive the first page from an `$effect`, so a tracked read of
		// `loading`/`done` here would subscribe that effect to them — and the `loading = false`
		// in the `finally` below would re-run it, re-calling `loadMore` forever whenever the
		// items array stays empty (i.e. on every failed request).
		if (untrack(() => loading || done || restartRequired)) return;
		const requestGeneration = generation;
		loading = true;
		error = null;
		try {
			const p = await fetchPage(cursor ?? undefined, snapshot);
			if (requestGeneration !== generation) return;
			// Skip the reassignment for an empty page: it would notify every subscriber of the
			// items signal for no visible change.
			if (p.items.length > 0) items = [...items, ...p.items];
			cursor = p.next_cursor;
			snapshot = p.next_snapshot ?? undefined;
			done = p.next_cursor === null;
		} catch (e) {
			if (requestGeneration !== generation) return;
			if (e instanceof ApiError && e.status === 409) {
				items = [];
				cursor = null;
				snapshot = undefined;
				done = false;
				restartRequired = true;
			}
			error = e;
		} finally {
			if (requestGeneration === generation) loading = false;
		}
	}

	function reset(): void {
		generation++;
		loading = false;
		restartRequired = false;
		snapshot = undefined;
		items = [];
		cursor = null;
		done = false;
		error = null;
	}

	return {
		get items() {
			return items;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		get done() {
			return done;
		},
		get restartRequired() {
			return restartRequired;
		},
		async restart() {
			reset();
			await loadMore();
		},
		loadMore,
		reset
	};
}
