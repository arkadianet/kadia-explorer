import { api } from '$lib/api/endpoints';
import { createPager } from '$lib/pager/pager.svelte';

export function createTxSummaryPager(f?: typeof fetch) {
	return createPager((cursor, snapshot) => api.txSummaries(cursor, 50, undefined, f, snapshot));
}
