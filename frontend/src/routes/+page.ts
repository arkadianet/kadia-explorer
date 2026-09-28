import { api } from '$lib/api/endpoints';
import type {
	PageDto,
	RichlistItemDto,
	StatusDto,
	TokenInfoDto,
	TxSummaryDto
} from '$lib/api/types';
import { networkSummary } from '$lib/home/api';
import type { PageLoad } from './$types';

async function topHolders(fetch: typeof globalThis.fetch): Promise<RichlistItemDto[]> {
	const page = await api.richlist(undefined, 5, fetch);
	return page.items;
}

/** The five tokens held by the most addresses — "what is actually in circulation here",
 * which is the question the richlist answers for ERG. */
async function topTokens(fetch: typeof globalThis.fetch): Promise<TokenInfoDto[]> {
	const page = await api.tokens('holders', undefined, 5, fetch);
	return page.items;
}

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
	// Each call is wrapped so one section's failure can't reject the whole `Promise.all` and
	// take down the page — every section gets its own success/error result to render from.
	const [blocks, txs, rent, tokens, richlist, status] = await Promise.all([
		safe(networkSummary(fetch)),
		// Compact rows keep the overview independent of transaction expansion budgets.
		safe<PageDto<TxSummaryDto>>(api.txSummaries(undefined, 6, undefined, fetch)),
		safe(api.rentUpcoming(720, 5, fetch)),
		safe<TokenInfoDto[]>(topTokens(fetch)),
		safe<RichlistItemDto[]>(topHolders(fetch)),
		safe<StatusDto>(api.status(fetch))
	]);

	return { blocks, txs, rent, tokens, richlist, status };
};
