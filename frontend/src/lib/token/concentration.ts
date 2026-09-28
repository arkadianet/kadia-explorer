import type { TokenHolderContext, TokenHolderDto } from '$lib/api/types';

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
