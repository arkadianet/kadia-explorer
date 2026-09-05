import { api } from '$lib/api/endpoints';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	return {
		status: await api.status(fetch)
	};
};
