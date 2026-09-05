import { error } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		return { addr: params.addr, info: await api.address(params.addr, fetch) };
	} catch (e) {
		if (e instanceof ApiError) {
			// A 404 is not an error page: the address is simply not in the index yet (never
			// funded, or not reached by the indexer). The page renders a "not seen yet" state.
			if (e.status === 404) return { addr: params.addr, info: null };
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
