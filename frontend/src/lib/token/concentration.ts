import type { PageDto, TokenHolderContext, TokenHolderDto } from '$lib/api/types';

const MAX_AMOUNT = (1n << 64n) - 1n;
function amount(raw: string): bigint | null {
	if (!/^(0|[1-9]\d{0,19})$/.test(raw)) return null;
	const value = BigInt(raw);
	return value <= MAX_AMOUNT ? value : null;
}

/** Shares are truncated from integer units, never summed from rounded API percentages. */
export function exactShare(value: bigint, supply: bigint): string {
	if (supply <= 0n || value < 0n || value > supply) return '—';
	if (value === 0n) return '0%';
	const basisPoints = (value * 10_000n) / supply;
	if (basisPoints === 0n) return '<0.01%';
	return `${basisPoints / 100n}.${(basisPoints % 100n).toString().padStart(2, '0')}%`;
}

export function holderConcentration(rows: TokenHolderDto[], context: TokenHolderContext | null) {
	if (!context || context.definition !== 'indexed_emission_minus_burned') return null;
	const supply = amount(context.supply);
	if (
		supply === null ||
		!Number.isSafeInteger(context.holder_count) ||
		context.holder_count < rows.length
	)
		return null;
	let sum = 0n;
	let top = 0n;
	let previous = MAX_AMOUNT;
	const seen = new Set<string>();
	for (const [index, row] of rows.entries()) {
		const units = amount(row.amount);
		if (units === null || units > previous || seen.has(row.tree_hash)) return null;
		seen.add(row.tree_hash);
		previous = units;
		sum += units;
		if (index < 10) top += units;
	}
	if (sum > supply) return null;
	return {
		supply,
		loaded: rows.length,
		total: context.holder_count,
		largest: rows.length ? amount(rows[0].amount)! : 0n,
		top,
		topCount: Math.min(10, rows.length),
		sum,
		topWidth: supply > 0n ? Number((top * 10_000n) / supply) / 100 : 0
	};
}

export const MAX_HOLDER_CSV_ROWS = 5000;
export const MAX_HOLDER_CSV_BYTES = 2 * 1024 * 1024;
export interface HolderExportContext {
	id: string;
	decimals: number | null;
	context: TokenHolderContext | null;
	anchor: PageDto<unknown>['anchor'];
}

export function holderCsvIssue(
	rows: TokenHolderDto[],
	options: HolderExportContext
): string | null {
	if (!rows.length) return 'Load holder rows before exporting.';
	if (rows.length > MAX_HOLDER_CSV_ROWS)
		return 'Export up to 5,000 loaded holder scripts. Reload for a smaller selection.';
	if (
		!options.anchor ||
		!Number.isSafeInteger(options.anchor.height) ||
		options.anchor.height < 0 ||
		options.anchor.height > 4_294_967_295 ||
		!/^[a-fA-F0-9]{64}$/.test(options.anchor.block_id) ||
		!holderConcentration(rows, options.context)
	)
		return 'A consistent holder snapshot and supply denominator are required for export.';
	if (
		!/^[a-fA-F0-9]{64}$/.test(options.id) ||
		(options.decimals !== null &&
			(!Number.isInteger(options.decimals) || options.decimals < 0 || options.decimals > 255)) ||
		rows.some(
			(row) =>
				!/^[a-fA-F0-9]{64}$/.test(row.tree_hash) ||
				(row.address !== null && (typeof row.address !== 'string' || row.address.length > 4096))
		)
	)
		return 'Holder data has invalid export fields. Refresh before exporting.';
	return null;
}

function csvCell(raw: string): string {
	// Quoting alone does not stop spreadsheet formula evaluation. Prefix untrusted
	// formula-like text, while keeping validated integer strings unchanged in the CSV.
	const safe = /^[\s\uFEFF]*[=+@-]/u.test(raw) || /^[\t\r\n]/.test(raw) ? `'${raw}` : raw;
	return `"${safe.replaceAll('"', '""')}"`;
}

/** A local export of the loaded strict page prefix, never a complete owner registry. */
export function holdersCsv(rows: TokenHolderDto[], options: HolderExportContext): string {
	const issue = holderCsvIssue(rows, options);
	if (issue) throw new Error(issue);
	const headers = [
		'token_id',
		'ordinal',
		'address',
		'tree_hash',
		'amount_raw',
		'decimals',
		'indexed_supply_raw',
		'supply_definition',
		'snapshot_height',
		'snapshot_block_id',
		'export_scope',
		'loaded_script_count',
		'indexed_holder_script_count'
	];
	const encoder = new TextEncoder();
	const lines = [headers.map(csvCell).join(',')];
	let bytes = encoder.encode(lines[0] + '\r\n').byteLength;
	for (const [index, row] of rows.entries()) {
		const line = [
			options.id,
			String(index + 1),
			row.address ?? '',
			row.tree_hash,
			row.amount,
			options.decimals === null ? '' : String(options.decimals),
			options.context!.supply,
			options.context!.definition,
			String(options.anchor!.height),
			options.anchor!.block_id,
			'loaded_holder_scripts_only',
			String(rows.length),
			String(options.context!.holder_count)
		]
			.map(csvCell)
			.join(',');
		bytes += encoder.encode(line + '\r\n').byteLength;
		if (bytes > MAX_HOLDER_CSV_BYTES)
			throw new Error('This export exceeds 2 MiB. Reload for a smaller loaded selection.');
		lines.push(line);
	}
	return lines.join('\r\n') + '\r\n';
}
