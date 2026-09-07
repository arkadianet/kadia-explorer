// Amounts arrive from the API as decimal strings — they can exceed Number.MAX_SAFE_INTEGER,
// so every conversion here goes through BigInt, never `Number`.

export const NANO = 1_000_000_000n;

function groupInt(digits: string): string {
	return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

function toBigInt(nano: string | bigint): bigint {
	return typeof nano === 'bigint' ? nano : BigInt(nano);
}

/** Grouped, trimmed ERG amount, e.g. "1,412,124.000000000" -> "1,412,124". `maxFrac` truncates
 * (never rounds) the fractional digits before trailing-zero trimming. */
export function formatErg(nano: string | bigint, opts?: { maxFrac?: number }): string {
	const n = toBigInt(nano);
	const negative = n < 0n;
	const abs = negative ? -n : n;
	const whole = abs / NANO;
	const frac = abs % NANO;
	const sign = negative ? '-' : '';
	const wholeStr = groupInt(whole.toString());

	if (frac === 0n) return `${sign}${wholeStr}`;

	let fracStr = frac.toString().padStart(9, '0');
	if (opts?.maxFrac !== undefined) fracStr = fracStr.slice(0, opts.maxFrac);
	fracStr = fracStr.replace(/0+$/, '');

	return fracStr === '' ? `${sign}${wholeStr}` : `${sign}${wholeStr}.${fracStr}`;
}

/** Grouped integer nanoERG amount, e.g. "1250000" -> "1,250,000". */
export function formatNano(nano: string | bigint): string {
	const n = toBigInt(nano);
	const negative = n < 0n;
	const abs = negative ? -n : n;
	return `${negative ? '-' : ''}${groupInt(abs.toString())}`;
}

/** Grouped token amount, dividing by `10^decimals` and inserting the decimal point, e.g.
 * `("12345", 2)` -> "123.45". `decimals: null` (the store has no mint row for the token) or
 * `0` both fall back to a grouped integer, since there is no fractional part to show. */
export function formatTokenAmount(amount: string, decimals: number | null): string {
	const n = toBigInt(amount);
	if (!decimals) return formatNano(n);

	const negative = n < 0n;
	const abs = negative ? -n : n;
	const divisor = 10n ** BigInt(decimals);
	const whole = abs / divisor;
	const frac = abs % divisor;
	const sign = negative ? '-' : '';
	const wholeStr = groupInt(whole.toString());

	if (frac === 0n) return `${sign}${wholeStr}`;

	const fracStr = frac.toString().padStart(decimals, '0').replace(/0+$/, '');

	return fracStr === '' ? `${sign}${wholeStr}` : `${sign}${wholeStr}.${fracStr}`;
}

/** Sums decimal nanoERG amounts via BigInt, e.g. summing a tx's output values. */
export function sumNano(values: string[]): bigint {
	return values.reduce((total, v) => total + BigInt(v), 0n);
}
