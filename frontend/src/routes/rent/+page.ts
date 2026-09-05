import { api } from '$lib/api/endpoints';
import type { PageLoad } from './$types';

/** Default N (in blocks) for the Upcoming tab's rent-maturity horizon. */
export const DEFAULT_UPCOMING_BLOCKS = 720;

export const load: PageLoad = async ({ fetch }) => {
	const page = await api.rentUpcoming(DEFAULT_UPCOMING_BLOCKS, 100, fetch);
	return { upcoming: page.items };
};
