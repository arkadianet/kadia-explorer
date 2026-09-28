import { apiGet, ApiError } from '$lib/api/client';

export interface HistoryAnchor {
	height: number;
	block_id: string | null;
}
export interface HistoryToken {
	token_id: string;
	amount: string;
}
export interface HistoryBalance {
	address: string;
	at: HistoryAnchor;
	indexed_height: number | null;
	balance: { nano: string; box_count: number; tokens: HistoryToken[] };
	complete: boolean;
}
export interface HistoryBox {
	box_id: string;
	inclusion_height: number;
	nano: string;
	tokens: HistoryToken[];
}
export interface HistoryBoxes {
	at: HistoryAnchor;
	items: HistoryBox[];
	next_cursor: string | null;
}
export interface HistorySelector {
	height: number;
	block_id?: string;
}
export interface HistoryComparison {
	address: string;
	indexed_height: number | null;
	complete: boolean;
	from: { at: HistoryAnchor; balance: HistoryBalance['balance'] };
	to: { at: HistoryAnchor; balance: HistoryBalance['balance'] };
}
export interface HistoryIssue {
	message: string;
	code: string;
	restart: boolean;
}
export const HISTORY_BOX_LIMIT = 500;
export const HISTORY_TOKEN_LIMIT = 10_000;

export function historySelector(
	height: string | null,
	blockId: string | null = null
): { value: HistorySelector | null; error: string | null } {
	if (height === null)
		return { value: null, error: blockId ? 'A pinned block ID also needs a height.' : null };
	if (!/^\d+$/.test(height) || Number(height) > 4_294_967_295)
		return { value: null, error: 'Enter a whole block height from 0 to 4,294,967,295.' };
	if (blockId !== null && !/^[a-fA-F0-9]{64}$/.test(blockId))
		return { value: null, error: 'The pinned block ID must contain 64 hexadecimal characters.' };
	if (Number(height) === 0 && blockId !== null)
		return { value: null, error: 'Height 0 is the genesis state and has no block ID.' };
	return {
		value: {
			height: Number(height),
			...(blockId === null ? {} : { block_id: blockId.toLowerCase() })
		},
		error: null
	};
}

export function historyIssue(error: unknown): HistoryIssue {
	const code = error instanceof ApiError ? (error.code ?? '') : '';
	if (error instanceof ApiError && error.status === 409)
		return {
			code,
			restart: true,
			message:
				'This historical anchor changed or is no longer indexed. The old results were cleared. Start again at the current canonical block for this height.'
		};
	const descriptions: Record<string, string> = {
		history_unavailable:
			'Exact history is unavailable on this server. It requires a complete mainnet index including genesis; a partial index cannot establish these balances.',
		height_not_indexed:
			'This height is above the server’s indexed tip. Choose an earlier height or try again after the index catches up.',
		history_scan_limit:
			'The full balance exceeds the server’s scan budget. Inspect the anchored box pages below; loaded totals remain incomplete until every page is read.',
		history_response_limit:
			'The historical result exceeds the server’s response budget. Smaller box pages may still be available below.',
		invalid_history_query:
			'The historical request is invalid. Check the height and pinned block ID.'
	};
	return {
		code,
		restart: false,
		message:
			descriptions[code] ??
			(error instanceof ApiError && error.status === 404
				? 'Historical snapshots are not available on this server yet.'
				: error instanceof ApiError && error.status === 503
					? 'Historical reads are temporarily unavailable or busy. Try again shortly.'
					: error instanceof Error
						? error.message
						: 'The historical snapshot could not be loaded.')
	};
}

export const historyApi = {
	balance: (address: string, at: HistorySelector) =>
		apiGet<HistoryBalance>(`/addresses/${encodeURIComponent(address)}/balance/at`, { ...at }),
	boxes: (address: string, at: HistorySelector, cursor?: string, limit = 20) =>
		apiGet<HistoryBoxes>(`/addresses/${encodeURIComponent(address)}/boxes/at`, {
			...at,
			cursor,
			limit
		})
};

