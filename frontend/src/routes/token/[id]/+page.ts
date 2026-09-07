import { error } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		return { id: params.id, token: await api.token(params.id, fetch) };
	} catch (e) {
		if (e instanceof ApiError) {
			// A 404 is not an error page: the id may simply not be a token, or the indexer may
			// not have reached the block that minted it. The page renders a not-found state.
			if (e.status === 404) return { id: params.id, token: null };
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
