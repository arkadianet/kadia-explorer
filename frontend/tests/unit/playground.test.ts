import { describe, expect, it, vi } from 'vitest';
import {
	buildRequest,
	continuation,
	curlCommand,
	defaults,
	ENDPOINTS,
	executeRequest,
	readShare,
	RESPONSE_LIMIT,
	sharePath
} from '$lib/developer/playground';

describe('API playground requests', () => {
	it('bounds mining and history ranges and selects the compact rent response', () => {
		for (const kind of ['mining', 'network-history']) {
			for (const values of [
				{ from_height: '0', to_height: '2' },
				{ from_height: '3', to_height: '2' },
				{ from_height: '1', to_height: '20161' }
			])
				expect(() => buildRequest(kind, values)).toThrow();
		}
		expect(() => buildRequest('mining', { from_height: '1', to_height: '2', top: '51' })).toThrow();
		expect(() =>
			buildRequest('network-history', { from_height: '1', to_height: '2', buckets: '121' })
		).toThrow();
		expect(buildRequest('rent-exposure', { address: '9addressTestQqWwee' })).toBe(
			'/v1/addresses/9addressTestQqWwee/rent?view=exposure'
		);
		expect(buildRequest('mempool', {})).toBe('/v1/mempool');
	});
	it('builds only curated local API paths and uses strict paired continuation', () => {
		const path = buildRequest('token-history', {
			id: 'a'.repeat(64),
			cursor: '9',
			snapshot: 'bound_token'
		});
		const url = new URL(path, 'https://explorer.example');
		expect(url.pathname).toBe(`/v1/tokens/${'a'.repeat(64)}/txs`);
		expect(Object.fromEntries(url.searchParams)).toEqual({
			dir: 'desc',
			limit: '20',
			cursor: '9',
			snapshot: 'bound_token',
			consistency: 'strict'
		});
		expect(() => buildRequest('https://evil.example', {})).toThrow('supported endpoint');
		expect(() => buildRequest('token-history', { id: 'a'.repeat(64), cursor: '9' })).toThrow(
			'together'
		);
		expect(() => buildRequest('blocks', { snapshot: 'orphan' })).toThrow('together');
	});
	it.each(['0', '101', '-1', '1.5', '20&target=external'])(
		'rejects unsafe page size %s',
		(limit) => {
			expect(() => buildRequest('blocks', { limit })).toThrow('1 through 100');
		}
	);
	it('encodes token queries literally, including shell and URL punctuation', () => {
		const query = "token' ; $(echo bad) &next=x/thing";
		const path = buildRequest('token-search', { q: query });
		expect(new URL(path, 'https://explorer.example').searchParams.get('q')).toBe(query);
		expect(curlCommand('https://explorer.example', path)).not.toContain('$(echo bad)');
		expect(curlCommand('https://explorer.example', "/v1/search?q='")).toContain('q=%27');
	});
	it('validates IDs and heights without treating them as free-form paths', () => {
		expect(() => buildRequest('transaction', { id: '../status' })).toThrow('invalid');
		expect(buildRequest('block', { block: '4294967295' })).toBe('/v1/blocks/4294967295');
		expect(() => buildRequest('block', { block: '4294967296' })).toThrow('invalid');
		expect(() =>
			buildRequest('balance-at', { address: '9addressTestQqWwee', height: '-1' })
		).toThrow('invalid');
		expect(buildRequest('balance-at', { address: '9addressTestQqWwee', height: '0' })).toContain(
			'height=0'
		);
	});
	it('shares only the selected endpoint fields and round-trips Unicode names', () => {
		const values = { q: 'Σ Test', match: 'exact', secret: 'not-a-field' };
		const shared = new URL(sharePath('token-search', values), 'https://explorer.example');
		expect(shared.pathname).toBe('/developers');
		expect(shared.href).not.toContain('secret');
		const restored = readShare(shared.searchParams);
		expect(buildRequest(restored.key, restored.values)).toBe(buildRequest('token-search', values));
		expect(() => readShare(new URLSearchParams('endpoint=unknown'))).toThrow('supported');
		expect(() =>
			readShare(new URLSearchParams({ endpoint: 'token-search', 'p.q': '\n<script>' }))
		).not.toThrow();
		expect(() =>
			readShare(new URLSearchParams({ endpoint: 'token-search', 'p.q': 'a\nb' }))
		).toThrow('invalid');
	});
	it('defines unique endpoint names with bounded field defaults', () => {
		expect(new Set(ENDPOINTS.map((e) => e.id)).size).toBe(ENDPOINTS.length);
		for (const endpoint of ENDPOINTS) {
			const values = defaults(endpoint);
			for (const field of endpoint.fields) {
				if (!field.required || field.initial) continue;
				values[field.name] =
					field.kind === 'id'
						? 'a'.repeat(64)
						: field.kind === 'address'
							? '9addressTestQqWwee'
							: field.kind === 'text'
								? 'test'
								: '1';
			}
			expect(buildRequest(endpoint.id, values)).toMatch(/^\/v1\//);
		}
	});
});

describe('bounded response handling', () => {
	it('preserves exact raw JSON and HTTP problem responses', async () => {
		const raw = '{"amount":"9007199254740993","code":"token_history_preparing"}';
		const fetcher = vi.fn().mockResolvedValue(new Response(raw, { status: 503 }));
		const abort = new AbortController();
		const result = await executeRequest('status', {}, abort.signal, fetcher);
		expect(result).toMatchObject({
			text: raw,
			status: 503,
			ok: false,
			data: { amount: '9007199254740993' }
		});
		expect(fetcher).toHaveBeenCalledWith(
			'/v1/status',
			expect.objectContaining({
				method: 'GET',
				signal: abort.signal,
				credentials: 'omit',
				redirect: 'error'
			})
		);
	});
	it('rejects oversized declared or chunked bodies and cancels the stream', async () => {
		for (const advertised of [false, true]) {
			const cancelled = vi.fn();
			const response = new Response(
				new ReadableStream({
					start(controller) {
						controller.enqueue(new Uint8Array(RESPONSE_LIMIT + 1));
					},
					cancel: cancelled
				}),
				{ headers: advertised ? { 'content-length': String(RESPONSE_LIMIT + 1) } : {} }
			);
			await expect(
				executeRequest(
					'status',
					{},
					new AbortController().signal,
					vi.fn().mockResolvedValue(response)
				)
			).rejects.toThrow('2 MiB');
			expect(cancelled).toHaveBeenCalled();
		}
	});
	it('decodes split Unicode code points and rejects non-JSON gateway pages', async () => {
		const bytes = new TextEncoder().encode('{"name":"Σ"}');
		const response = new Response(
			new ReadableStream({
				start(controller) {
					for (const byte of bytes) controller.enqueue(new Uint8Array([byte]));
					controller.close();
				}
			})
		);
		expect(
			(
				await executeRequest(
					'status',
					{},
					new AbortController().signal,
					vi.fn().mockResolvedValue(response)
				)
			).data
		).toEqual({ name: 'Σ' });
		await expect(
			executeRequest(
				'status',
				{},
				new AbortController().signal,
				vi.fn().mockResolvedValue(new Response('<html>bad gateway</html>', { status: 502 }))
			)
		).rejects.toThrow('HTTP 502');
	});
	it('offers continuation only for complete strict pairs', () => {
		expect(continuation({ consistency: 'strict', next_cursor: '1', next_snapshot: 'a' })).toEqual({
			cursor: '1',
			snapshot: 'a'
		});
		for (const value of [
			null,
			{},
			{ next_cursor: '1', next_snapshot: 'a' },
			{ consistency: 'strict', next_cursor: '1', next_snapshot: null },
			{ consistency: 'strict', next_cursor: '1', next_snapshot: 'a'.repeat(4097) }
		])
			expect(continuation(value)).toBeNull();
	});
});
