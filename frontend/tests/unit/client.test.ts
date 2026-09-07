import { describe, it, expect, vi } from 'vitest';
import { apiGet, ApiError } from '$lib/api/client';
import { api } from '$lib/api/endpoints';

function jsonResponse(body: unknown, status = 200, statusText = 'OK') {
	return new Response(JSON.stringify(body), {
		status,
		statusText,
		headers: { 'content-type': 'application/json' }
	});
}

describe('apiGet', () => {
	it('rejects with ApiError on a 404 problem response', async () => {
		const fetchFn = vi.fn().mockResolvedValue(
			jsonResponse(
				{
					type: 'about:blank',
					title: 'Not Found',
					status: 404,
					detail: 'the requested resource does not exist'
				},
				404,
				'Not Found'
			)
		);
		await expect(apiGet('/boxes/nope', undefined, fetchFn)).rejects.toMatchObject({
			status: 404,
			title: 'Not Found',
			detail: 'the requested resource does not exist'
		});
		await expect(apiGet('/boxes/nope', undefined, fetchFn)).rejects.toBeInstanceOf(ApiError);
	});

	it('resolves with the parsed body on 200', async () => {
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse({ best: 42 }));
		const result = await apiGet<{ best: number }>('/status', undefined, fetchFn);
		expect(result).toEqual({ best: 42 });
	});

	it('serialises query params and skips undefined ones', async () => {
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse({ items: [] }));
		await apiGet('/blocks', { cursor: '5', limit: 50, dir: undefined }, fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toContain('/v1/blocks?');
		expect(calledUrl).toContain('cursor=5');
		expect(calledUrl).toContain('limit=50');
		expect(calledUrl).not.toContain('dir=');
	});
});

describe('api path encoding', () => {
	it('encodes caller-supplied path segments', async () => {
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse({}));
		await api.address('a/b c', fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toContain('a%2Fb%20c');
	});
});

describe('api.rentUpcoming', () => {
	it('requests /v1/rent/upcoming with blocks and limit, and returns the page unchanged', async () => {
		const body = { items: [], next_cursor: null };
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse(body));
		const result = await api.rentUpcoming(720, 50, fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toBe('/v1/rent/upcoming?blocks=720&limit=50');
		expect(result).toEqual(body);
	});
});

describe('api.tokens', () => {
	it('requests /v1/tokens with sort, cursor and limit', async () => {
		const body = { items: [], next_cursor: null };
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse(body));
		const result = await api.tokens('holders', 'c1', 20, fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toBe('/v1/tokens?sort=holders&cursor=c1&limit=20');
		expect(result).toEqual(body);
	});
});

describe('api.token', () => {
	it('requests /v1/tokens/{id}', async () => {
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse({ id: 'abc' }));
		await api.token('abc', fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toBe('/v1/tokens/abc');
	});
});

describe('api.tokenHolders', () => {
	it('requests /v1/tokens/{id}/holders with cursor and limit, no dir', async () => {
		const body = { items: [], next_cursor: null };
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse(body));
		const result = await api.tokenHolders('abc', 'c1', 20, fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toBe('/v1/tokens/abc/holders?cursor=c1&limit=20');
		expect(result).toEqual(body);
	});
});

describe('api.tokenBoxes', () => {
	it('requests /v1/tokens/{id}/boxes with unspent, cursor, limit and dir', async () => {
		const body = { items: [], next_cursor: null };
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse(body));
		const result = await api.tokenBoxes('abc', true, 'c1', 20, 'asc', fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toBe('/v1/tokens/abc/boxes?unspent=true&cursor=c1&limit=20&dir=asc');
		expect(result).toEqual(body);
	});
});

describe('api.template', () => {
	it('requests /v1/templates/{hash}', async () => {
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse({ hash: 'abc' }));
		await api.template('abc', fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toBe('/v1/templates/abc');
	});
});

describe('api.templateBoxes', () => {
	it('requests /v1/templates/{hash}/boxes with unspent, cursor, limit and dir', async () => {
		const body = { items: [], next_cursor: null };
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse(body));
		const result = await api.templateBoxes('abc', false, 'c1', 20, 'desc', fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toBe('/v1/templates/abc/boxes?unspent=false&cursor=c1&limit=20&dir=desc');
		expect(result).toEqual(body);
	});
});

describe('api.boxesByRegister', () => {
	it('requests /v1/registers/{reg}/{valueHex}/boxes with cursor, limit and dir', async () => {
		const body = { items: [], next_cursor: null };
		const fetchFn = vi.fn().mockResolvedValue(jsonResponse(body));
		const result = await api.boxesByRegister('R4', 'deadbeef', 'c1', 20, 'asc', fetchFn);
		const calledUrl = fetchFn.mock.calls[0][0] as string;
		expect(calledUrl).toBe('/v1/registers/R4/deadbeef/boxes?cursor=c1&limit=20&dir=asc');
		expect(result).toEqual(body);
	});
});
