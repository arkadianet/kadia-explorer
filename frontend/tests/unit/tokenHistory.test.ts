import { afterEach, describe, expect, it, vi } from 'vitest';
import { tokenHistory } from '$lib/token/history';

const page = () => ({
	items: [],
	next_cursor: null,
	next_snapshot: null,
	consistency: 'strict',
	anchor: { height: 10, block_id: 'a'.repeat(64) },
	history_context: { scope: 'indexed_token_touches', partial_from: null }
});
afterEach(() => vi.unstubAllGlobals());

describe('token history contract', () => {
	it('requests only strict bounded summaries and preserves the continuation pair', async () => {
		const mock = vi.fn().mockResolvedValue(new Response(JSON.stringify(page())));
		vi.stubGlobal('fetch', mock);
		await tokenHistory('ab'.repeat(32), 'cursor', 'snapshot');
		const url = new URL(mock.mock.calls[0][0], 'https://explorer.example');
		expect(url.pathname).toBe(`/v1/tokens/${'ab'.repeat(32)}/txs`);
		expect(Object.fromEntries(url.searchParams)).toEqual({
			limit: '20',
			dir: 'desc',
			consistency: 'strict',
			cursor: 'cursor',
			snapshot: 'snapshot'
		});
	});
	it.each([
		{ consistency: 'best_effort' },
		{ anchor: null },
		{ history_context: undefined },
		{ history_context: { scope: 'indexed_token_touches', partial_from: -1 } },
		{ next_cursor: 'cursor', next_snapshot: null }
	])('rejects incomplete snapshot evidence %j', async (override) => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(new Response(JSON.stringify({ ...page(), ...override })))
		);
		await expect(tokenHistory('ab'.repeat(32))).rejects.toThrow(
			'consistent token history snapshot'
		);
	});
	it('keeps preparing distinct from an empty successful history', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(
				new Response(JSON.stringify({ code: 'token_history_preparing', title: 'Preparing' }), {
					status: 503
				})
			)
		);
		await expect(tokenHistory('ab'.repeat(32))).rejects.toMatchObject({
			status: 503,
			code: 'token_history_preparing',
			title: 'Token history is preparing'
		});
	});
	it('preserves a reorg conflict for the pager restart latch', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(
				new Response(JSON.stringify({ code: 'snapshot_invalidated', title: 'Changed' }), {
					status: 409
				})
			)
		);
		await expect(tokenHistory('ab'.repeat(32))).rejects.toMatchObject({
			status: 409,
			code: 'snapshot_invalidated'
		});
	});
});
