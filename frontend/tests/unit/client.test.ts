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
