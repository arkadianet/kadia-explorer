import { afterEach, expect, test, vi } from 'vitest';
import { createTxSummaryPager } from '$lib/tx/summaryPager.svelte';
import { formatErg } from '$lib/format/amount';
import { load as homeLoad } from '../../src/routes/+page';
import { load as blockLoad } from '../../src/routes/blocks/[id]/+page';
import { block, newestTx } from '../e2e/data';

const fee = '9007199254999999';
const summary = {
	id: newestTx.id,
	height: newestTx.height,
	index: 0,
	timestamp: 0,
	size: 100,
	fee,
	input_count: 7,
	data_input_count: 1,
	output_count: 9
};
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
afterEach(() => vi.restoreAllMocks());

test('global list loads only summary pages and preserves large fees and counts', async () => {
	const f = vi
		.fn()
		.mockResolvedValueOnce(json({ items: [summary], next_cursor: '123', next_snapshot: 'aa' }))
		.mockResolvedValueOnce(json({ items: [{ ...summary, id: 'second' }], next_cursor: null }));
	const pager = createTxSummaryPager(f);
	await pager.loadMore();
	await pager.loadMore();
	await pager.loadMore();
	expect(f.mock.calls.map(([url]) => url)).toEqual([
		'/v1/tx-summaries?limit=50&consistency=strict',
		'/v1/tx-summaries?cursor=123&limit=50&consistency=strict&snapshot=aa'
	]);
	expect(pager.items).toHaveLength(2);
	expect(pager.items[0]).toEqual(summary);
	expect(formatErg(pager.items[0].fee)).toBe('9,007,199.254999999');
	expect(pager.done).toBe(true);
});

test('summary route error is exposed and retries the same page', async () => {
	const f = vi
		.fn()
		.mockResolvedValueOnce(json({ detail: 'summary unavailable' }, 503))
		.mockResolvedValueOnce(json({ items: [summary], next_cursor: null }));
	const pager = createTxSummaryPager(f);
	await pager.loadMore();
	expect(pager.error).toMatchObject({ status: 503, detail: 'summary unavailable' });
	expect(pager.items).toEqual([]);
	expect(pager.done).toBe(false);
	await pager.loadMore();
	expect(pager.error).toBeNull();
	expect(f.mock.calls.map(([url]) => url)).toEqual(
		Array(2).fill('/v1/tx-summaries?limit=50&consistency=strict')
	);
});

test('homepage summaries preserve exact large fees without expanding outputs', async () => {
	const f = vi.fn(async (url: string) => {
		if (url === '/v1/tx-summaries?limit=6&consistency=strict')
			return json({ items: [summary], next_cursor: null });
		if (url.startsWith('/v1/tx')) throw new Error(`Unexpected transaction route: ${url}`);
		return json({ items: [], next_cursor: null });
	});
	const result = await homeLoad({ fetch: f } as never);
	expect(result!.txs.data!.items).toEqual([summary]);
	expect(result!.txs.data!.items[0].fee).toBe(fee);
});

test('block load pins reward evidence to resolved ID and survives unavailable evidence', async () => {
	const f = vi.fn(async (url: string) => {
		if (url === `/v1/blocks/${block.height}`) return json(block);
		if (url === `/v1/blocks/${block.id}/rewards`)
			return json({ detail: 'evidence unavailable' }, 503);
		throw new Error(`Unexpected block route: ${url}`);
	});
	const result = await blockLoad({ params: { id: String(block.height) }, fetch: f } as never);
	expect(f.mock.calls.map(([url]) => url)).toEqual([
		`/v1/blocks/${block.height}`,
		`/v1/blocks/${block.id}/rewards`
	]);
	expect(result!.block).toEqual(block);
	expect(result!.rewards.data).toBeNull();
	expect(result!.rewards.error).toMatchObject({ status: 503 });
});

test('409 never concatenates pre-change rows with restarted pages and never auto-retries', async () => {
	const f = vi
		.fn()
		.mockResolvedValueOnce(
			json({ items: [{ ...summary, id: 'old' }], next_cursor: '1', next_snapshot: 'aa' })
		)
		.mockResolvedValueOnce(json({ detail: 'anchor changed' }, 409))
		.mockResolvedValueOnce(
			json({ items: [{ ...summary, id: 'new' }], next_cursor: '2', next_snapshot: 'bb' })
		)
		.mockResolvedValueOnce(json({ items: [{ ...summary, id: 'new-next' }], next_cursor: null }));
	const pager = createTxSummaryPager(f);
	await pager.loadMore();
	await pager.loadMore();
	expect(pager.items).toEqual([]);
	expect(pager.restartRequired).toBe(true);
	// Sentinel/effect callbacks, even if already queued, cannot restart a changed chain.
	await pager.loadMore();
	await pager.loadMore();
	expect(f).toHaveBeenCalledTimes(2);
	await pager.restart();
	await pager.loadMore();
	expect(pager.items.map((item) => item.id)).toEqual(['new', 'new-next']);
	expect(f.mock.calls.map(([url]) => url)).toEqual([
		'/v1/tx-summaries?limit=50&consistency=strict',
		'/v1/tx-summaries?cursor=1&limit=50&consistency=strict&snapshot=aa',
		'/v1/tx-summaries?limit=50&consistency=strict',
		'/v1/tx-summaries?cursor=2&limit=50&consistency=strict&snapshot=bb'
	]);
});
