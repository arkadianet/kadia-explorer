import type { PageDto } from '$lib/api/types';

export interface Pager<T> {
	readonly items: T[];
	readonly loading: boolean;
	readonly error: unknown;
	readonly done: boolean;
	loadMore(): Promise<void>;
	reset(): void;
}

/** Cursor pager over a `PageDto<T>` endpoint. `loadMore` is a no-op while a load is in flight
 * or once the API returned `next_cursor: null`; `reset` puts it back to its initial state. */
export function createPager<T>(fetchPage: (cursor?: string) => Promise<PageDto<T>>): Pager<T> {
	let items = $state<T[]>([]);
	let cursor = $state<string | null>(null);
	let loading = $state(false);
	let error = $state<unknown>(null);
	let done = $state(false);

	async function loadMore(): Promise<void> {
		if (loading || done) return;
		loading = true;
		error = null;
		try {
			const p = await fetchPage(cursor ?? undefined);
			items = [...items, ...p.items];
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
