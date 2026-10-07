import { api } from '$lib/api/endpoints';
import { ApiError } from '$lib/api/client';
import type {
	RentScheduleBatchDto,
	RentScheduleItemDto,
	RentScheduleMode,
	RentSchedulePageDto,
	RentScheduleWindow
} from '$lib/api/types';

export const SCHEDULE_WINDOWS = ['24h', '7d', '30d', '90d'] as const;
export const SCHEDULE_MODES = ['all', 'collectible', 'full_claim'] as const;
export const SCHEDULE_ROW_LIMIT = 500;
const HASH = /^[0-9a-f]{64}$/;
const UINT = /^(0|[1-9][0-9]{0,38})$/;
const HOUR = 3_600_000;
const DAY = 24 * HOUR;

export interface ScheduleQuery {
	window: RentScheduleWindow;
	mode: RentScheduleMode;
	token_id?: string;
}

export function scheduleQuery(params: URLSearchParams): {
	query: ScheduleQuery;
	error: string | null;
} {
	const window = params.get('window') ?? '24h';
	const mode = params.get('mode') ?? 'all';
	const token = params.get('token_id')?.trim().toLowerCase() ?? '';
	return {
		query: {
			window: SCHEDULE_WINDOWS.includes(window as RentScheduleWindow)
				? (window as RentScheduleWindow)
				: '24h',
			mode: SCHEDULE_MODES.includes(mode as RentScheduleMode) ? (mode as RentScheduleMode) : 'all',
			...(token ? { token_id: token } : {})
		},
		error: token && !HASH.test(token) ? 'Enter a complete 64-character hexadecimal token ID.' : null
	};
}

export function scheduleIssue(error: unknown): string {
	if (error instanceof ApiError) {
		if (error.status === 404)
			return 'This server does not provide the rent calendar yet. The Eligible tab remains available.';
		if (error.status === 409)
			return 'The indexed chain changed. Refresh the schedule to start a new anchored snapshot.';
		return error.detail;
	}
	return error instanceof Error ? error.message : 'The rent schedule could not be loaded.';
}

const integer = (value: number) => Number.isSafeInteger(value) && value >= 0;
const amount = (value: string) => typeof value === 'string' && UINT.test(value);
const timestamp = (value: number) => integer(value) && value <= 8_640_000_000_000_000;

