import { expect, test, vi } from 'vitest';
import { api } from '$lib/api/endpoints';
import { load } from '../../src/routes/+page';

test('a one-minute chain loads a full day past the old 800-block cap', async () => {
	let offset = 0;
	vi.spyOn(api, 'blocks').mockImplementation(async () => {
		const items = Array.from({ length: 500 }, (_, i) => ({
			timestamp: 2_000_000_000 - (offset + i) * 60000
		}));
		offset += 500;
		return { items, next_cursor: String(offset) } as never;
	});
	vi.spyOn(api, 'rentUpcoming').mockResolvedValue({ items: [], next_cursor: null } as never);
	vi.spyOn(api, 'txs').mockResolvedValue({ items: [], next_cursor: null });
	vi.spyOn(api, 'tokens').mockResolvedValue({ items: [], next_cursor: null });
	vi.spyOn(api, 'richlist').mockResolvedValue({ items: [], next_cursor: null });
	vi.spyOn(api, 'status').mockResolvedValue({} as never);
	const result = await load({ fetch } as never);
	expect(result!.blocks.data!.length).toBe(1500);
	vi.restoreAllMocks();
});
