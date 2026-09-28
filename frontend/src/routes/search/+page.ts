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
	const tokenQuery = q.replace(/^[\t\n\v\f\r ]+|[\t\n\v\f\r ]+$/g, '');
	const match =
		url.searchParams.get('match') === 'exact' ? ('exact' as const) : ('prefix' as const);

	// The register lookup lives in the query string beside `q`, so a result list survives a
	// reload and can be linked to. It is read on every path: a lookup and a failed `q` search
	// can be on screen at the same time.
	const registers = parseRegisterQuery(url.searchParams.get('reg'), url.searchParams.get('value'));

	if (!tokenQuery) return { q, reason: 'empty' as NotFoundReason, registers };

	const classified = classify(trimmed);
	const direct = routeFor(classified);
	if (direct) throw redirect(302, direct);

	if (classified.kind !== 'hex32' && classified.kind !== 'address') {
		return { q, reason: 'empty' as NotFoundReason, registers, tokenQuery, match };
	}

	// Resolve IDs and validate address candidates on the server.
	try {
		const result = await api.search(classified.value, fetch);
		if (result.matches && result.matches.length > 1) {
			return {
				q,
				reason: 'empty' as NotFoundReason,
				registers,
				matches: result.matches.map((match) => ({
					...match,
					href: destinationFor(match.kind, match.id)
				}))
			};
		}
		throw redirect(302, destinationFor(result.kind, result.id));
	} catch (e) {
		if (e instanceof ApiError) {
			if (e.status === 400)
				return {
					q,
					reason: 'unknown-format' as NotFoundReason,
					registers,
					tokenQuery,
					match
				};
			if (e.status === 404) {
				if (classified.kind === 'hex32') {
					let pending = false;
					try {
						pending = (await api.txStatus(classified.value, fetch)).state === 'pending';
					} catch {
						/* Search still offers an explicit tracking link during an outage. */
					}
					if (pending) throw redirect(302, `/tx/${classified.value}`);
					return { q, reason: 'not-found' as NotFoundReason, registers, trackId: classified.value };
				}
				return { q, reason: 'not-found' as NotFoundReason, registers };
			}
			throw error(e.status, e.detail);
		}
		throw e;
	}
};
