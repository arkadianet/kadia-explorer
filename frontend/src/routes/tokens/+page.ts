import type { PageLoad } from './$types';

export type TokenSort = 'newest' | 'holders';

// The rows themselves are pulled client-side by `InfiniteList`/`createPager`; this load only
// reads the sort out of the query string, so the choice survives a reload and is shareable.
// Reading `url.searchParams` here is what makes SvelteKit re-run the load when it changes.
export const load: PageLoad = ({ url }) => {
	const sort: TokenSort = url.searchParams.get('sort') === 'holders' ? 'holders' : 'newest';
	return { sort };
};
