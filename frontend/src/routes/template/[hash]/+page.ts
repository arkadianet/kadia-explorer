import { error } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		return { hash: params.hash, template: await api.template(params.hash, fetch) };
	} catch (e) {
		if (e instanceof ApiError) {
			// A 404 is not an error page: the hash may simply not be a template the indexer has
			// seen a box for yet. The page renders a not-found state.
			if (e.status === 404) return { hash: params.hash, template: null };
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