/** Reject incoherent amounts and anchors before presenting a collectible opportunity. */
export function validateSchedule(
	page: RentSchedulePageDto,
	query: ScheduleQuery
): RentSchedulePageDto {
	const context = page.schedule_context;
	const invalid = () => {
		throw new Error('The server returned an inconsistent rent schedule. Refresh to try again.');
	};
	if (
		!page.anchor ||
		!integer(page.anchor.height) ||
		!HASH.test(page.anchor.block_id) ||
		page.observed_anchor?.height !== page.anchor.height ||
		page.observed_anchor?.block_id !== page.anchor.block_id ||
		page.consistency !== 'strict' ||
		!context ||
		context.scope !== 'indexed_unspent_rent_window' ||
		context.from_height !== page.anchor.height + 1 ||
		!integer(context.to_height) ||
		context.to_height < context.from_height ||
		context.target_block_seconds !== 120 ||
		!timestamp(context.anchor_timestamp_ms) ||
		context.window !== query.window ||
		context.mode !== query.mode ||
		context.token_id !== (query.token_id ?? null) ||
		!Array.isArray(page.items) ||
		page.items.length > 100 ||
		typeof context.full_history !== 'boolean' ||
		typeof context.complete !== 'boolean' ||
		typeof context.scan_limit_reached !== 'boolean' ||
		!integer(context.scanned) ||
		context.scan_limit !== 2000 ||
		context.scanned > context.scan_limit ||
		context.scanned < page.items.length ||
		(context.partial_from !== null && !integer(context.partial_from)) ||
		(context.full_history && context.partial_from !== null) ||
		(context.scan_limit_reached &&
			(context.scanned !== context.scan_limit || page.next_cursor === null)) ||
		context.batch_scan_limit !== 50_000 ||
		!integer(context.rent_factor) ||
		context.fee_arithmetic !== 'wrapping_i32' ||
		context.complete !== (page.next_cursor === null) ||
		(page.next_cursor !== null && (!page.next_cursor || !page.next_snapshot))
	)
		invalid();
	const ids = new Set<string>();
	let previousHeight = context.from_height;
	for (const item of page.items) {
		if (
			!HASH.test(item.box_id) ||
			ids.has(item.box_id) ||
			!HASH.test(item.tx_id) ||
			!integer(item.maturity_height) ||
			item.maturity_height < previousHeight ||
			item.maturity_height > context.to_height ||
			!timestamp(item.estimated_maturity_ms) ||
			item.estimated_maturity_ms !==
				context.anchor_timestamp_ms + (item.maturity_height - page.anchor!.height) * 120_000 ||
			!amount(item.value) ||
			!amount(item.collectible_due_nano) ||
			!/^(-?(0|[1-9][0-9]{0,9}))$/.test(item.consensus_fee_nano) ||
			!Array.isArray(item.tokens)
		)
			invalid();
		const fee = BigInt(item.consensus_fee_nano);
		const value = BigInt(item.value);
		const due = fee <= 0n || item.protocol_constrained ? 0n : fee < value ? fee : value;
		if (
			fee < -2147483648n ||
			fee > 2147483647n ||
			typeof item.protocol_constrained !== 'boolean' ||
			item.collectible !== (fee > 0n && !item.protocol_constrained) ||
			BigInt(item.collectible_due_nano) !== due ||
			item.full_claim !== (fee > 0n && fee >= value && !item.protocol_constrained) ||
			(query.mode === 'collectible' && !item.collectible) ||
			(query.mode === 'full_claim' && !item.full_claim)
		)
			invalid();
		const tokens = new Set<string>();
		for (const token of item.tokens) {
			if (
				!HASH.test(token.id) ||
				tokens.has(token.id) ||
				!amount(token.amount) ||
				BigInt(token.amount) === 0n ||
				(token.decimals !== null && (!integer(token.decimals) || token.decimals > 255)) ||
				(token.name !== null && typeof token.name !== 'string')
			)
				invalid();
			tokens.add(token.id);
		}
		if (query.token_id && !tokens.has(query.token_id)) invalid();
		ids.add(item.box_id);
		previousHeight = item.maturity_height;
	}
	if (page.batches !== null) {
		if (!Array.isArray(page.batches) || page.batches.length > 2161) invalid();
		if (
			typeof context.batch_complete !== 'boolean' ||
			context.batch_scanned === null ||
			!integer(context.batch_scanned) ||
			context.batch_scanned > context.batch_scan_limit ||
			(context.batch_complete
				? context.batch_stop_reason !== null
				: !['candidate_limit', 'decode_limit', 'work_limit', 'deadline'].includes(
						context.batch_stop_reason ?? ''
					))
		)
			invalid();
		let previousHour = -1;
		let totalBoxes = 0;
		for (const batch of page.batches) {
			if (
				!timestamp(batch.estimated_hour_start_ms) ||
				batch.estimated_hour_start_ms % HOUR !== 0 ||
				batch.estimated_hour_start_ms <= previousHour ||
				!integer(batch.box_count) ||
				!integer(batch.full_claim_count) ||
				batch.full_claim_count > batch.box_count ||
				!amount(batch.collectible_due_nano) ||
				!amount(batch.full_claim_value_nano) ||
				BigInt(batch.full_claim_value_nano) > BigInt(batch.collectible_due_nano) ||
				(query.token_id
					? !amount(batch.selected_token_full_claim_amount!)
					: batch.selected_token_full_claim_amount !== null)
			)
				invalid();
			previousHour = batch.estimated_hour_start_ms;
			totalBoxes += batch.box_count;
		}
		if (totalBoxes > context.batch_scanned!) invalid();
	} else if (
		context.batch_complete !== null ||
		context.batch_scanned !== null ||
		context.batch_stop_reason !== null
	)
		invalid();
	return page;
}

