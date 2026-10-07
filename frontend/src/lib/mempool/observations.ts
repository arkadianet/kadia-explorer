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
export interface PendingConnection {
	producer_id: string;
	consumer_id: string;
	box_id: string;
	kind: 'spend' | 'read';
}
export interface SharedPendingInput {
	box_id: string;
	transaction_count: number;
	transaction_ids: string[];
	truncated: boolean;
}
export interface PendingConnections {
	scope: 'returned_snapshot_only';
	output_count: number;
	identified_output_count: number;
	edge_count: number;
	edges_truncated: boolean;
	edges: PendingConnection[];
	shared_input_count: number;
	shared_inputs_truncated: boolean;
	shared_inputs: SharedPendingInput[];
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
	/** Absent on older API versions; absence is not evidence of zero connections. */
	connections?: PendingConnections;
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
	if (data.connections !== undefined) validateConnections(data.connections, data.items);
	return data;
}

function validateConnections(value: PendingConnections, items: PendingTransaction[]) {
	const rows = new Map(items.map((item) => [item.id, item]));
	const outputs = items.reduce((sum, item) => sum + item.output_count, 0);
	const inputs = items.reduce((sum, item) => sum + item.input_count, 0);
	const references = inputs + items.reduce((sum, item) => sum + item.data_input_count, 0);
	if (
		!value ||
		value.scope !== 'returned_snapshot_only' ||
		value.output_count !== outputs ||
		!integer(value.identified_output_count, 0, outputs) ||
		!integer(value.edge_count, 0, Math.min(10_000, references)) ||
		!Array.isArray(value.edges) ||
		value.edges.length > 256 ||
		value.edges.length > value.edge_count ||
		value.edges_truncated !== value.edges.length < value.edge_count ||
		!integer(value.shared_input_count, 0, Math.min(5_000, Math.floor(inputs / 2))) ||
		!Array.isArray(value.shared_inputs) ||
		value.shared_inputs.length > 16 ||
		value.shared_inputs.length > value.shared_input_count
	)
		return invalid();
	const edgeKeys = new Set<string>();
	const producers = new Map<string, string>();
	const outputIds = new Map<string, Set<string>>();
	const consumerCounts = new Map<string, number>();
	for (const edge of value.edges) {
		if (
			!edge ||
			!rows.has(edge.producer_id) ||
			!rows.has(edge.consumer_id) ||
			edge.producer_id === edge.consumer_id ||
			typeof edge.box_id !== 'string' ||
			!ID.test(edge.box_id) ||
			(edge.kind !== 'spend' && edge.kind !== 'read')
		)
			return invalid();
		const key = `${edge.producer_id}:${edge.consumer_id}:${edge.box_id}:${edge.kind}`;
		if (
			edgeKeys.has(key) ||
			(producers.has(edge.box_id) && producers.get(edge.box_id) !== edge.producer_id)
		)
			return invalid();
		edgeKeys.add(key);
		producers.set(edge.box_id, edge.producer_id);
		const produced = outputIds.get(edge.producer_id) ?? new Set<string>();
		produced.add(edge.box_id);
		outputIds.set(edge.producer_id, produced);
		if (produced.size > rows.get(edge.producer_id)!.output_count) return invalid();
		const consumerKey = `${edge.consumer_id}:${edge.kind}`;
		const count = (consumerCounts.get(consumerKey) ?? 0) + 1;
		consumerCounts.set(consumerKey, count);
		const row = rows.get(edge.consumer_id)!;
		if (count > (edge.kind === 'spend' ? row.input_count : row.data_input_count)) return invalid();
	}
	if (producers.size > value.identified_output_count) return invalid();
	const boxes = new Set<string>();
	let memberships = 0;
	for (const group of value.shared_inputs) {
		if (
			!group ||
			typeof group.box_id !== 'string' ||
			!ID.test(group.box_id) ||
			boxes.has(group.box_id) ||
			!integer(group.transaction_count, 2, items.length) ||
			!Array.isArray(group.transaction_ids) ||
			group.transaction_ids.length < 2 ||
			group.transaction_ids.length > group.transaction_count ||
			group.truncated !== group.transaction_ids.length < group.transaction_count ||
			new Set(group.transaction_ids).size !== group.transaction_ids.length ||
			group.transaction_ids.some((id) => !rows.has(id))
		)
			return invalid();
		boxes.add(group.box_id);
		memberships += group.transaction_ids.length;
		if (memberships > 256) return invalid();
	}
	if (
		value.shared_inputs_truncated !==
		(value.shared_inputs.length < value.shared_input_count ||
			value.shared_inputs.some((group) => group.truncated))
	)
		return invalid();
}

export function parseMempoolFocus(value: string | null): string | null {
	return value !== null && ID.test(value) ? value : null;
}

/** Select only already returned references. This function never resolves missing boxes. */
export function connectionsFor(snapshot: MempoolSnapshot, id: string) {
	return {
		transaction: snapshot.items.find((item) => item.id === id) ?? null,
		incoming: snapshot.connections?.edges.filter((edge) => edge.consumer_id === id) ?? [],
		outgoing: snapshot.connections?.edges.filter((edge) => edge.producer_id === id) ?? [],
		shared:
			snapshot.connections?.shared_inputs.filter((group) => group.transaction_ids.includes(id)) ??
			[]
	};
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