export function comparisonSelectors(
	from: string | null,
	to: string | null,
	fromBlock: string | null = null,
	toBlock: string | null = null
) {
	const before = historySelector(from, fromBlock);
	const after = historySelector(to, toBlock);
	const error =
		before.error ??
		after.error ??
		(!before.value || !after.value
			? 'Enter both comparison heights.'
			: before.value.height > after.value.height
				? 'The earlier height must not exceed the later height.'
				: null);
	return {
		value: error || !before.value || !after.value ? null : { from: before.value, to: after.value },
		error
	};
}

export function historyComparisonLink(address: string, value: HistoryComparison): string {
	const params = new URLSearchParams({
		from_height: String(value.from.at.height),
		to_height: String(value.to.at.height)
	});
	if (value.from.at.block_id) params.set('from_block', value.from.at.block_id);
	if (value.to.at.block_id) params.set('to_block', value.to.at.block_id);
	return `/address/${encodeURIComponent(address)}?${params}#history`;
}

export async function historyCompareApi(
	address: string,
	from: HistorySelector,
	to: HistorySelector
) {
	return apiGet<HistoryComparison>(`/addresses/${encodeURIComponent(address)}/balance/compare`, {
		from_height: from.height,
		to_height: to.height,
		from_block_id: from.block_id,
		to_block_id: to.block_id
	});
}

/** Reject incomplete or inconsistent endpoint states before any BigInt subtraction. */
export function comparisonChanges(
	value: HistoryComparison,
	address: string,
	from: HistorySelector,
	to: HistorySelector
) {
	if (!value || value.complete !== true || value.address !== address || !value.from || !value.to)
		throw new Error(
			'A complete comparison for this address is required. No balance difference is shown.'
		);
	if (
		from.height > to.height ||
		(value.indexed_height === null
			? to.height !== 0
			: !Number.isSafeInteger(value.indexed_height) || value.indexed_height < to.height)
	)
		throw new Error('The comparison has invalid indexed coverage. No balance difference is shown.');
	for (const [point, requested] of [
		[value.from, from],
		[value.to, to]
	] as const) {
		if (
			!point.at ||
			point.at.height !== requested.height ||
			(requested.height === 0
				? point.at.block_id !== null
				: typeof point.at.block_id !== 'string' || !/^[a-f0-9]{64}$/i.test(point.at.block_id)) ||
			(requested.block_id && point.at.block_id?.toLowerCase() !== requested.block_id.toLowerCase())
		)
			throw new ApiError(
				409,
				'Snapshot changed',
				'A comparison anchor does not match the request.',
				'snapshot_changed'
			);
	}
	if (from.height === to.height && value.from.at.block_id !== value.to.at.block_id)
		throw new ApiError(
			409,
			'Snapshot changed',
			'Equal heights returned different blocks.',
			'snapshot_changed'
		);
	const raw = (amount: unknown): amount is string =>
		typeof amount === 'string' &&
		/^(0|[1-9]\d{0,38})$/.test(amount) &&
		BigInt(amount) <= (1n << 128n) - 1n;
	let entries = 0;
	for (const point of [value.from, value.to]) {
		const balance = point.balance;
		if (
			!balance ||
			!raw(balance.nano) ||
			!Number.isSafeInteger(balance.box_count) ||
			balance.box_count < 0 ||
			!Array.isArray(balance.tokens)
		)
			throw new Error('The comparison contains invalid exact balances. Refresh before comparing.');
		entries += balance.tokens.length;
		if (entries > HISTORY_TOKEN_LIMIT)
			throw new Error(
				'This comparison exceeds the 10,000-token-entry view limit. No partial difference is shown.'
			);
		const seen = new Set<string>();
		for (const token of balance.tokens) {
			if (
				!token ||
				typeof token.token_id !== 'string' ||
				!/^[a-f0-9]{64}$/.test(token.token_id) ||
				!raw(token.amount) ||
				seen.has(token.token_id)
			)
				throw new Error(
					'The comparison contains invalid or repeated token balances. No difference is shown.'
				);
			seen.add(token.token_id);
		}
	}
	const before = new Map(
		value.from.balance.tokens.map((token) => [token.token_id, BigInt(token.amount)])
	);
	const after = new Map(
		value.to.balance.tokens.map((token) => [token.token_id, BigInt(token.amount)])
	);
	return {
		nano: (BigInt(value.to.balance.nano) - BigInt(value.from.balance.nano)).toString(),
		box_count: (
			BigInt(value.to.balance.box_count) - BigInt(value.from.balance.box_count)
		).toString(),
		tokens: [...new Set([...before.keys(), ...after.keys()])].sort().map((token_id) => ({
			token_id,
			before: (before.get(token_id) ?? 0n).toString(),
			after: (after.get(token_id) ?? 0n).toString(),
			delta: ((after.get(token_id) ?? 0n) - (before.get(token_id) ?? 0n)).toString()
		}))
	};
}

