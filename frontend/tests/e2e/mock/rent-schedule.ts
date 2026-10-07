/** Synthetic upcoming boxes, not captured live opportunities. Keep their calendar anchor
 * stable for the mock server's lifetime so continuations share the same observation. */
import type {
	RentScheduleBatchDto,
	RentScheduleItemDto,
	RentScheduleMode,
	RentSchedulePageDto,
	RentScheduleWindow,
	TokenDto
} from '../../../src/lib/api/types.ts';
import { FIXTURE_HEIGHTS, type Dataset } from './fixtures.ts';

export const RENT_CALENDAR_TOKEN = 'c1'.repeat(32);
export const RENT_CALENDAR_UNKNOWN_TOKEN = 'c2'.repeat(32);
export const RENT_CALENDAR_NFT = 'c3'.repeat(32);
export const RENT_CALENDAR_LARGE_AMOUNT = '9007199254740993';
export const RENT_CALENDAR_REEMISSION =
	'd9a2cc8a09abfaed87afacfbb7daee79a6b26f10c6613fc13d3f3953e5521d1a';
const ANCHOR_TIME = Math.floor(Date.now() / 3_600_000) * 3_600_000;
const WINDOWS = { '24h': 720, '7d': 5_040, '30d': 21_600, '90d': 64_800 } as const;
const MODES: RentScheduleMode[] = ['all', 'collectible', 'full_claim'];
const HEX64 = /^[0-9a-f]{64}$/;

function token(id: string, amount: string): TokenDto {
	return {
		id,
		amount,
		name:
			id === RENT_CALENDAR_TOKEN
				? 'Calendar demo token'
				: id === RENT_CALENDAR_NFT
					? 'Calendar demo collectible'
					: id === RENT_CALENDAR_REEMISSION
						? 'Re-emission obligation'
						: null,
		decimals: id === RENT_CALENDAR_TOKEN ? 2 : id === RENT_CALENDAR_UNKNOWN_TOKEN ? null : 0
	};
}

export function rentScheduleFixture(
	height: number,
	timestamp = ANCHOR_TIME
): RentScheduleItemDto[] {
	const items: RentScheduleItemDto[] = [];
	const add = (offset: number, value: bigint, tokens: TokenDto[], size = 105) => {
		const fee = BigInt.asIntN(32, BigInt(size) * 1_250_000n);
		const constrained = tokens.some((entry) => entry.id === RENT_CALENDAR_REEMISSION);
		const collectible = fee > 0n && !constrained;
		const index = items.length + 1;
		items.push({
			box_id: 'ca'.repeat(28) + index.toString(16).padStart(8, '0'),
			tx_id: 'cb'.repeat(28) + index.toString(16).padStart(8, '0'),
			creation_height: height + offset - 1_051_200,
			maturity_height: height + offset,
			estimated_maturity_ms: timestamp + offset * 120_000,
			value: value.toString(),
			consensus_fee_nano: fee.toString(),
			collectible_due_nano: collectible ? (fee < value ? fee : value).toString() : '0',
			collectible,
			full_claim: collectible && fee >= value,
			protocol_constrained: constrained,
			tokens
		});
	};
	// The first item page cannot contain the peak: it is eight later boxes in one hour.
	add(1, 1_000_000n, [token(RENT_CALENDAR_TOKEN, '101')]);
	add(5, 10_000_000_000n, [token(RENT_CALENDAR_TOKEN, '202')]);
	add(10, 2_000_000n, [token(RENT_CALENDAR_UNKNOWN_TOKEN, '7')]);
	add(20, 3_000_000n, [token(RENT_CALENDAR_NFT, '1')]);
	add(40, 10_000_000_000n, [token(RENT_CALENDAR_TOKEN, '303')], 2_008);
	const burst = (offset: number, count: number, value: bigint, large = false) => {
		for (let i = 0; i < count; i++) {
			add(offset + i, value, [
				token(RENT_CALENDAR_TOKEN, large && i === 0 ? RENT_CALENDAR_LARGE_AMOUNT : String(100 + i))
			]);
		}
	};
	burst(180, 8, 50_000_000n, true);
	add(500, 63_000_000_000n, [token(RENT_CALENDAR_REEMISSION, '12000000000')], 135);
	add(600, 131_250_000n, [token(RENT_CALENDAR_TOKEN, '1000')]); // Fee equality is full consumption.
	add(620, 2_000_000n, [token(RENT_CALENDAR_UNKNOWN_TOKEN, '9')], 3_436); // Second wrap is positive.
	burst(720 + 180, 10, 75_000_000n);
	burst(1_440 + 180, 8, 60_000_000n);
	burst(1_440 + 240, 8, 60_000_000n); // Two separated hours make the larger daily batch.
	burst(13 * 720 + 180, 12, 80_000_000n);
	burst(59 * 720 + 180, 15, 90_000_000n);
	return items;
}

