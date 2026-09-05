import { api } from '$lib/api/endpoints';
import type { BlockDto, PageDto, RentItemDto, StatusDto, TxDto } from '$lib/api/types';
import type { PageLoad } from './$types';

export interface Loaded<T> {
	data: T | null;
	error: unknown;
}

async function safe<T>(p: Promise<T>): Promise<Loaded<T>> {
	try {
		return { data: await p, error: null };
	} catch (error) {
		return { data: null, error };
	}
}

export const load: PageLoad = async ({ fetch }) => {
	// Each call is wrapped so one panel's failure can't reject the whole `Promise.all` and
	// take down the page — every panel gets its own success/error result to render from.
	const [blocks, txs, rent, status] = await Promise.all([
		safe<PageDto<BlockDto>>(api.blocks(undefined, 10, undefined, fetch)),
		safe<PageDto<TxDto>>(api.txs(undefined, 10, undefined, fetch)),
		safe<RentItemDto[]>(api.rentUpcoming(720, 5, fetch)),
		safe<StatusDto>(api.status(fetch))
	]);

	return { blocks, txs, rent, status };
};
