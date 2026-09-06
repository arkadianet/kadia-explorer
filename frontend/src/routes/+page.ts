import { api } from '$lib/api/endpoints';
import type {
	BlockDto,
	PageDto,
	RentItemDto,
	RichlistItemDto,
	StatusDto,
	TxDto
} from '$lib/api/types';
import type { PageLoad } from './$types';

/** Largest page `/v1/blocks` will serve. */
const PAGE = 500;
/** Enough blocks to cover a day at Ergo's two-minute target, with headroom for fast runs. */
const WANT_BLOCKS = 800;
/** The home page's figures are all "over the last 24 hours of the indexed chain". */
const DAY_MS = 86_400_000;

/**
 * Walks the `/v1/blocks` cursor until a day of chain is in hand (or the chain runs out).
 * The 24 h statistics on the home page are sums over exactly these blocks, so the window has
 * to be fetched rather than estimated.
 */
async function blockWindow(fetch: typeof globalThis.fetch): Promise<BlockDto[]> {
	const items: BlockDto[] = [];
	let cursor: string | undefined;
	for (let round = 0; round < 6; round++) {
		const page = await api.blocks(cursor, PAGE, undefined, fetch);
		items.push(...page.items);
		const newest = items[0]?.timestamp ?? 0;
		const oldest = items[items.length - 1]?.timestamp ?? 0;
		if (
			page.next_cursor === null ||
			items.length >= WANT_BLOCKS ||
			newest - oldest >= DAY_MS ||
			page.items.length === 0
		) {
			break;
		}
		cursor = page.next_cursor;
	}
	return items;
}

async function upcomingRentItems(fetch: typeof globalThis.fetch): Promise<RentItemDto[]> {
	const page = await api.rentUpcoming(720, 500, fetch);
	return page.items;
}

async function topHolders(fetch: typeof globalThis.fetch): Promise<RichlistItemDto[]> {
	const page = await api.richlist(undefined, 5, fetch);
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
	const [blocks, txs, rent, richlist, status] = await Promise.all([
		safe<BlockDto[]>(blockWindow(fetch)),
		safe<PageDto<TxDto>>(api.txs(undefined, 12, undefined, fetch)),
		safe<RentItemDto[]>(upcomingRentItems(fetch)),
		safe<RichlistItemDto[]>(topHolders(fetch)),
		safe<StatusDto>(api.status(fetch))
	]);

	return { blocks, txs, rent, richlist, status };
};
