import { afterEach, expect, test, vi } from 'vitest';
import { api } from '$lib/api/endpoints';
import * as overview from '$lib/home/api';
import { load } from '../../src/routes/+page';

afterEach(() => vi.restoreAllMocks());
function otherSections() {
	vi.spyOn(api, 'rentUpcoming').mockResolvedValue({ items: [], complete: false } as never);
	vi.spyOn(api, 'txSummaries').mockResolvedValue({ items: [], next_cursor: null });
	vi.spyOn(api, 'tokens').mockResolvedValue({ items: [], next_cursor: null });
	vi.spyOn(api, 'richlist').mockResolvedValue({ items: [], next_cursor: null });
	vi.spyOn(api, 'status').mockResolvedValue({ indexed: 100 } as never);
}
test('home uses one compact network request and bounded previews', async () => {
	otherSections();
	const summary = { block_count: 720, fees: '9007199254740993', recent_blocks: [] };
	const getSummary = vi.spyOn(overview, 'networkSummary').mockResolvedValue(summary as never);
	const expanded = vi.spyOn(api, 'txs');
	const blocks = vi.spyOn(api, 'blocks');
	const result = await load({ fetch } as never);
	expect(result!.blocks.data).toEqual(summary);
	expect(getSummary).toHaveBeenCalledOnce();
	expect(api.txSummaries).toHaveBeenCalledWith(undefined, 6, undefined, fetch);
	expect(api.rentUpcoming).toHaveBeenCalledWith(720, 5, fetch);
	expect(expanded).not.toHaveBeenCalled();
	expect(blocks).not.toHaveBeenCalled();
});
test('a failed overview leaves independently successful sections usable', async () => {
	otherSections();
	vi.spyOn(overview, 'networkSummary').mockRejectedValue(new Error('offline'));
	const result = await load({ fetch } as never);
	expect(result!.blocks.data).toBeNull();
	expect(result!.blocks.error).toBeInstanceOf(Error);
	expect(result!.txs.data?.items).toEqual([]);
	expect(result!.status.data?.indexed).toBe(100);
});