export interface ScheduleBatch extends RentScheduleBatchDto {
	hours: RentScheduleBatchDto[];
}

/** UTC calendar buckets. Never use Number to add exact chain amounts. */
export type ScheduleRanking = 'rent' | 'boxes' | 'full_claims' | 'token';
export function scheduleBatches(
	batches: RentScheduleBatchDto[],
	window: RentScheduleWindow,
	ranking: ScheduleRanking = 'rent'
): ScheduleBatch[] {
	const buckets = new Map<number, ScheduleBatch>();
	for (const hour of batches) {
		const start =
			window === '24h'
				? hour.estimated_hour_start_ms
				: Math.floor(hour.estimated_hour_start_ms / DAY) * DAY;
		let bucket = buckets.get(start);
		if (!bucket) {
			bucket = {
				estimated_hour_start_ms: start,
				box_count: 0,
				collectible_due_nano: '0',
				full_claim_count: 0,
				full_claim_value_nano: '0',
				selected_token_full_claim_amount:
					hour.selected_token_full_claim_amount === null ? null : '0',
				hours: []
			};
			buckets.set(start, bucket);
		}
		bucket.box_count += hour.box_count;
		bucket.full_claim_count += hour.full_claim_count;
		bucket.collectible_due_nano = (
			BigInt(bucket.collectible_due_nano) + BigInt(hour.collectible_due_nano)
		).toString();
		bucket.full_claim_value_nano = (
			BigInt(bucket.full_claim_value_nano) + BigInt(hour.full_claim_value_nano)
		).toString();
		if (
			bucket.selected_token_full_claim_amount !== null &&
			hour.selected_token_full_claim_amount !== null
		)
			bucket.selected_token_full_claim_amount = (
				BigInt(bucket.selected_token_full_claim_amount) +
				BigInt(hour.selected_token_full_claim_amount)
			).toString();
		bucket.hours.push(hour);
	}
	const score = (batch: ScheduleBatch) =>
		ranking === 'boxes'
			? BigInt(batch.box_count)
			: ranking === 'full_claims'
				? BigInt(batch.full_claim_count)
				: ranking === 'token'
					? BigInt(batch.selected_token_full_claim_amount ?? '0')
					: BigInt(batch.collectible_due_nano);
	return [...buckets.values()].sort((a, b) => {
		const difference = score(b) - score(a);
		return difference < 0n
			? -1
			: difference > 0n
				? 1
				: a.estimated_hour_start_ms - b.estimated_hour_start_ms;
	});
}

export function scheduleTotals(batches: RentScheduleBatchDto[]) {
	return batches.reduce(
		(sum, batch) => ({
			boxes: sum.boxes + batch.box_count,
			due: sum.due + BigInt(batch.collectible_due_nano),
			fullClaims: sum.fullClaims + batch.full_claim_count,
			fullValue: sum.fullValue + BigInt(batch.full_claim_value_nano),
			selectedToken: sum.selectedToken + BigInt(batch.selected_token_full_claim_amount ?? '0')
		}),
		{ boxes: 0, due: 0n, fullClaims: 0, fullValue: 0n, selectedToken: 0n }
	);
}

export interface ScheduleState {
	first: RentSchedulePageDto | null;
	items: RentScheduleItemDto[];
	cursor: string | null;
	snapshot: string | null;
	scanned: number;
	scanLimited: boolean;
	busy: boolean;
	stale: boolean;
	error: string | null;
}
export const emptySchedule = (): ScheduleState => ({
	first: null,
	items: [],
	cursor: null,
	snapshot: null,
	scanned: 0,
	scanLimited: false,
	busy: false,
	stale: false,
	error: null
});
type ReadSchedule = (
	query: ScheduleQuery,
	cursor: string | null,
	snapshot: string | null,
	signal: AbortSignal,
	limit: number
) => Promise<RentSchedulePageDto>;

