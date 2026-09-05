import { untrack } from 'svelte';
import type { PageDto } from '$lib/api/types';

export interface Pager<T> {
	readonly items: T[];
	readonly loading: boolean;
	readonly error: unknown;
	readonly done: boolean;
	loadMore(): Promise<void>;
	reset(): void;
}

/**
 * Cursor pager over a `PageDto<T>` endpoint.
 *
 * Semantics `InfiniteList` and its callers rely on:
 * - `loadMore` is a no-op while a load is in flight, and once the API returned
 *   `next_cursor: null` (`done`).
 * - A failed load leaves `items`, `cursor` and `done` untouched and sets `error`. It does
 *   *not* latch: the next `loadMore` retries the same cursor and clears `error` — but nothing
 *   calls it on its own, so a retry is always an explicit act (the Retry button, or a fresh
 *   sentinel intersection). Callers must not drive retries from an `$effect` that reads the
 *   pager's own state; see the `untrack` note in `loadMore`.
 * - `reset` puts it back to its initial state.
 */
export function createPager<T>(fetchPage: (cursor?: string) => Promise<PageDto<T>>): Pager<T> {
	let items = $state<T[]>([]);
	let cursor = $state<string | null>(null);
	let loading = $state(false);
	let error = $state<unknown>(null);
	let done = $state(false);

	async function loadMore(): Promise<void> {
		// `untrack`: callers drive the first page from an `$effect`, so a tracked read of
		// `loading`/`done` here would subscribe that effect to them — and the `loading = false`
		// in the `finally` below would re-run it, re-calling `loadMore` forever whenever the
		// items array stays empty (i.e. on every failed request).
		if (untrack(() => loading || done)) return;
		loading = true;
		error = null;
		try {
			const p = await fetchPage(cursor ?? undefined);
			// Skip the reassignment for an empty page: it would notify every subscriber of the
			// items signal for no visible change.
			if (p.items.length > 0) items = [...items, ...p.items];
			cursor = p.next_cursor;
			done = p.next_cursor === null;
		} catch (e) {
			error = e;
		} finally {
			loading = false;
		}
	}

	function reset(): void {
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
		loadMore,
		reset
	};
}
