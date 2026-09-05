import { error, redirect } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import { classify, routeFor } from '$lib/search/classify';
import type { PageLoad } from './$types';

export type NotFoundReason = 'empty' | 'unknown-format' | 'not-found';

function destinationFor(kind: 'block' | 'tx' | 'box' | 'address', id: string): string {
	switch (kind) {
		case 'block':
			return `/blocks/${id}`;
		case 'tx':
			return `/tx/${id}`;
		case 'box':
			return `/box/${id}`;
		case 'address':
			return `/address/${id}`;
	}
}

export const load: PageLoad = async ({ url, fetch }) => {
	const q = url.searchParams.get('q') ?? '';
	const trimmed = q.trim();

	if (!trimmed) return { q, reason: 'empty' as NotFoundReason };

	const classified = classify(trimmed);
	const direct = routeFor(classified);
	if (direct) throw redirect(302, direct);

	if (classified.kind !== 'hex32') {
		return { q, reason: 'unknown-format' as NotFoundReason };
	}

	// hex32 — ambiguous between block/tx/box, ask the API.
	try {
		const result = await api.search(classified.value, fetch);
		throw redirect(302, destinationFor(result.kind, result.id));
	} catch (e) {
		if (e instanceof ApiError) {
			if (e.status === 404) return { q, reason: 'not-found' as NotFoundReason };
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
