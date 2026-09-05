import { describe, expect, it } from 'vitest';
import { createPager } from '../../src/lib/pager/pager.svelte';
import type { PageDto } from '../../src/lib/api/types';

function page<T>(items: T[], next_cursor: string | null): PageDto<T> {
	return { items, next_cursor };
}

describe('createPager', () => {
	it('appends the first page and carries the cursor into the next fetch', async () => {
		const seen: (string | undefined)[] = [];
		const pager = createPager<number>(async (cursor) => {
			seen.push(cursor);
			return seen.length === 1 ? page([1, 2], 'c1') : page([3, 4], 'c2');
		});

		expect(pager.items).toEqual([]);
		expect(pager.done).toBe(false);

		await pager.loadMore();
		expect(pager.items).toEqual([1, 2]);
		expect(pager.done).toBe(false);
		expect(pager.loading).toBe(false);

		await pager.loadMore();
		expect(pager.items).toEqual([1, 2, 3, 4]);
		expect(seen).toEqual([undefined, 'c1']);
	});

	it('marks done when next_cursor is null and stops fetching', async () => {
		let calls = 0;
		const pager = createPager<number>(async () => {
			calls += 1;
			return calls === 1 ? page([1], 'c1') : page([2], null);
		});

		await pager.loadMore();
		await pager.loadMore();
		expect(pager.items).toEqual([1, 2]);
		expect(pager.done).toBe(true);

		await pager.loadMore();
		expect(calls).toBe(2);
		expect(pager.items).toEqual([1, 2]);
	});

	it('does not double-load while a load is in flight', async () => {
		let calls = 0;
		let release!: (p: PageDto<number>) => void;
		const pager = createPager<number>(() => {
			calls += 1;
			return new Promise<PageDto<number>>((resolve) => {
				release = resolve;
			});
		});

		const first = pager.loadMore();
		expect(pager.loading).toBe(true);
		await pager.loadMore();
		expect(calls).toBe(1);

		release(page([1], 'c1'));
		await first;
		expect(pager.loading).toBe(false);
		expect(pager.items).toEqual([1]);
	});

	it('records the error and keeps the items already loaded', async () => {
		let calls = 0;
		const boom = new Error('boom');
		const pager = createPager<number>(async () => {
			calls += 1;
			if (calls === 1) return page([1], 'c1');
			throw boom;
		});

		await pager.loadMore();
		await pager.loadMore();

		expect(pager.error).toBe(boom);
		expect(pager.items).toEqual([1]);
		expect(pager.loading).toBe(false);
		expect(pager.done).toBe(false);
	});

	it('clears the error on a subsequent successful load', async () => {
		let calls = 0;
		const pager = createPager<number>(async () => {
			calls += 1;
			if (calls === 1) throw new Error('boom');
			return page([7], null);
		});

		await pager.loadMore();
		expect(pager.error).toBeTruthy();

		await pager.loadMore();
		expect(pager.error).toBe(null);
		expect(pager.items).toEqual([7]);
		expect(pager.done).toBe(true);
	});

	it('an effect-driven caller does not re-enter after a failure', async () => {
		// The `$effect`s that kick off the first page re-run whenever a signal they read
		// changes. `loadMore` reads its `loading`/`done` guard through `untrack` precisely so
		// those effects never subscribe to it: a failed load must leave the pager parked on the
		// error, with nothing pulling it back into another request. What a failure leaves
		// behind: `error` set, `items`/`done` untouched, `loading` false — and a *deliberate*
		// retry (the Retry button, a fresh sentinel intersection) is still allowed.
		let calls = 0;
		const pager = createPager<number>(async () => {
			calls += 1;
			throw new Error(`boom ${calls}`);
		});

		await pager.loadMore();
		expect(calls).toBe(1);
		expect(pager.error).toBeInstanceOf(Error);
		expect(pager.items).toEqual([]);
		expect(pager.done).toBe(false);
		expect(pager.loading).toBe(false);

		// Nothing re-entered on its own: the count only moves when a caller asks again.
		await Promise.resolve();
		expect(calls).toBe(1);

		await pager.loadMore();
		expect(calls).toBe(2);
	});

	it('does not touch the items array for an empty page', async () => {
		const pager = createPager<number>(async () => page([], 'c1'));

		await pager.loadMore();
		const first = pager.items;
		await pager.loadMore();

		expect(pager.items).toEqual([]);
		// Same array instance both times — an empty page notifies no subscriber.
		expect(pager.items).toBe(first);
	});

	it('reset clears items, cursor, error and done', async () => {
		const seen: (string | undefined)[] = [];
		const pager = createPager<number>(async (cursor) => {
			seen.push(cursor);
			return page([1], null);
		});

		await pager.loadMore();
		expect(pager.done).toBe(true);

		pager.reset();
		expect(pager.items).toEqual([]);
		expect(pager.done).toBe(false);
		expect(pager.error).toBe(null);

		await pager.loadMore();
		expect(seen).toEqual([undefined, undefined]);
	});
});
