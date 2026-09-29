import { ApiError } from '$lib/api/client';

export const RENT_SCAN_LIMIT = 5000;
export const RENT_VIEW_LIMIT = 100;
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;

export interface RentExposureBox {
	id: string;
	value: string;
	creation_height: number;
	size: number;
	token_count: number;
	rent: {
		maturity_height: number;
		due_nano: string;
		claimable_at_tip: boolean;
		consensus_fee_nano: string;
		collectible: boolean;
	};
}
export interface RentExposure {
	items: RentExposureBox[];
	truncated: boolean;
	context: {
		scope: 'indexed_unspent_boxes';
		address: string;
		tree_hash: string;
		indexed_height: number | null;
		anchor: { height: number; block_id: string } | null;
		full_history: boolean;
		partial_from: number | null;
		scanned_count: number;
		scan_limit: number;
		scan_complete: boolean;
	};
}
export type RentCategory = 'mature' | 'approaching' | 'later' | 'non_collectible' | 'unknown';
export const RENT_CATEGORIES: Record<RentCategory, string> = {
	mature: 'Mature and collectible',
	approaching: 'Approaching maturity',
	later: 'Matures later',
	non_collectible: 'Not collectible under current rules',
	unknown: 'Snapshot height unavailable'
};
export interface RentSummary {
	complete: boolean;
	count: number;
	value: string;
	mature: string | null;
	approaching: string | null;
	counts: Record<RentCategory, number>;
}
const integer = (v: unknown): v is number =>
	typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 && v <= 0xffff_ffff;
const hash = (v: unknown): v is string => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
const amount = (v: unknown): v is string =>
	typeof v === 'string' &&
	/^(0|[1-9][0-9]{0,19})$/.test(v) &&
	BigInt(v) <= 18_446_744_073_709_551_615n;
const invalid = (): never => {
	throw new Error(
		'The server did not provide consistent anchored rent exposure. Refresh after the server is updated.'
	);
};

export function validateExposure(value: unknown, address: string): RentExposure {
	if (!value || typeof value !== 'object') return invalid();
	const data = value as RentExposure;
	const context = data.context;
	if (
		!context ||
		context.scope !== 'indexed_unspent_boxes' ||
		context.address !== address ||
		!hash(context.tree_hash) ||
		(context.indexed_height !== null && !integer(context.indexed_height)) ||
		(context.partial_from !== null && !integer(context.partial_from)) ||
		typeof context.full_history !== 'boolean' ||
		(context.full_history && context.partial_from !== null) ||
		(context.anchor !== null &&
			(!context.anchor ||
				!integer(context.anchor.height) ||
				context.anchor.height !== context.indexed_height ||
				!hash(context.anchor.block_id))) ||
		typeof data.truncated !== 'boolean' ||
		context.scan_complete !== !data.truncated ||
		context.scan_limit !== RENT_SCAN_LIMIT ||
		!Array.isArray(data.items) ||
		data.items.length > RENT_SCAN_LIMIT ||
		context.scanned_count !== data.items.length
	)
		return invalid();
	const ids = new Set<string>();
	for (const box of data.items) {
		const rent = box?.rent;
		if (
			!box ||
			!hash(box.id) ||
			ids.has(box.id) ||
			!amount(box.value) ||
			!integer(box.creation_height) ||
			!integer(box.size) ||
			!integer(box.token_count) ||
			!rent ||
			!integer(rent.maturity_height) ||
			rent.maturity_height < box.creation_height ||
			!amount(rent.due_nano) ||
			BigInt(rent.due_nano) > BigInt(box.value) ||
			typeof rent.consensus_fee_nano !== 'string' ||
			!/^(0|-?[1-9][0-9]{0,9})$/.test(rent.consensus_fee_nano) ||
			BigInt(rent.consensus_fee_nano) < -2_147_483_648n ||
			BigInt(rent.consensus_fee_nano) > 2_147_483_647n ||
			rent.collectible !== BigInt(rent.consensus_fee_nano) > 0n ||
			rent.claimable_at_tip !==
				(context.indexed_height !== null && rent.maturity_height <= context.indexed_height)
		)
			return invalid();
		ids.add(box.id);
	}
	return data;
}

