import { ApiError } from '$lib/api/client';

export interface GroupToken {
	id: string;
	amount: string;
}
export interface GroupBalance {
	nano: string;
	tokens: GroupToken[];
}
export interface GroupMember {
	address: string;
	tree_hash: string | null;
	status: 'resolved' | 'unseen' | 'invalid' | 'duplicate';
	duplicate_of: number | null;
	balance: GroupBalance | null;
}
export interface GroupBalances {
	scope: 'selected_address_scripts';
	consistency: 'single_reader';
	indexed_height: number | null;
	anchor: { height: number; block_id: string } | null;
	full_history: boolean;
	partial_from: number | null;
	complete: boolean;
	requested_count: number;
	resolved_script_count: number;
	members: GroupMember[];
	observed_totals: GroupBalance | null;
}

export function groupRequest(addresses: readonly string[]): { addresses: string[] } {
	if (!addresses.length || addresses.length > 100)
		throw new Error('Choose between 1 and 100 saved addresses.');
	const encoder = new TextEncoder();
	let bytes = 0;
	for (const address of addresses) {
		const length = encoder.encode(address).length;
		if (!length || length > 4096) throw new Error('An address exceeds the supported size.');
		bytes += length;
	}
	if (bytes > 128_000) throw new Error('This selection exceeds the 128,000-byte address limit.');
	const body = { addresses: [...addresses] };
	if (encoder.encode(JSON.stringify(body)).length > 512_000)
		throw new Error('This selection exceeds the request size limit.');
	return body;
}

/** Check the response belongs to this exact selection before displaying a combined balance. */
export function validateGroupBalances(value: GroupBalances, addresses: readonly string[]) {
	const invalid = (): never => {
		throw new Error('The server returned an inconsistent group balance. Try loading again.');
	};
	const height = (v: unknown) => v === null || (Number.isSafeInteger(v) && Number(v) >= 0);
	const hash = (v: unknown) => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
	const amount = (v: unknown) => typeof v === 'string' && /^(0|[1-9]\d{0,79})$/.test(v);
	const balance = (v: GroupBalance | null) =>
		v &&
		amount(v.nano) &&
		Array.isArray(v.tokens) &&
		v.tokens.every((t) => t && hash(t.id) && amount(t.amount)) &&
		new Set(v.tokens.map((t) => t.id)).size === v.tokens.length;
	if (
		!value ||
		value.scope !== 'selected_address_scripts' ||
		value.consistency !== 'single_reader' ||
		!height(value.indexed_height) ||
		!height(value.partial_from) ||
		typeof value.full_history !== 'boolean' ||
		typeof value.complete !== 'boolean' ||
		(value.anchor !== null &&
			(!value.anchor ||
				!height(value.anchor.height) ||
				value.anchor.height !== value.indexed_height ||
				!hash(value.anchor.block_id))) ||
		value.requested_count !== addresses.length ||
		!Number.isSafeInteger(value.resolved_script_count) ||
		value.resolved_script_count < 0 ||
		value.resolved_script_count > addresses.length ||
		!Array.isArray(value.members) ||
		value.members.length !== addresses.length
	)
		invalid();
	let missing = false;
	let resolved = 0;
	const scripts = new Set<string>();
	let nano = 0n;
	const tokens = new Map<string, bigint>();
	value.members.forEach((member, i) => {
		if (!member || member.address !== addresses[i]) invalid();
		if (member.status === 'duplicate') {
			const first = member.duplicate_of;
			if (
				first === null ||
				!Number.isInteger(first) ||
				first < 0 ||
				first >= i ||
				member.balance !== null ||
				!hash(member.tree_hash) ||
				value.members[first].tree_hash !== member.tree_hash ||
				value.members[first].status === 'duplicate'
			)
				invalid();
			return;
		}
		if (member.duplicate_of !== null) invalid();
		if (member.status === 'invalid') {
			if (member.tree_hash !== null || member.balance !== null) invalid();
			missing = true;
			return;
		}
		if (!hash(member.tree_hash) || scripts.has(member.tree_hash!)) invalid();
		scripts.add(member.tree_hash!);
		if (member.status === 'unseen') {
			if (member.balance !== null) invalid();
			missing = true;
			return;
		}
		if (member.status !== 'resolved' || !balance(member.balance)) invalid();
		resolved++;
		nano += BigInt(member.balance!.nano);
		for (const token of member.balance!.tokens)
			tokens.set(token.id, (tokens.get(token.id) ?? 0n) + BigInt(token.amount));
	});
	if (
		value.resolved_script_count !== resolved ||
		value.complete !== (value.full_history && !missing)
	)
		invalid();
	if (missing) {
		if (value.observed_totals !== null) invalid();
	} else {
		if (!balance(value.observed_totals) || BigInt(value.observed_totals!.nano) !== nano) invalid();
		const totalTokens = value.observed_totals!.tokens;
		if (
			totalTokens.length !== tokens.size ||
			totalTokens.some((t) => tokens.get(t.id) !== BigInt(t.amount))
		)
			invalid();
	}
	return value;
}