export interface ComparisonState {
	busy: boolean;
	result: HistoryComparison | null;
	changes: ReturnType<typeof comparisonChanges> | null;
	issue: HistoryIssue | null;
}
export function createHistoryComparison(
	onChange: (value: ComparisonState) => void,
	api = historyCompareApi
) {
	let current: ComparisonState = { busy: false, result: null, changes: null, issue: null };
	let generation = 0;
	let stopped = false;
	function update(value: ComparisonState) {
		current = value;
		onChange(value);
	}
	return {
		get state() {
			return current;
		},
		reset() {
			generation++;
			if (!stopped) update({ busy: false, result: null, changes: null, issue: null });
		},
		stop() {
			generation++;
			stopped = true;
		},
		async start(address: string, from: HistorySelector, to: HistorySelector) {
			if (stopped) return;
			const own = ++generation;
			update({ busy: true, result: null, changes: null, issue: null });
			try {
				const result = await api(address, from, to);
				if (stopped || own !== generation) return;
				const changes = comparisonChanges(result, address, from, to);
				update({ busy: false, result, changes, issue: null });
			} catch (error) {
				if (stopped || own !== generation) return;
				const issue = historyIssue(error);
				if (['history_scan_limit', 'history_response_limit'].includes(issue.code))
					issue.message =
						'The complete comparison exceeds the server’s work or response budget. Neither balance nor a partial difference is shown. Try a smaller address; separately loaded box pages cannot establish this comparison.';
				update({ busy: false, result: null, changes: null, issue });
			}
		}
	};
}

export function sumHistoryBoxes(boxes: HistoryBox[]) {
	let nano = 0n;
	const tokens = new Map<string, bigint>();
	for (const box of boxes) {
		nano += BigInt(box.nano);
		for (const token of box.tokens)
			tokens.set(token.token_id, (tokens.get(token.token_id) ?? 0n) + BigInt(token.amount));
	}
	return {
		nano: nano.toString(),
		box_count: boxes.length,
		tokens: [...tokens]
			.sort(([a], [b]) => a.localeCompare(b))
			.map(([token_id, amount]) => ({ token_id, amount: amount.toString() }))
	};
}

export interface HistoryState {
	started: boolean;
	busy: boolean;
	balance: HistoryBalance | null;
	anchor: HistoryAnchor | null;
	boxes: HistoryBox[];
	boxesLoaded: boolean;
	nextCursor: string | null;
	issue: HistoryIssue | null;
	balanceIssue: HistoryIssue | null;
	restartRequired: boolean;
	viewLimit: boolean;
}
const empty = (): HistoryState => ({
	started: false,
	busy: false,
	balance: null,
	anchor: null,
	boxes: [],
	boxesLoaded: false,
	nextCursor: null,
	issue: null,
	balanceIssue: null,
	restartRequired: false,
	viewLimit: false
});

/** No requests run until start() is explicitly invoked by the user. Each page is bound
 * to H's block ID; append-only tip changes do not invalidate this historical anchor. */
