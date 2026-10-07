import { api } from '$lib/api/endpoints';
import { scheduleIssue, scheduleQuery, validateSchedule } from '$lib/rent/schedule';
import type { RentSchedulePageDto } from '$lib/api/types';
import type { PageLoad } from './$types';

/** Keep schedule failures local: the separately paged Eligible view remains available. */
export const load: PageLoad = async ({ fetch, url }) => {
	const { query, error } = scheduleQuery(url.searchParams);
	if (error) return { query, schedule: null as RentSchedulePageDto | null, error };
	try {
		const schedule = validateSchedule(await api.rentSchedule(query, null, null, 100, fetch), query);
		return {
			query,
			schedule: schedule as RentSchedulePageDto | null,
			error: null as string | null
		};
	} catch (cause) {
		return { query, schedule: null as RentSchedulePageDto | null, error: scheduleIssue(cause) };
	}
};