export async function fetchGroupBalances(
	addresses: readonly string[],
	signal?: AbortSignal,
	fetchFn: typeof fetch = fetch
): Promise<GroupBalances> {
	const body = groupRequest(addresses);
	const controller = new AbortController();
	const abort = () => controller.abort();
	if (signal?.aborted) controller.abort();
	signal?.addEventListener('abort', abort, { once: true });
	const timeout = setTimeout(abort, 10_000);
	try {
		const response = await fetchFn('/v1/addresses/balances', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(body),
			signal: controller.signal
		});
		if (!response.ok) {
			const problem = await response.json().catch(() => ({}));
			throw new ApiError(
				response.status,
				problem.title || response.statusText || 'Request failed',
				problem.detail || 'The selected balances could not be loaded.',
				problem.code
			);
		}
		return validateGroupBalances((await response.json()) as GroupBalances, addresses);
	} finally {
		clearTimeout(timeout);
		signal?.removeEventListener('abort', abort);
	}
}

export function groupIssue(error: unknown): string {
	if (error instanceof ApiError) {
		if (error.status === 404) return 'Group balances are not available on this server yet.';
		if (error.status === 422)
			return 'This selection exceeds the server’s read budget. Choose a smaller group or filter to fewer addresses.';
		if (error.status === 413) return 'This selection is too large. Choose fewer addresses.';
		if (error.status === 429 || error.status === 503)
			return 'Group balances are temporarily unavailable or busy. Try again shortly.';
	}
	if (error instanceof Error && error.name === 'AbortError')
		return 'The balance request timed out. Try loading again.';
	return error instanceof Error ? error.message : 'The selected balances could not be loaded.';
}

export interface GroupState {
	busy: boolean;
	result: GroupBalances | null;
	stale: boolean;
	error: string | null;
}
const empty = (): GroupState => ({ busy: false, result: null, stale: false, error: null });

export function createGroupBalances(
	onChange: (state: GroupState) => void,
	load: typeof fetchGroupBalances = fetchGroupBalances
) {
	let state = empty();
	let addresses: string[] = [];
	let generation = 0;
	let stopped = false;
	let request: AbortController | null = null;
	const publish = (next: GroupState) => {
		state = next;
		onChange(next);
	};
	return {
		get state() {
			return state;
		},
		setAddresses(next: readonly string[]) {
			if (JSON.stringify(next) === JSON.stringify(addresses)) return;
			generation++;
			request?.abort();
			addresses = [...next];
			publish(empty());
		},
		async load() {
			if (stopped || state.busy || !addresses.length) return;
			const current = ++generation;
			request = new AbortController();
			publish({ ...state, busy: true, error: null });
			try {
				const result = await load([...addresses], request.signal);
				if (stopped || current !== generation) return;
				publish({ result, busy: false, stale: false, error: null });
			} catch (error) {
				if (stopped || current !== generation) return;
				publish({ ...state, busy: false, stale: state.result !== null, error: groupIssue(error) });
			}
		},
		stop() {
			stopped = true;
			generation++;
			request?.abort();
		}
	};
}
