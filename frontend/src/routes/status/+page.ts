import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type { StatusDto } from '$lib/api/types';
import type { PageLoad } from './$types';

/**
 * A failing `/v1/status` is exactly what this page exists to report, so it renders the
 * problem detail in place instead of throwing to the generic error page.
 */
export const load: PageLoad = async ({ fetch }) => {
	try {
		return { status: await api.status(fetch), error: null as unknown };
	} catch (e) {
		if (e instanceof ApiError) return { status: null as StatusDto | null, error: e as unknown };
		throw e;
	}
};
