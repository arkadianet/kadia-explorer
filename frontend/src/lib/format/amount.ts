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

/** Sums decimal nanoERG amounts via BigInt, e.g. summing a tx's output values. */
export function sumNano(values: string[]): bigint {
	return values.reduce((total, v) => total + BigInt(v), 0n);
}
