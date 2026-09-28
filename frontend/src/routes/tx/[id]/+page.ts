import { error } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, fetch }) => {
	if (!/^[0-9a-fA-F]{64}$/.test(params.id))
		throw error(400, 'A transaction ID must contain 64 hexadecimal characters');
	const id = params.id.toLowerCase();
	try {
		return { id, tx: await api.tx(id, fetch), tracking: null };
	} catch (e) {
		if (e instanceof ApiError) {
			if (e.status === 404) {
				// A freshly broadcast transaction may not have reached our node or index.
				// Keep a valid ID watchable even if its first status request fails.
				try {
					return { id, tx: null, tracking: await api.txStatus(id, fetch) };
				} catch {
					return { id, tx: null, tracking: null };
				}
			}
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
