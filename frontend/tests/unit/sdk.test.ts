import { describe, expect, it, vi } from 'vitest';
import {
	createKadiaClient,
	KadiaApiError,
	KadiaLimitError
} from '../../static/sdk/kadia-client.js';
import contract from '../../static/openapi.json';
import { readFileSync } from 'node:fs';

const id = 'a'.repeat(64);
const baseUrl = 'https://example.test/v1';
const page = (cursor: string | null, snapshot = 'snapshot-a', height = 2) => ({
	items: [],
	next_cursor: cursor,
	next_snapshot: cursor ? snapshot : null,
	consistency: 'strict',
	anchor: { height, block_id: id }
});
const stub = (...responses: Response[]) =>
	vi.fn<typeof fetch>().mockImplementation(async () => {
		const response = responses.shift();
		if (!response) throw new Error('Unexpected request');
		return response;
	});

describe('portable typed API client', () => {
	it('retains coverage metadata and treats an absent header as unknown', async () => {
		const fetch = stub(
			Response.json({ indexed: 2 }, { headers: { 'x-explorer-completeness': 'incomplete' } }),
			Response.json({ indexed: 2 })
		);
		const client = createKadiaClient({ baseUrl, fetch });
		await expect(client.observe('status', {})).resolves.toMatchObject({
			status: 200,
			completeness: 'incomplete',
			data: { indexed: 2 }
		});
		await expect(client.observe('status', {})).resolves.toMatchObject({ completeness: 'unknown' });
	});
	it('keeps exact wire amounts, encodes address paths and omits ambient credentials', async () => {
		const fetch = stub(Response.json({ balance: { nano: '9007199254740993123' } }));
		const client = createKadiaClient({ baseUrl, fetch });
		const result = await client.request('address', { addr: '9abc/path?x' });
		expect(result.balance.nano).toBe('9007199254740993123');
		expect(fetch.mock.calls[0][0]).toBe('https://example.test/v1/addresses/9abc%2Fpath%3Fx');
		expect(fetch.mock.calls[0][1]).toMatchObject({
			credentials: 'omit',
			redirect: 'error',
			referrerPolicy: 'no-referrer'
		});
	});
	it('sends selected addresses once in a read-only POST body', async () => {
		const fetch = stub(Response.json({ members: [] }));
		await createKadiaClient({ baseUrl, fetch }).request('groupBalances', {
			addresses: ['9abc', '9def']
		});
		expect(fetch.mock.calls[0][0]).toBe(baseUrl + '/addresses/balances');
		expect(fetch.mock.calls[0][1]).toMatchObject({
			method: 'POST',
			body: '{"addresses":["9abc","9def"]}'
		});
	});
	it('rejects invalid parameters and prototype operation names before sending', async () => {
		const fetch = vi.fn<typeof globalThis.fetch>();
		const client = createKadiaClient({ baseUrl, fetch });
		await expect(client.request('transaction', { id: '../status' })).rejects.toThrow(
			'Invalid parameter'
		);
		await expect(
			client.request('compareBalances', { addr: '9abc', from_height: 5, to_height: 2 })
		).rejects.toThrow('from_height');
		await expect(
			client.request('historicalBalance', { addr: '9abc', height: 0, block_id: id })
		).rejects.toThrow('Genesis');
		// @ts-expect-error Unsupported operation must also be rejected for JavaScript consumers.
		await expect(client.request('toString', {})).rejects.toThrow('Unsupported');
		// @ts-expect-error Strict query shapes reject extra keys.
		await expect(client.request('status', { url: 'https://other.test' })).rejects.toThrow(
			'Unsupported parameter'
		);
		expect(fetch).not.toHaveBeenCalled();
	});
	it('bounds address count and UTF-8 request bytes', async () => {
		const client = createKadiaClient({ baseUrl, fetch: vi.fn() });
		await expect(client.request('groupBalances', { addresses: [] })).rejects.toThrow();
		await expect(
			client.request('groupBalances', { addresses: Array(101).fill('9abc') })
		).rejects.toThrow();
		await expect(
			client.request('groupBalances', { addresses: ['\u4e00'.repeat(2000)] })
		).rejects.toBeInstanceOf(KadiaLimitError);
	});
	it('rejects route-specific paging and empty date windows before fetching', async () => {
		const fetch = vi.fn<typeof globalThis.fetch>();
		const client = createKadiaClient({ baseUrl, fetch });
		// @ts-expect-error Blocks are descending-only in both the SDK and server contract.
		await expect(client.request('blocks', { dir: 'asc' })).rejects.toThrow('Invalid parameter');
		await expect(client.request('addressActivity', { addr: '9abc', limit: 101 })).rejects.toThrow(
			'Invalid parameter'
		);
		await expect(
			client.request('addressActivity', { addr: '9abc', from_ms: 10, to_ms: 10 })
		).rejects.toThrow('must precede');
		expect(fetch).not.toHaveBeenCalled();
	});
	it('supports the complete 512-byte token-name query range and preserves non-ASCII matching', async () => {
		const fetch = stub(
			Response.json({ items: [] }),
			Response.json({ items: [] }),
			Response.json({ items: [] })
		);
		const client = createKadiaClient({ baseUrl, fetch });
		for (const q of ['a'.repeat(512), 'é'.repeat(256), '\u00a0'])
			await client.request('searchTokens', { q });
		expect(fetch).toHaveBeenCalledTimes(3);
		for (const q of ['a'.repeat(513), 'é'.repeat(257), '😀'.repeat(129), ' \t\r\n'])
			await expect(client.request('searchTokens', { q })).rejects.toThrow();
		expect(fetch).toHaveBeenCalledTimes(3);
	});
	it('rejects malformed UTF-8 on successful responses without replacing evidence text', async () => {
		const bytes = new Uint8Array([123, 34, 110, 97, 109, 101, 34, 58, 34, 255, 34, 125]);
		const fetch = stub(new Response(bytes));
		await expect(createKadiaClient({ baseUrl, fetch }).request('status', {})).rejects.toThrow(
			'invalid UTF-8'
		);
	});
	it('retains HTTP status for malformed UTF-8 failure bodies', async () => {
		const fetch = stub(new Response(new Uint8Array([255]), { status: 503 }));
		await expect(createKadiaClient({ baseUrl, fetch }).request('status', {})).rejects.toMatchObject(
			{ status: 503, data: null }
		);
	});
	it('preserves HTTP problem codes and Retry-After without retrying', async () => {
		const fetch = stub(
			Response.json(
				{ code: 'rate_limited', detail: 'Try later.' },
				{ status: 429, headers: { 'retry-after': '3' } }
			)
		);
		const result = createKadiaClient({ baseUrl, fetch }).request('status', {});
		await expect(result).rejects.toMatchObject({
			status: 429,
			code: 'rate_limited',
			retryAfter: '3'
		});
		await expect(result).rejects.toBeInstanceOf(KadiaApiError);
		expect(fetch).toHaveBeenCalledTimes(1);
	});
	it('keeps a framework error status when the body is not JSON', async () => {
		const fetch = stub(new Response('Timed out', { status: 408 }));
		await expect(createKadiaClient({ baseUrl, fetch }).request('status', {})).rejects.toMatchObject(
			{ status: 408, data: null }
		);
	});
	it('rejects invalid JSON and advertised or streamed oversized responses', async () => {
		for (const response of [
			new Response('{'),
			new Response('x', { headers: { 'content-length': '100' } }),
			new Response('x'.repeat(100))
		]) {
			await expect(
				createKadiaClient({ baseUrl, fetch: stub(response), maxResponseBytes: 20 }).request(
					'status',
					{}
				)
			).rejects.toThrow();
		}
	});
	it('aborts before fetching and cancels a stalled response body', async () => {
		const canceled = new AbortController();
		canceled.abort();
		const fetch = vi.fn<typeof globalThis.fetch>();
		await expect(
			createKadiaClient({ baseUrl, fetch }).request('status', {}, { signal: canceled.signal })
		).rejects.toThrow();
		expect(fetch).not.toHaveBeenCalled();
		const cancel = vi.fn();
		const response = new Response(new ReadableStream({ cancel }));
		await expect(
			createKadiaClient({ baseUrl, fetch: stub(response), timeoutMs: 5 }).request('status', {})
		).rejects.toThrow('timed out');
		expect(cancel).toHaveBeenCalledOnce();
	});
	it('walks empty pages using both opaque continuation fields', async () => {
		const fetch = stub(Response.json(page('cursor-a')), Response.json(page(null)));
		const pages = [];
		for await (const result of createKadiaClient({ baseUrl, fetch }).pages('blocks', { limit: 2 }))
			pages.push(result);
		expect(pages).toHaveLength(2);
		const url = new URL(String(fetch.mock.calls[1][0]));
		expect(Object.fromEntries(url.searchParams)).toEqual({
			limit: '2',
			consistency: 'strict',
			cursor: 'cursor-a',
			snapshot: 'snapshot-a'
		});
	});
	it('fails instead of silently returning a complete-looking bounded walk', async () => {
		const fetch = stub(Response.json(page('more')));
		const walk = createKadiaClient({ baseUrl, fetch }).pages('blocks', {}, { maxPages: 1 });
		await expect(walk.next()).resolves.toMatchObject({ done: false });
		await expect(walk.next()).rejects.toBeInstanceOf(KadiaLimitError);
		expect(fetch).toHaveBeenCalledOnce();
	});
	it('rejects missing snapshot, repeated cursor and changed anchor', async () => {
		for (const second of [
			page('same'),
			page(null, 'snapshot-a', 3),
			{ ...page('another'), next_snapshot: null }
		]) {
			const fetch = stub(Response.json(page('same')), Response.json(second));
			const walk = createKadiaClient({ baseUrl, fetch }).pages('blocks', {});
			await walk.next();
			await expect(walk.next()).rejects.toThrow();
		}
	});
	it('surfaces reorg conflict and does not restart or blend pages', async () => {
		const fetch = stub(
			Response.json(page('more')),
			Response.json({ code: 'snapshot_changed', detail: 'Restart' }, { status: 409 })
		);
		const walk = createKadiaClient({ baseUrl, fetch }).pages('blocks', {});
		await walk.next();
		await expect(walk.next()).rejects.toMatchObject({ status: 409, code: 'snapshot_changed' });
		expect(fetch).toHaveBeenCalledTimes(2);
	});
	it('rejects invalid or excessive network ranges before starting a request', async () => {
		const fetch = vi.fn<typeof globalThis.fetch>();
		const client = createKadiaClient({ baseUrl, fetch });
		for (const params of [
			{ from_height: 0, to_height: 1 },
			{ from_height: 5, to_height: 1 },
			{ from_height: 1, to_height: 20161 },
			{ from_height: 1, to_height: 10, buckets: 121 }
		])
			await expect(client.request('networkHistory', params)).rejects.toThrow();
		expect(fetch).not.toHaveBeenCalled();
	});
	it('documents only registered routes and resolves every schema reference', () => {
		const router = readFileSync(
			new URL('../../../crates/xp-api/src/lib.rs', import.meta.url),
			'utf8'
		);
		for (const path of Object.keys(contract.paths)) expect(router).toContain('"/v1' + path + '"');
		const source = JSON.stringify(contract);
		for (const match of source.matchAll(/"\$ref":"#\/components\/schemas\/([^"/]+)"/g))
			expect(contract.components.schemas).toHaveProperty(match[1]);
		expect(contract.paths['/addresses/balances']).toHaveProperty('post');
		expect(contract.components.schemas.TokenDto.properties.amount).toEqual({ type: 'string' });
		expect(contract.components.schemas.TxDto.properties.fee).toEqual({ type: 'string' });
		const activity = contract.paths['/addresses/{addr}/activity'].get.parameters;
		expect(activity.find((parameter) => parameter.name === 'consistency')?.schema).toEqual({
			type: 'string',
			enum: ['strict'],
			default: 'strict'
		});
		expect(activity.find((parameter) => parameter.name === 'limit')?.schema).toMatchObject({
			maximum: 100
		});
		expect(
			contract.paths['/blocks'].get.parameters.find((parameter) => parameter.name === 'dir')?.schema
		).toEqual({ type: 'string', enum: ['desc'] });
	});
	it('bounds mining ranges and requires the compact rent view before issuing requests', async () => {
		const fetch = vi.fn<typeof globalThis.fetch>();
		const client = createKadiaClient({ baseUrl, fetch });
		for (const params of [
			{ from_height: 0, to_height: 1 },
			{ from_height: 5, to_height: 1 },
			{ from_height: 1, to_height: 20161 },
			{ from_height: 1, to_height: 10, top: 51 }
		])
			await expect(client.request('mining', params)).rejects.toThrow();
		await expect(
			client.request('addressRentExposure', { addr: '9abc' } as never)
		).rejects.toThrow();
		await expect(
			client.request('addressRentExposure', { addr: '9abc', view: 'legacy' } as never)
		).rejects.toThrow();
		expect(fetch).not.toHaveBeenCalled();
	});
});
