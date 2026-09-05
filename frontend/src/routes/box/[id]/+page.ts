import { error } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		return { box: await api.box(params.id, fetch) };
	} catch (e) {
		if (e instanceof ApiError) {
			if (e.status === 404) throw error(404, 'Box not found');
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
