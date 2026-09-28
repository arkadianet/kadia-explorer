import type { AddressActivityDto, AddressActivityFilters, PageDto } from '$lib/api/types';

export const MAX_ACTIVITY_ITEMS = 500;
export const MAX_CSV_ROWS = 5000;
export const DIRECTIONS = ['all', 'received', 'sent', 'mixed', 'neutral', 'unknown'] as const;

/** Aggregates only the rows supplied by the current filtered, anchored view. Unknown
 * input coverage withholds the entire ERG aggregate, never just the missing terms. */
export function loadedActivitySummary(items: AddressActivityDto[]) {
	const directions = { received: 0, sent: 0, mixed: 0, neutral: 0, unknown: 0 };
	let nano = 0n;
	let unresolved = 0;
	let fromMs: number | null = null;
	let toMs: number | null = null;
	for (const item of items) {
		directions[item.direction]++;
		fromMs = fromMs === null ? item.timestamp : Math.min(fromMs, item.timestamp);
		toMs = toMs === null ? item.timestamp : Math.max(toMs, item.timestamp);
		if (
			!item.coverage.complete ||
			item.coverage.resolved_inputs !== item.coverage.total_inputs ||
			item.erg_delta === null
		)
			unresolved++;
		else nano += BigInt(item.erg_delta);
	}
	return {
		count: items.length,
		nano: unresolved ? null : nano.toString(),
		unresolved,
		directions,
		fromMs,
		toMs
	};
}

export function parseActivityFilters(params: URLSearchParams): {
	filters: AddressActivityFilters;
	error: string | null;
} {
	const asset = params.get('asset')?.trim().toLowerCase() || 'all';
	const direction = params.get('direction') || 'all';
	if (asset !== 'all' && asset !== 'erg' && !/^[a-f0-9]{64}$/.test(asset))
		return { filters: {}, error: 'Choose ERG, all assets, or a 64-character token ID.' };
	if (!DIRECTIONS.includes(direction as (typeof DIRECTIONS)[number]))
		return { filters: {}, error: 'Choose a valid activity direction.' };
	const filters: AddressActivityFilters = {
		asset,
		direction: direction as AddressActivityFilters['direction']
	};
	for (const key of ['from', 'to'] as const) {
		const raw = params.get(key);
		if (!raw) continue;
		const ms = Date.parse(`${raw}T00:00:00.000Z`);
		if (
			!/^\d{4}-\d{2}-\d{2}$/.test(raw) ||
			!Number.isFinite(ms) ||
			ms < 0 ||
			new Date(ms).toISOString().slice(0, 10) !== raw
		)
			return { filters: {}, error: 'Use valid calendar dates from 1970 onward in UTC.' };
		if (key === 'from') filters.from_ms = ms;
		else filters.to_ms = ms + 86_400_000;
	}
	if (
		filters.from_ms !== undefined &&
		filters.to_ms !== undefined &&
		filters.from_ms >= filters.to_ms
	)
		return { filters: {}, error: 'The end date must be on or after the start date.' };
	return { filters, error: null };
}
function cell(raw: string, integer = false) {
	if (integer && raw !== '' && !/^-?\d+$/.test(raw))
		throw new Error('An activity amount is not an exact integer. Refresh before exporting.');
	const safe =
		!integer && (/^[\s\uFEFF]*[=+@-]/u.test(raw) || raw.startsWith('\t') || raw.startsWith('\r'))
			? `'${raw}`
			: raw;
	return `"${safe.replaceAll('"', '""')}"`;
}
export function activityCsv(
	items: AddressActivityDto[],
	address: string,
	label: string,
	anchor: NonNullable<PageDto<unknown>['anchor']>,
	filters: AddressActivityFilters,
	scope: { partialFrom: number | null; matchingScanComplete: boolean }
): string {
	if (!items.length || items.length > MAX_ACTIVITY_ITEMS)
		throw new Error(
			'Export between 1 and 500 loaded transactions. Narrow the filters for a smaller export.'
		);
	if (items.reduce((total, item) => total + 1 + item.tokens.length, 0) > MAX_CSV_ROWS)
		throw new Error('This export exceeds 5,000 asset rows. Narrow the filters before exporting.');
	const headers = [
		'address',
		'local_label',
		'transaction_id',
		'block_id',
		'height',
		'timestamp_utc',
		'direction',
		'coverage_complete',
		'resolved_inputs',
		'total_inputs',
		'asset',
		'token_id',
		'token_name',
		'delta_raw',
		'decimals',
		'asset_match',
		'snapshot_height',
		'snapshot_block_id',
		'filter_asset',
		'filter_direction',
		'from_utc_inclusive',
		'to_utc_exclusive',
		'export_scope',
		'loaded_transaction_count',
		'indexed_partial_from',
		'matching_scan_complete'
	];
	const lines = [headers.map((value) => cell(value)).join(',')];
	const encoder = new TextEncoder();
	let bytes = encoder.encode(lines[0] + '\r\n').length;
	for (const item of items) {
		const assets = [{ id: '', name: 'ERG', decimals: 9, delta: item.erg_delta }, ...item.tokens];
		for (const asset of assets) {
			const values = [
				address,
				label,
				item.id,
				item.block_id,
				String(item.height),
				new Date(item.timestamp).toISOString(),
				item.direction,
				String(item.coverage.complete),
				String(item.coverage.resolved_inputs),
				String(item.coverage.total_inputs),
				asset.id ? 'token' : 'ERG',
				asset.id,
				asset.name ?? '',
				item.coverage.complete ? (asset.delta ?? '') : '',
				asset.decimals === null ? '' : String(asset.decimals),
				item.asset_match,
				String(anchor.height),
				anchor.block_id,
				filters.asset ?? 'all',
				filters.direction ?? 'all',
				filters.from_ms === undefined ? '' : new Date(filters.from_ms).toISOString(),
				filters.to_ms === undefined ? '' : new Date(filters.to_ms).toISOString(),
				'loaded_transactions_only',
				String(items.length),
				scope.partialFrom === null ? '' : String(scope.partialFrom),
				String(scope.matchingScanComplete)
			];
			const line = values.map((value, index) => cell(value, index === 13)).join(',');
			bytes += encoder.encode(line + '\r\n').length;
			if (bytes > 2 * 1024 * 1024)
				throw new Error('This export exceeds 2 MB. Narrow the filters before exporting.');
			lines.push(line);
		}
	}
	const csv = lines.join('\r\n') + '\r\n';
	return csv;
}
