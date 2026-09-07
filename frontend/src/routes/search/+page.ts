import { error, redirect } from '@sveltejs/kit';
import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import { classify, routeFor } from '$lib/search/classify';
import { parseRegisterQuery } from '$lib/registers/search';
import type { PageLoad } from './$types';

export type NotFoundReason = 'empty' | 'unknown-format' | 'not-found';

function destinationFor(
	kind: 'block' | 'tx' | 'box' | 'address' | 'token' | 'template',
	id: string
): string {
	switch (kind) {
		case 'block':
			return `/blocks/${id}`;
		case 'tx':
			return `/tx/${id}`;
		case 'box':
			return `/box/${id}`;
		case 'address':
			return `/address/${id}`;
		case 'token':
			return `/token/${id}`;
		case 'template':
			return `/template/${id}`;
	}
}

export const load: PageLoad = async ({ url, fetch }) => {
	const q = url.searchParams.get('q') ?? '';
	const trimmed = q.trim();

	// The register lookup lives in the query string beside `q`, so a result list survives a
	// reload and can be linked to. It is read on every path: a lookup and a failed `q` search
	// can be on screen at the same time.
	const registers = parseRegisterQuery(url.searchParams.get('reg'), url.searchParams.get('value'));

	if (!trimmed) return { q, reason: 'empty' as NotFoundReason, registers };

	const classified = classify(trimmed);
	const direct = routeFor(classified);
	if (direct) throw redirect(302, direct);

	if (classified.kind !== 'hex32') {
		return { q, reason: 'unknown-format' as NotFoundReason, registers };
	}

	// hex32 — ambiguous between block/tx/box, ask the API.
	try {
		const result = await api.search(classified.value, fetch);
		throw redirect(302, destinationFor(result.kind, result.id));
	} catch (e) {
		if (e instanceof ApiError) {
			if (e.status === 404) return { q, reason: 'not-found' as NotFoundReason, registers };
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