export function rentScheduleResponse(
	d: Dataset,
	url: URL,
	timestamp = ANCHOR_TIME
): { status: number; body: RentSchedulePageDto | Record<string, unknown> } {
	const fail = (status: number, code: string, detail: string) => ({
		status,
		body: {
			type: 'about:blank',
			title: status === 409 ? 'Snapshot changed' : 'Bad Request',
			status,
			code,
			detail
		}
	});
	const window = (url.searchParams.get('window') ?? '24h') as RentScheduleWindow;
	const mode = (url.searchParams.get('mode') ?? 'all') as RentScheduleMode;
	const tokenId = url.searchParams.get('token_id')?.toLowerCase() ?? null;
	const rawLimit = url.searchParams.get('limit') ?? '100';
	const limit = Number(rawLimit);
	if (
		!Object.hasOwn(WINDOWS, window) ||
		!MODES.includes(mode) ||
		(tokenId !== null && !HEX64.test(tokenId)) ||
		!/^\d+$/.test(rawLimit) ||
		!Number.isInteger(limit) ||
		limit < 1 ||
		limit > 100 ||
		![null, 'strict'].includes(url.searchParams.get('consistency'))
	)
		return fail(400, 'invalid_request', 'Invalid rent schedule window, mode, token or pagination.');
	const height = d.status.indexed;
	const block = height === null ? undefined : d.blockByHeight.get(height);
	if (!block)
		return {
			status: 503,
			body: {
				status: 503,
				title: 'Service Unavailable',
				code: 'rent_schedule_unavailable',
				detail: 'No indexed fixture anchor.'
			}
		};
	const anchor = { height: block.height, block_id: block.id };
	const cursor = url.searchParams.get('cursor');
	const provided = url.searchParams.get('snapshot');
	const binding = [anchor, timestamp, window, mode, tokenId];
	const snapshotFor = (next: string) =>
		'mock-rent-schedule-' + Buffer.from(JSON.stringify([...binding, next])).toString('base64url');
	if ((cursor === null) !== (provided === null))
		return fail(400, 'invalid_request', 'A rent continuation requires both cursor and snapshot.');
	if (cursor !== null && provided !== snapshotFor(cursor)) {
		let saved: unknown;
		try {
			saved = JSON.parse(
				Buffer.from((provided ?? '').replace(/^mock-rent-schedule-/, ''), 'base64url').toString()
			);
		} catch {
			return fail(400, 'invalid_request', 'Malformed rent snapshot.');
		}
		if (
			!Array.isArray(saved) ||
			saved.length !== 6 ||
			JSON.stringify(saved.slice(2)) !== JSON.stringify([window, mode, tokenId, cursor])
		)
			return fail(
				400,
				'invalid_request',
				'The rent snapshot is bound to different filters or cursor.'
			);
		return fail(409, 'snapshot_changed', 'Restart the calendar with the current snapshot.');
	}
	const candidates = rentScheduleFixture(height!, timestamp).filter(
		(item) => item.maturity_height <= height! + WINDOWS[window]
	);
	const cursorFor = (item: RentScheduleItemDto) =>
		`${item.maturity_height}:${parseInt(item.box_id.slice(-8), 16)}`;
	const after = cursor === null ? -1 : candidates.findIndex((item) => cursorFor(item) === cursor);
	if (cursor !== null && after < 0)
		return fail(400, 'invalid_request', 'The rent cursor is outside this fixture window.');
	const matches = (item: RentScheduleItemDto) =>
		(mode === 'all' || (mode === 'collectible' ? item.collectible : item.full_claim)) &&
		(tokenId === null || item.tokens.some((entry) => entry.id === tokenId));
	const items: RentScheduleItemDto[] = [];
	let scanned = 0;
	let last = after;
	for (let index = after + 1; index < candidates.length; index++) {
		scanned++;
		last = index;
		if (matches(candidates[index])) items.push(candidates[index]);
		if (items.length === Math.min(limit, 5)) break;
	}
	const more = last + 1 < candidates.length;
	const next = more ? cursorFor(candidates[last]) : null;
	let batches: RentScheduleBatchDto[] | null = null;
	if (cursor === null) {
		const byHour = new Map<number, RentScheduleBatchDto>();
		for (const item of candidates.filter(matches)) {
			const hour = Math.floor(item.estimated_maturity_ms / 3_600_000) * 3_600_000;
			const total = byHour.get(hour) ?? {
				estimated_hour_start_ms: hour,
				box_count: 0,
				collectible_due_nano: '0',
				full_claim_count: 0,
				full_claim_value_nano: '0',
				selected_token_full_claim_amount: tokenId === null ? null : '0'
			};
			total.box_count++;
			total.collectible_due_nano = (
				BigInt(total.collectible_due_nano) + BigInt(item.collectible_due_nano)
			).toString();
			if (item.full_claim) {
				total.full_claim_count++;
				total.full_claim_value_nano = (
					BigInt(total.full_claim_value_nano) + BigInt(item.value)
				).toString();
				if (tokenId !== null)
					total.selected_token_full_claim_amount = (
						BigInt(total.selected_token_full_claim_amount!) +
						item.tokens
							.filter((entry) => entry.id === tokenId)
							.reduce((sum, entry) => sum + BigInt(entry.amount), 0n)
					).toString();
			}
			byHour.set(hour, total);
		}
		batches = [...byHour.values()];
	}
	return {
		status: 200,
		body: {
			items,
			next_cursor: next,
			next_snapshot: next === null ? null : snapshotFor(next),
			consistency: 'strict',
			anchor,
			observed_anchor: anchor,
			schedule_context: {
				scope: 'indexed_unspent_rent_window',
				anchor_timestamp_ms: timestamp,
				from_height: height! + 1,
				to_height: height! + WINDOWS[window],
				target_block_seconds: 120,
				window,
				mode,
				token_id: tokenId,
				full_history: false,
				partial_from: FIXTURE_HEIGHTS[0],
				scanned,
				scan_limit: 2_000,
				scan_limit_reached: false,
				complete: next === null,
				batch_scanned: cursor === null ? candidates.length : null,
				batch_scan_limit: 50_000,
				batch_complete: cursor === null ? true : null,
				batch_stop_reason: null,
				rent_factor: 1_250_000,
				fee_arithmetic: 'wrapping_i32'
			},
			batches
		}
	};
}
