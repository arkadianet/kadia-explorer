import type { PageLoad } from './$types';

// The list is populated client-side by `InfiniteList`/`createPager` on mount — no server
// fetch needed here. An explicit (empty) load keeps the route's `PageData` typing consistent
// with the rest of the app.
export const load: PageLoad = async () => {
	return {};
};
