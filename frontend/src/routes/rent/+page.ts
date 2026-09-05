import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type { RentItemDto } from '$lib/api/types';
import type { PageLoad } from './$types';

/** Default N (in blocks) for the Upcoming tab's rent-maturity horizon. */
export const DEFAULT_UPCOMING_BLOCKS = 720;

/**
 * The Upcoming seed is returned as data rather than thrown: the Eligible tab is served by a
 * separate endpoint, so an error here must not replace the whole page (and its tabs) with the
 * generic error page.
 */
export const load: PageLoad = async ({ fetch }) => {
	try {
		const page = await api.rentUpcoming(DEFAULT_UPCOMING_BLOCKS, 100, fetch);
		return { upcoming: page.items, error: null as unknown };
	} catch (e) {
		if (e instanceof ApiError) return { upcoming: [] as RentItemDto[], error: e as unknown };
		throw e;
	}
};
