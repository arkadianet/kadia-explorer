export const MAX_BLOCKS = 20_160;
export const MAX_TOP = 50;
const ID = /^[a-f0-9]{64}$/;
const KEY = /^[a-f0-9]{66}$/;
const VOTES = /^[a-f0-9]{6}$/;
const U128_MAX = (1n << 128n) - 1n;
export interface MiningRange {
	from_height: number;
	to_height: number;
	top: number;
	end_block_id?: string;
}
export interface MiningKey {
	public_key: string;
	block_count: number;
	first_height: number;
	last_height: number;
	first_block_id: string;
	last_block_id: string;
	fees: string;
}
export interface MiningOverview {
	scope: 'canonical_block_headers';
	consistency: 'single_reader';
	complete: true;
	from_height: number;
	to_height: number;
	block_count: number;
	indexed_height: number;
	full_history: boolean;
	partial_from: number | null;
	anchor: { height: number; block_id: string };
	top: number;
	totals: { fees: string; transaction_count: string };
	miner_keys: {
		distinct_count: number;
		items: MiningKey[];
		other_key_count: number;
		other_block_count: number;
		other_fees: string;
	};
	versions: { version: number; block_count: number }[];
	votes: {
		known_blocks: number;
		unknown_blocks: number;
		zero_vote_blocks: number;
		distinct_tuples: number;
		items: { votes: string; block_count: number }[];
		other_tuple_count: number;
		other_block_count: number;
	};
}
export function range(from: string, to: string, top = '20', end?: string): MiningRange {
	const integer = (text: string) => {
		if (!/^[1-9][0-9]{0,9}$/.test(text) || Number(text) > 0xffff_ffff)
			throw new Error('Use positive whole block heights and a top count from 1 to 50.');
		return Number(text);
	};
	const start = integer(from),
		finish = integer(to),
		count = integer(top);
	if (finish < start || finish - start >= MAX_BLOCKS || count > MAX_TOP)
		throw new Error('Choose 1–20,160 blocks and a top count from 1 to 50.');
	if (end !== undefined && !ID.test(end)) throw new Error('The end-block anchor is invalid.');
	return {
		from_height: start,
		to_height: finish,
		top: count,
		...(end ? { end_block_id: end } : {})
	};
}
function integer(value: unknown, minimum = 0, maximum = 0xffff_ffff): value is number {
	return (
		typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum && value <= maximum
	);
}
function id(value: unknown): value is string {
	return typeof value === 'string' && ID.test(value);
}
function raw(value: unknown): bigint {
	if (
		typeof value !== 'string' ||
		!/^(0|[1-9][0-9]{0,38})$/.test(value) ||
		BigInt(value) > U128_MAX
	)
		throw new Error('Mining signals contain an invalid exact integer.');
	return BigInt(value);
}
export function validateOverview(value: unknown, requested: MiningRange): MiningOverview {
	const bad = (): never => {
		throw new Error('The mining response does not match the requested complete range.');
	};
	if (!value || typeof value !== 'object') return bad();
	const data = value as MiningOverview;
	const count = requested.to_height - requested.from_height + 1;
	if (
		data.scope !== 'canonical_block_headers' ||
		data.consistency !== 'single_reader' ||
		data.complete !== true ||
		data.from_height !== requested.from_height ||
		data.to_height !== requested.to_height ||
		data.block_count !== count ||
		data.top !== requested.top ||
		!integer(data.indexed_height, requested.to_height) ||
		typeof data.full_history !== 'boolean' ||
		(data.partial_from !== null && !integer(data.partial_from, 0, requested.from_height)) ||
		(data.full_history && data.partial_from !== null) ||
		!data.anchor ||
		data.anchor.height !== requested.to_height ||
		!id(data.anchor.block_id) ||
		(requested.end_block_id && data.anchor.block_id !== requested.end_block_id) ||
		!data.totals
	)
		return bad();
	const fees = raw(data.totals.fees);
	raw(data.totals.transaction_count);
	const keys = data.miner_keys;
	if (
		!keys ||
		!integer(keys.distinct_count, 1, count) ||
		!Array.isArray(keys.items) ||
		keys.items.length !== Math.min(keys.distinct_count, requested.top) ||
		keys.other_key_count !== keys.distinct_count - keys.items.length ||
		!integer(keys.other_block_count, keys.other_key_count, count)
	)
		return bad();
	const seenKeys = new Set<string>();
	let keyBlocks = keys.other_block_count;
	let keyFees = raw(keys.other_fees);
	keys.items.forEach((key, index) => {
		if (
			!key ||
			typeof key.public_key !== 'string' ||
			!KEY.test(key.public_key) ||
			seenKeys.has(key.public_key) ||
			!integer(key.block_count, 1, count) ||
			!integer(key.first_height, requested.from_height, requested.to_height) ||
			!integer(key.last_height, key.first_height, requested.to_height) ||
			key.block_count > key.last_height - key.first_height + 1 ||
			!id(key.first_block_id) ||
			!id(key.last_block_id) ||
			(key.block_count === 1 && key.first_height !== key.last_height) ||
			(key.first_height !== key.last_height && key.first_block_id === key.last_block_id) ||
			(key.first_height === key.last_height && key.first_block_id !== key.last_block_id) ||
			(key.first_height === requested.to_height && key.first_block_id !== data.anchor.block_id) ||
			(key.last_height === requested.to_height && key.last_block_id !== data.anchor.block_id)
		)
			return bad();
		const previous = keys.items[index - 1];
		if (
			previous &&
			(previous.block_count < key.block_count ||
				(previous.block_count === key.block_count && previous.public_key >= key.public_key))
		)
			return bad();
		seenKeys.add(key.public_key);
		keyBlocks += key.block_count;
		keyFees += raw(key.fees);
	});
	if (
		keyBlocks !== count ||
		keyFees !== fees ||
		(keys.other_key_count === 0 &&
			(keys.other_block_count !== 0 ||
				keyFees - keys.items.reduce((sum, key) => sum + raw(key.fees), 0n) !== 0n)) ||
		keys.other_block_count > keys.other_key_count * keys.items.at(-1)!.block_count
	)
		return bad();
	if (
		!Array.isArray(data.versions) ||
		data.versions.length < 1 ||
		data.versions.length > Math.min(256, count)
	)
		return bad();
	let versionBlocks = 0;
	data.versions.forEach((version, index) => {
		if (
			!version ||
			!integer(version.version, 0, 255) ||
			!integer(version.block_count, 1, count) ||
			(index > 0 && data.versions[index - 1].version >= version.version)
		)
			return bad();
		versionBlocks += version.block_count;
	});
	if (versionBlocks !== count) return bad();
	const votes = data.votes;
	if (
		!votes ||
		!integer(votes.known_blocks, 0, count) ||
		!integer(votes.unknown_blocks, 0, count) ||
		votes.known_blocks + votes.unknown_blocks !== count ||
		!integer(votes.zero_vote_blocks, 0, votes.known_blocks) ||
		!integer(votes.distinct_tuples, votes.known_blocks > 0 ? 1 : 0, votes.known_blocks) ||
		!Array.isArray(votes.items) ||
		votes.items.length !== Math.min(votes.distinct_tuples, requested.top) ||
		votes.other_tuple_count !== votes.distinct_tuples - votes.items.length ||
		!integer(votes.other_block_count, votes.other_tuple_count, votes.known_blocks)
	)
		return bad();
	const seenVotes = new Set<string>();
	let voteBlocks = votes.other_block_count;
	votes.items.forEach((tuple, index) => {
		if (
			!tuple ||
			typeof tuple.votes !== 'string' ||
			!VOTES.test(tuple.votes) ||
			seenVotes.has(tuple.votes) ||
			!integer(tuple.block_count, 1, votes.known_blocks)
		)
			return bad();
		const previous = votes.items[index - 1];
		if (
			previous &&
			(previous.block_count < tuple.block_count ||
				(previous.block_count === tuple.block_count && previous.votes >= tuple.votes))
		)
			return bad();
		seenVotes.add(tuple.votes);
		voteBlocks += tuple.block_count;
	});
	const zero = votes.items.find((tuple) => tuple.votes === '000000');
	if (
		voteBlocks !== votes.known_blocks ||
		(votes.other_tuple_count === 0 && votes.other_block_count !== 0) ||
		votes.other_block_count > votes.other_tuple_count * (votes.items.at(-1)?.block_count ?? 0) ||
		(zero
			? zero.block_count !== votes.zero_vote_blocks
			: votes.zero_vote_blocks > votes.other_block_count)
	)
		return bad();
	return data;
}
/** Only the decorative width is quantized. Visible shares remain exact count / denominator. */
export function shareWidth(count: number, total: number): number {
	return total > 0 ? Number((BigInt(count) * 1000n) / BigInt(total)) / 10 : 0;
}
export function miningQuery(requested: MiningRange): string {
	return new URLSearchParams(
		Object.entries(requested).map(([key, value]) => [key, String(value)])
	).toString();
}
export async function fetchOverview(requested: MiningRange, signal: AbortSignal): Promise<unknown> {
	const response = await fetch('/v1/mining?' + miningQuery(requested), {
		signal,
		credentials: 'omit',
		redirect: 'error'
	});
	if (!response.body) throw new Error('Mining signals returned an empty response.');
	const reader = response.body.getReader(),
		chunks: Uint8Array[] = [];
	let size = 0;
	try {
		while (true) {
			const part = await reader.read();
			if (part.done) break;
			size += part.value.length;
			if (size > 128 * 1024) throw new Error('Mining signals exceeded their response limit.');
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
			data && typeof data.detail === 'string' ? data.detail : 'Mining signals are unavailable.'
		);
	return data;
}
export interface MiningState {
	loading: boolean;
	data: MiningOverview | null;
	error: string | null;
}
export function createMiningLoader(
	onChange: (state: MiningState) => void,
	fetcher = fetchOverview
) {
	let generation = 0,
		stopped = false;
	let active: AbortController | null = null;
	function clear() {
		generation++;
		active?.abort();
		active = null;
		if (!stopped) onChange({ loading: false, data: null, error: null });
	}
	async function load(requested: MiningRange) {
		if (stopped) return;
		clear();
		const request = generation,
			controller = new AbortController();
		active = controller;
		onChange({ loading: true, data: null, error: null });
		const timer = setTimeout(() => controller.abort(), 10_000);
		try {
			const data = await fetcher(requested, controller.signal);
			if (stopped || request !== generation || controller.signal.aborted) return;
			onChange({ loading: false, data: validateOverview(data, requested), error: null });
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
