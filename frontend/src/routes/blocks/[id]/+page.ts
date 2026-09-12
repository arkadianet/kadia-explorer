import { error } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		const [block, txs] = await Promise.all([
			api.block(params.id, fetch),
			// Expanded by design: bounded by the M3a work budget rather than pagination;
			// the summary contract has no output total, which this page displays.
			api.blockTxs(params.id, fetch)
		]);
		return { block, txs };
	} catch (e) {
		if (e instanceof ApiError) {
			if (e.status === 404) throw error(404, 'Block not found');
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