export function createHistoryInspector(onChange: (state: HistoryState) => void, api = historyApi) {
	let current = empty();
	let generation = 0;
	let disposed = false;
	let address = '';
	let selector: HistorySelector = { height: 0 };
	function update(patch: Partial<HistoryState>) {
		current = { ...current, ...patch };
		onChange(current);
	}
	function checkedAnchor(at: HistoryAnchor) {
		if (
			at.height !== selector.height ||
			(selector.height === 0
				? at.block_id !== null
				: !at.block_id || !/^[a-f0-9]{64}$/i.test(at.block_id)) ||
			(selector.block_id && at.block_id?.toLowerCase() !== selector.block_id.toLowerCase()) ||
			(current.anchor &&
				(current.anchor.height !== at.height || current.anchor.block_id !== at.block_id))
		)
			throw new ApiError(
				409,
				'Snapshot changed',
				'Historical responses did not share one anchor.',
				'snapshot_changed'
			);
	}
	function failure(error: unknown) {
		const issue = historyIssue(error);
		if (issue.restart) update({ ...empty(), started: true, restartRequired: true, issue });
		else update({ issue });
	}
	async function boxPage(my: number) {
		const at = current.anchor
			? {
					height: current.anchor.height,
					...(current.anchor.block_id ? { block_id: current.anchor.block_id } : {})
				}
			: selector;
		try {
			const page = await api.boxes(
				address,
				at,
				current.nextCursor ?? undefined,
				Math.min(20, HISTORY_BOX_LIMIT - current.boxes.length)
			);
			if (disposed || my !== generation) return;
			checkedAnchor(page.at);
			const boxes = [...current.boxes, ...page.items];
			if (
				boxes.length > HISTORY_BOX_LIMIT ||
				boxes.reduce((count, box) => count + box.tokens.length, 0) > HISTORY_TOKEN_LIMIT
			) {
				update({
					anchor: page.at,
					viewLimit: true,
					issue: {
						code: 'view_limit',
						restart: false,
						message:
							'The next page exceeds this view’s 500-box or 10,000-token-entry limit. Loaded box totals are incomplete; the historical API can continue this anchor.'
					}
				});
				return;
			}
			update({
				anchor: page.at,
				boxes,
				boxesLoaded: true,
				nextCursor: page.next_cursor,
				viewLimit: boxes.length === HISTORY_BOX_LIMIT && page.next_cursor !== null
			});
		} catch (error) {
			if (!disposed && my === generation) failure(error);
		}
	}
	return {
		get state() {
			return current;
		},
		reset() {
			generation++;
			if (!disposed) {
				current = empty();
				onChange(current);
			}
		},
		stop() {
			generation++;
			disposed = true;
		},
		async start(nextAddress: string, nextSelector: HistorySelector) {
			if (disposed) return;
			const my = ++generation;
			address = nextAddress;
			selector = nextSelector;
			current = empty();
			update({ started: true, busy: true });
			try {
				const balance = await api.balance(address, selector);
				if (disposed || my !== generation) return;
				checkedAnchor(balance.at);
				if (!balance.complete)
					throw new Error(
						'The server did not return a complete historical balance. No exact total is shown.'
					);
				update({ balance, anchor: balance.at });
			} catch (error) {
				if (disposed || my !== generation) return;
				const issue = historyIssue(error);
				if (!['history_scan_limit', 'history_response_limit'].includes(issue.code)) {
					failure(error);
					update({ busy: false });
					return;
				}
				update({ balanceIssue: issue });
			}
			await boxPage(my);
			if (!disposed && my === generation) update({ busy: false });
		},
		async more() {
			if (
				disposed ||
				current.busy ||
				current.restartRequired ||
				current.viewLimit ||
				(current.boxesLoaded && current.nextCursor === null) ||
				!current.started
			)
				return;
			const my = generation;
			update({ busy: true, issue: null });
			await boxPage(my);
			if (!disposed && my === generation) update({ busy: false });
		}
	};
}
