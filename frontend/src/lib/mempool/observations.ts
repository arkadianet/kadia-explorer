const ID = /^[a-f0-9]{64}$/;
const U128_MAX = (1n << 128n) - 1n;
export const MEMPOOL_LIMIT = 100;

export interface PendingTransaction {
	id: string;
	input_count: number;
	data_input_count: number;
	output_count: number;
	size: number | null;
	fee: string | null;
}
export interface MempoolSnapshot {
	scope: 'configured_node_mempool';
	source: 'configured_primary_node';
	checked_at_ms: number;
	expires_at_ms: number;
	cached: boolean;
	limit: 100;
	limit_reached: boolean;
	observed_count: number;
	items: PendingTransaction[];
}
export interface MempoolProblem {
	code: string;
	message: string;
}
export interface MempoolState {
	loading: boolean;
	snapshot: MempoolSnapshot | null;
	error: MempoolProblem | null;
}

export class MempoolError extends Error {
	constructor(
		public code: string,
		message: string
	) {
		super(message);
		this.name = 'MempoolError';
	}
}
const invalid = () => {
	throw new MempoolError(
		'mempool_invalid_response',
		'The node observation did not meet the expected data limits.'
	);
};
const integer = (value: unknown, minimum = 0, maximum = Number.MAX_SAFE_INTEGER): value is number =>
	typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum && value <= maximum;

export function validateSnapshot(value: unknown): MempoolSnapshot {
	if (!value || typeof value !== 'object') return invalid();
	const data = value as MempoolSnapshot;
	if (
		data.scope !== 'configured_node_mempool' ||
		data.source !== 'configured_primary_node' ||
		!integer(data.checked_at_ms, 0, 8_640_000_000_000_000) ||
		!integer(data.expires_at_ms, data.checked_at_ms, 8_640_000_000_000_000) ||
		data.expires_at_ms - data.checked_at_ms > 60_000 ||
		typeof data.cached !== 'boolean' ||
		typeof data.limit_reached !== 'boolean' ||
		data.limit !== MEMPOOL_LIMIT ||
		!Array.isArray(data.items) ||
		data.items.length > MEMPOOL_LIMIT ||
		data.observed_count !== data.items.length ||
		data.limit_reached !== (data.items.length === MEMPOOL_LIMIT)
	)
		return invalid();
	const seen = new Set<string>();
	for (const item of data.items) {
		if (
			!item ||
			typeof item.id !== 'string' ||
			!ID.test(item.id) ||
			seen.has(item.id) ||
			!integer(item.input_count, 1, 512) ||
			!integer(item.data_input_count, 0, 512) ||
			!integer(item.output_count, 1, 4096) ||
			(item.size !== null && !integer(item.size, 1, 0xffff_ffff)) ||
			(item.fee !== null &&
				(typeof item.fee !== 'string' ||
					!/^(0|[1-9][0-9]{0,38})$/.test(item.fee) ||
					BigInt(item.fee) > U128_MAX))
		)
			return invalid();
		seen.add(item.id);
	}
	return data;
}

export function filterPending(items: PendingTransaction[], query: string): PendingTransaction[] {
	const normalized = query.trim().toLowerCase();
	return normalized ? items.filter((item) => item.id.includes(normalized)) : items;
}

export async function fetchMempool(signal: AbortSignal): Promise<unknown> {
	const response = await fetch('/v1/mempool', { signal, credentials: 'omit', redirect: 'error' });
	if (!response.body)
		throw new MempoolError('mempool_unavailable', 'The observation returned no response body.');
	const reader = response.body.getReader();
	const chunks: Uint8Array[] = [];
	let size = 0;
	try {
		while (true) {
			const part = await reader.read();
			if (part.done) break;
			size += part.value.byteLength;
			if (size > 256 * 1024)
				throw new MempoolError(
					'mempool_invalid_response',
					'The observation exceeded the response limit.'
				);
			chunks.push(part.value);
		}
	} finally {
		await reader.cancel().catch(() => undefined);
		reader.releaseLock();
	}
	const bytes = new Uint8Array(size);
	let offset = 0;
	for (const chunk of chunks) {
		bytes.set(chunk, offset);
		offset += chunk.length;
	}
	let data: unknown;
	try {
		data = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
	} catch {
		throw new MempoolError('mempool_invalid_response', 'The observation was not valid JSON.');
	}
	if (!response.ok) {
		const problem = data && typeof data === 'object' ? (data as Record<string, unknown>) : {};
		throw new MempoolError(
			typeof problem.code === 'string' && /^mempool_[a-z_]{1,40}$/.test(problem.code)
				? problem.code
				: 'mempool_unavailable',
			typeof problem.detail === 'string'
				? problem.detail.slice(0, 512)
				: `The node could not be checked (HTTP ${response.status}).`
		);
	}
	return data;
}

export function createMempoolLoader(
	onChange: (state: MempoolState) => void,
	fetcher = fetchMempool
) {
	let active: AbortController | null = null;
	let stopped = false;
	let generation = 0;
	async function refresh() {
		if (stopped || active) return;
		const request = ++generation;
		const controller = new AbortController();
		active = controller;
		onChange({ loading: true, snapshot: null, error: null });
		const timer = setTimeout(() => controller.abort(), 10_000);
		try {
			const response = await fetcher(controller.signal);
			if (stopped || request !== generation || controller.signal.aborted) return;
			onChange({ loading: false, snapshot: validateSnapshot(response), error: null });
		} catch (error) {
			if (stopped || request !== generation) return;
			onChange({
				loading: false,
				snapshot: null,
				error: {
					code: controller.signal.aborted
						? 'mempool_unavailable'
						: error instanceof MempoolError
							? error.code
							: 'mempool_unavailable',
					message: controller.signal.aborted
						? 'The node check timed out. Refresh to try again.'
						: error instanceof Error
							? error.message
							: 'The node could not be checked.'
				}
			});
		} finally {
			clearTimeout(timer);
			if (!stopped && request === generation) {
				active = null;
				if (controller.signal.aborted)
					onChange({
						loading: false,
						snapshot: null,
						error: {
							code: 'mempool_unavailable',
							message: 'The node check timed out. Refresh to try again.'
						}
					});
			}
		}
	}
	return {
		refresh,
		stop() {
			stopped = true;
			generation++;
			active?.abort();
			active = null;
		}
	};
}
