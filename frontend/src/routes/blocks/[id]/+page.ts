import { error } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import { blockRewards } from '$lib/blocks/rewards';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		const block = await api.block(params.id, fetch);
		const rewards = await blockRewards(block.id, fetch).then(
			(data) => ({ data, error: null }),
			(error) => ({ data: null, error })
		);
		return { block, rewards };
	} catch (e) {
		if (e instanceof ApiError) {
			if (e.status === 404) throw error(404, 'Block not found');
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
