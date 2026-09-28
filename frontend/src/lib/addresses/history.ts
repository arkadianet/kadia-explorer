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
