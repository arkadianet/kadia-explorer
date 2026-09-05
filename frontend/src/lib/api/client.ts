// Thin fetch wrapper for the xp-api server. Every route is mounted under `/v1`, so callers
// pass the route's own path (e.g. `/status`) and this module adds the prefix.

const API_PREFIX = '/v1';
const TIMEOUT_MS = 10_000;

export class ApiError extends Error {
	constructor(
		public status: number,
		public title: string,
		public detail: string
	) {
		super(`${status} ${title}: ${detail}`);
		this.name = 'ApiError';
	}
}

export type QueryParams = Record<string, string | number | boolean | undefined>;

function buildUrl(path: string, params?: QueryParams): string {
	const url = `${API_PREFIX}${path}`;
	if (!params) return url;
	const search = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value === undefined) continue;
		search.set(key, String(value));
	}
	const qs = search.toString();
	return qs ? `${url}?${qs}` : url;
}

interface Problem {
	type?: string;
	title?: string;
	status?: number;
	detail?: string;
}

export async function apiGet<T>(
	path: string,
	params?: QueryParams,
	fetchFn: typeof fetch = fetch
): Promise<T> {
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
	try {
		const res = await fetchFn(buildUrl(path, params), { signal: controller.signal });
		if (!res.ok) {
			let problem: Problem = {};
			try {
				problem = (await res.json()) as Problem;
			} catch {
				// Body wasn't JSON (or empty) — fall back to the status line below.
			}
			throw new ApiError(
				res.status,
				problem.title ?? res.statusText,
				problem.detail ?? res.statusText
			);
		}
		return (await res.json()) as T;
	} finally {
		clearTimeout(timer);
	}
}