export function createScheduleLoader(
	publish: (state: ScheduleState) => void,
	read: ReadSchedule = (query, cursor, snapshot, signal, limit) =>
		api.rentSchedule(query, cursor, snapshot, limit, (input, init) =>
			fetch(input, {
				...init,
				signal: AbortSignal.any([signal, ...(init?.signal ? [init.signal] : [])])
			})
		)
) {
	let query: ScheduleQuery = { window: '24h', mode: 'all' };
	let current = emptySchedule();
	let generation = 0;
	let controller: AbortController | null = null;
	const emit = (patch: Partial<ScheduleState>) => {
		current = { ...current, ...patch };
		publish(current);
	};
	function stop() {
		generation++;
		controller?.abort();
		controller = null;
	}
	function seed(next: ScheduleQuery, page: RentSchedulePageDto | null, error: string | null) {
		stop();
		query = { ...next };
		current = emptySchedule();
		try {
			if (page) {
				validateSchedule(page, query);
				if (page.batches === null)
					throw new Error('The first schedule page is missing its batch overview.');
				emit({
					first: page,
					items: page.items,
					cursor: page.next_cursor,
					snapshot: page.next_snapshot ?? null,
					scanned: page.schedule_context.scanned,
					scanLimited: page.schedule_context.scan_limit_reached,
					error
				});
			} else emit({ error });
		} catch (cause) {
			emit({ error: scheduleIssue(cause) });
		}
	}
	async function load(more = false) {
		if (current.busy || (more && (!current.cursor || current.items.length >= SCHEDULE_ROW_LIMIT)))
			return;
		const token = ++generation;
		controller = new AbortController();
		const cursor = more ? current.cursor : null;
		emit({ busy: true, error: null });
		try {
			const limit = more ? Math.min(100, SCHEDULE_ROW_LIMIT - current.items.length) : 100;
			const next = validateSchedule(
				await read(query, cursor, more ? current.snapshot : null, controller.signal, limit),
				query
			);
			if (token !== generation) return;
			if (next.items.length > limit)
				throw new Error('The server exceeded the requested schedule row limit.');
			if (more && current.first) {
				if (
					next.anchor?.block_id !== current.first.anchor?.block_id ||
					next.anchor?.height !== current.first.anchor?.height ||
					next.schedule_context.anchor_timestamp_ms !==
						current.first.schedule_context.anchor_timestamp_ms ||
					next.batches !== null ||
					next.next_cursor === cursor ||
					next.items.some((item) => current.items.some((old) => old.box_id === item.box_id)) ||
					(next.items.length &&
						current.items.length &&
						next.items[0].maturity_height < current.items.at(-1)!.maturity_height)
				)
					throw new ApiError(
						409,
						'Snapshot changed',
						'The schedule continuation no longer matches its anchor.'
					);
				emit({
					items: [...current.items, ...next.items],
					cursor: next.next_cursor,
					snapshot: next.next_snapshot ?? current.snapshot,
					scanned: current.scanned + next.schedule_context.scanned,
					scanLimited: next.schedule_context.scan_limit_reached,
					stale: false
				});
			} else {
				if (next.batches === null)
					throw new Error('The first schedule page is missing its batch overview.');
				emit({
					first: next,
					items: next.items,
					cursor: next.next_cursor,
					snapshot: next.next_snapshot ?? null,
					scanned: next.schedule_context.scanned,
					scanLimited: next.schedule_context.scan_limit_reached,
					stale: false
				});
			}
		} catch (error) {
			if (token !== generation) return;
			if (error instanceof ApiError && error.status === 409) current = emptySchedule();
			emit({ error: scheduleIssue(error), stale: current.first !== null });
		} finally {
			if (token === generation) {
				controller = null;
				emit({ busy: false });
			}
		}
	}
	return { seed, refresh: () => load(), more: () => load(true), stop };
}