/** Positive consensus fee capped by box value; nominal rent is deliberately not used. */
export function collectibleNano(box: RentExposureBox): bigint {
	const fee = BigInt(box.rent.consensus_fee_nano);
	return fee <= 0n ? 0n : fee < BigInt(box.value) ? fee : BigInt(box.value);
}
export function rentCategory(
	box: RentExposureBox,
	tip: number | null,
	horizon: number
): RentCategory {
	if (!box.rent.collectible) return 'non_collectible';
	if (tip === null) return 'unknown';
	const distance = box.rent.maturity_height - tip;
	return distance <= 0 ? 'mature' : distance <= horizon ? 'approaching' : 'later';
}
export function summarizeExposure(data: RentExposure, horizon: number): RentSummary {
	const counts: RentSummary['counts'] = {
		mature: 0,
		approaching: 0,
		later: 0,
		non_collectible: 0,
		unknown: 0
	};
	let value = 0n,
		mature = 0n,
		approaching = 0n;
	for (const box of data.items) {
		const category = rentCategory(box, data.context.indexed_height, horizon);
		counts[category]++;
		value += BigInt(box.value);
		if (category === 'mature') mature += collectibleNano(box);
		if (category === 'approaching') approaching += collectibleNano(box);
	}
	return {
		complete: data.context.full_history && data.context.scan_complete,
		count: data.items.length,
		value: value.toString(),
		mature: data.context.indexed_height === null ? null : mature.toString(),
		approaching: data.context.indexed_height === null ? null : approaching.toString(),
		counts
	};
}

export async function fetchExposure(address: string, signal: AbortSignal): Promise<RentExposure> {
	const response = await fetch(`/v1/addresses/${encodeURIComponent(address)}/rent?view=exposure`, {
		signal,
		credentials: 'omit',
		redirect: 'error',
		cache: 'no-store'
	});
	if (!response.body) throw new Error('The rent exposure response was empty.');
	const reader = response.body.getReader();
	const chunks: Uint8Array[] = [];
	let size = 0;
	try {
		while (true) {
			const part = await reader.read();
			if (part.done) break;
			size += part.value.byteLength;
			if (size > MAX_RESPONSE_BYTES)
				throw new Error('Rent exposure exceeds the browser response limit.');
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
	let json: unknown;
	try {
		json = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
	} catch {
		throw new Error(
			`Rent exposure is unavailable (HTTP ${response.status}); the response was not valid JSON.`
		);
	}
	if (!response.ok) {
		const problem = json && typeof json === 'object' ? (json as Record<string, unknown>) : {};
		throw new ApiError(
			response.status,
			'Rent exposure unavailable',
			typeof problem.detail === 'string'
				? problem.detail.slice(0, 512)
				: 'The address could not be checked.',
			typeof problem.code === 'string' ? problem.code : undefined
		);
	}
	return validateExposure(json, address);
}
export interface ExposureState {
	busy: boolean;
	result: RentExposure | null;
	checkedAt: number | null;
	stale: boolean;
	error: string | null;
}
export const emptyExposure = (): ExposureState => ({
	busy: false,
	result: null,
	checkedAt: null,
	stale: false,
	error: null
});
export function createExposureLoader(
	onChange: (state: ExposureState) => void,
	fetcher = fetchExposure
) {
	let current = emptyExposure();
	let address = '';
	let generation = 0;
	let stopped = false;
	let request: AbortController | null = null;
	const publish = (next: ExposureState) => {
		current = next;
		onChange(next);
	};
	return {
		setAddress(next: string) {
			if (next === address) return;
			generation++;
			request?.abort();
			address = next;
			publish(emptyExposure());
		},
		async load() {
			if (stopped || current.busy || !address) return;
			const sequence = ++generation;
			const controller = new AbortController();
			request = controller;
			const timer = setTimeout(() => controller.abort(), 10_000);
			publish({ ...current, busy: true, error: null });
			try {
				const result = await fetcher(address, controller.signal);
				if (stopped || sequence !== generation) return;
				if (controller.signal.aborted) throw new Error('The rent exposure check timed out.');
				publish({
					busy: false,
					result: validateExposure(result, address),
					checkedAt: Date.now(),
					stale: false,
					error: null
				});
			} catch (error) {
				if (stopped || sequence !== generation) return;
				const message =
					error instanceof ApiError && error.status === 404
						? 'This address or the exposure view is unavailable on this server. Unseen addresses are not zero exposure.'
						: error instanceof ApiError && error.status === 422
							? 'This address exceeds the rent read budget. Use its paged unspent boxes for individual evidence.'
							: controller.signal.aborted
								? 'The rent exposure check timed out. Try again.'
								: error instanceof Error
									? error.message
									: 'Rent exposure could not be checked.';
				publish({ ...current, busy: false, stale: current.result !== null, error: message });
			} finally {
				clearTimeout(timer);
			}
		},
		stop() {
			stopped = true;
			generation++;
			request?.abort();
		}
	};
}
