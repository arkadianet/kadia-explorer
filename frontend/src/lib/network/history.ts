export const MAX_BLOCKS = 20_160;
export const MAX_BUCKETS = 120;
const ID = /^[a-f0-9]{64}$/;
const U128_MAX = (1n << 128n) - 1n;
export interface NetworkBucket {
	from_height: number;
	to_height: number;
	block_count: number;
	transaction_count: string;
	fees: string;
	size_bytes: string;
	difficulty_min: string;
	difficulty_max: string;
	difficulty_end: string;
	first_timestamp: number;
	last_timestamp: number;
	earliest_timestamp: number;
	latest_timestamp: number;
}
export interface NetworkHistory {
	scope: 'canonical_block_headers';
	consistency: 'single_reader';
	complete: true;
	full_history: boolean;
	partial_from: number | null;
	indexed_height: number;
	anchor: { height: number; block_id: string };
	requested_buckets: number;
	bucket_width: number;
	totals: NetworkBucket;
	buckets: NetworkBucket[];
}
export interface NetworkRange {
	from_height: number;
	to_height: number;
	buckets: number;
	end_block_id?: string;
}
export type NetworkMetric = 'transaction_count' | 'fees' | 'size_bytes' | 'difficulty_end';
export function range(from: string, to: string, buckets = '60', end?: string): NetworkRange {
	const integer = (text: string) => {
		if (!/^[1-9][0-9]{0,9}$/.test(text) || Number(text) > 0xffff_ffff)
			throw new Error('Use positive whole block heights and bucket counts.');
		return Number(text);
	};
	const start = integer(from),
		finish = integer(to),
		count = integer(buckets);
	if (finish < start || finish - start >= MAX_BLOCKS || count > MAX_BUCKETS)
		throw new Error('Choose 1–20,160 blocks and 1–120 height buckets.');
	if (end !== undefined && !ID.test(end)) throw new Error('The end-block anchor is invalid.');
	return {
		from_height: start,
		to_height: finish,
		buckets: count,
		...(end ? { end_block_id: end } : {})
	};
}
function raw(value: unknown, maximum = U128_MAX): bigint {
	if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,38})$/.test(value) || BigInt(value) > maximum)
		throw new Error('Network history contains an invalid exact integer.');
	return BigInt(value);
}
function integer(value: unknown, minimum = 0): value is number {
	return typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
}
export function validateHistory(value: unknown, requested: NetworkRange): NetworkHistory {
	const bad = () => {
		throw new Error('The network-history response does not match the requested complete range.');
	};
	if (!value || typeof value !== 'object') return bad();
	const data = value as NetworkHistory;
	if (
		data.scope !== 'canonical_block_headers' ||
		data.consistency !== 'single_reader' ||
		data.complete !== true ||
		typeof data.full_history !== 'boolean' ||
		!integer(data.indexed_height, requested.to_height) ||
		(data.partial_from !== null &&
			(!integer(data.partial_from) || data.partial_from > requested.from_height)) ||
		(data.full_history && data.partial_from !== null) ||
		!data.anchor ||
		data.anchor.height !== requested.to_height ||
		!ID.test(data.anchor.block_id) ||
		(requested.end_block_id && requested.end_block_id !== data.anchor.block_id) ||
		data.requested_buckets !== requested.buckets ||
		data.bucket_width !==
			Math.ceil((requested.to_height - requested.from_height + 1) / requested.buckets) ||
		!Array.isArray(data.buckets) ||
		data.buckets.length !==
			Math.ceil((requested.to_height - requested.from_height + 1) / data.bucket_width)
	)
		return bad();
	const validateBucket = (bucket: NetworkBucket, from: number, to: number) => {
		if (
			!bucket ||
			bucket.from_height !== from ||
			bucket.to_height !== to ||
			bucket.block_count !== to - from + 1
		)
			return bad();
		for (const key of [
			'transaction_count',
			'fees',
			'size_bytes',
			'difficulty_min',
			'difficulty_max',
			'difficulty_end'
		] as const)
			raw(bucket[key]);
		if (
			raw(bucket.difficulty_min) > raw(bucket.difficulty_end) ||
			raw(bucket.difficulty_end) > raw(bucket.difficulty_max)
		)
			return bad();
		for (const timestamp of [
			bucket.first_timestamp,
			bucket.last_timestamp,
			bucket.earliest_timestamp,
			bucket.latest_timestamp
		])
			if (
				!integer(timestamp) ||
				timestamp < bucket.earliest_timestamp ||
				timestamp > bucket.latest_timestamp
			)
				return bad();
	};
	validateBucket(data.totals, requested.from_height, requested.to_height);
	data.buckets.forEach((bucket, index) =>
		validateBucket(
			bucket,
			requested.from_height + index * data.bucket_width,
			Math.min(requested.to_height, requested.from_height + (index + 1) * data.bucket_width - 1)
		)
	);
	for (const key of ['transaction_count', 'fees', 'size_bytes'] as const)
		if (data.buckets.reduce((sum, bucket) => sum + raw(bucket[key]), 0n) !== raw(data.totals[key]))
			return bad();
	const min = data.buckets.reduce(
		(m, bucket) => (raw(bucket.difficulty_min) < m ? raw(bucket.difficulty_min) : m),
		U128_MAX
	);
	const max = data.buckets.reduce(
		(m, bucket) => (raw(bucket.difficulty_max) > m ? raw(bucket.difficulty_max) : m),
		0n
	);
	if (
		raw(data.totals.difficulty_min) !== min ||
		raw(data.totals.difficulty_max) !== max ||
		data.totals.difficulty_end !== data.buckets.at(-1)!.difficulty_end ||
		data.totals.first_timestamp !== data.buckets[0].first_timestamp ||
		data.totals.last_timestamp !== data.buckets.at(-1)!.last_timestamp ||
		data.totals.earliest_timestamp !== Math.min(...data.buckets.map((b) => b.earliest_timestamp)) ||
		data.totals.latest_timestamp !== Math.max(...data.buckets.map((b) => b.latest_timestamp))
	)
		return bad();
	return data;
}
/** Convert only a bounded display ratio to Number. The plotted values stay exact strings. */
export function chartHeights(buckets: NetworkBucket[], metric: NetworkMetric): number[] {
	const values = buckets.map((bucket) => raw(bucket[metric]));
	const maximum = values.reduce((a, b) => (a > b ? a : b), 0n);
	return values.map((value) => (maximum === 0n ? 0 : Number((value * 1000n) / maximum) / 10));
}
export function historyQuery(requested: NetworkRange): string {
	return new URLSearchParams(
		Object.entries(requested).map(([key, value]) => [key, String(value)])
	).toString();
}
export async function fetchHistory(requested: NetworkRange, signal: AbortSignal): Promise<unknown> {
	const response = await fetch('/v1/network/history?' + historyQuery(requested), {
		signal,
		credentials: 'omit',
		redirect: 'error'
	});
	if (!response.body) throw new Error('Network history returned an empty response.');
	const reader = response.body.getReader();
	const chunks: Uint8Array[] = [];
	let size = 0;
	try {
		while (true) {
			const part = await reader.read();
			if (part.done) break;
			size += part.value.length;
			if (size > 256 * 1024) throw new Error('Network history exceeded its response limit.');
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
	const data = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
	if (!response.ok)
		throw new Error(
			typeof data.detail === 'string' ? data.detail : 'Network history is unavailable.'
		);
	return data;
}
export interface HistoryState {
	loading: boolean;
	data: NetworkHistory | null;
	error: string | null;
}
export function createHistoryLoader(
	onChange: (state: HistoryState) => void,
	fetcher = fetchHistory
) {
	let generation = 0;
	let active: AbortController | null = null;
	let stopped = false;
	function clear() {
		generation++;
		active?.abort();
		active = null;
		if (!stopped) onChange({ loading: false, data: null, error: null });
	}
	async function load(requested: NetworkRange) {
		if (stopped) return;
		clear();
		const request = generation;
		const controller = new AbortController();
		active = controller;
		onChange({ loading: true, data: null, error: null });
		const timer = setTimeout(() => controller.abort(), 10_000);
		try {
			const data = await fetcher(requested, controller.signal);
			if (stopped || request !== generation || controller.signal.aborted) return;
			onChange({ loading: false, data: validateHistory(data, requested), error: null });
		} catch (error) {
			if (!stopped && request === generation)
				onChange({
					loading: false,
					data: null,
					error: controller.signal.aborted
						? 'The range timed out. Try a smaller range.'
						: error instanceof Error
							? error.message
							: 'The range could not be loaded.'
				});
		} finally {
			clearTimeout(timer);
			if (!stopped && request === generation && controller.signal.aborted)
				onChange({
					loading: false,
					data: null,
					error: 'The range timed out. Try a smaller range.'
				});
		}
	}
	return {
		load,
		clear,
		stop() {
			stopped = true;
			clear();
		}
	};
}
