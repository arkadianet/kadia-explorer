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
		.mockResolvedValueOnce(json({ items: [summary], next_cursor: '123' }))
		.mockResolvedValueOnce(json({ items: [{ ...summary, id: 'second' }], next_cursor: null }));
	const pager = createTxSummaryPager(f);
	await pager.loadMore();
	await pager.loadMore();
	await pager.loadMore();
	expect(f.mock.calls.map(([url]) => url)).toEqual([
		'/v1/tx-summaries?limit=50',
		'/v1/tx-summaries?cursor=123&limit=50'
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
	expect(f.mock.calls.map(([url]) => url)).toEqual(Array(2).fill('/v1/tx-summaries?limit=50'));
});

test('homepage deliberately requests full transactions with limit 12 and retains outputs', async () => {
	const tx = { ...newestTx, outputs: [{ ...newestTx.outputs[0], value: fee }] };
	const f = vi.fn(async (url: string) => {
		if (url === '/v1/txs?limit=12') return json({ items: [tx], next_cursor: 'unused' });
		if (url.startsWith('/v1/tx')) throw new Error(`Unexpected transaction route: ${url}`);
		return json({ items: [], next_cursor: null });
	});
	const result = await homeLoad({ fetch: f } as never);
	expect(f.mock.calls.map(([url]) => url).filter((url) => url.startsWith('/v1/tx'))).toEqual([
		'/v1/txs?limit=12'
	]);
	expect(result!.txs.data!.items).toEqual([tx]);
	expect(result!.txs.data!.items[0].outputs[0].value).toBe(fee);
});

test('block page deliberately requests expanded transactions and retains output values', async () => {
	const tx = { ...newestTx, outputs: [{ ...newestTx.outputs[0], value: fee }] };
	const f = vi.fn(async (url: string) => {
		if (url === `/v1/blocks/${block.id}`) return json(block);
		if (url === `/v1/blocks/${block.id}/txs`) return json([tx]);
		throw new Error(`Unexpected block route: ${url}`);
	});
	const result = await blockLoad({ params: { id: block.id }, fetch: f } as never);
	expect(f.mock.calls.map(([url]) => url)).toEqual([
		`/v1/blocks/${block.id}`,
		`/v1/blocks/${block.id}/txs`
	]);
	expect(result!.txs).toEqual([tx]);
	expect(result!.txs[0].outputs[0].value).toBe(fee);
});
