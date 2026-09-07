// The pure half of "find boxes by register value": which register was asked for, and whether
// the typed value is something `/v1/registers/{reg}/{valueHex}/boxes` can be asked about.
// The API rejects a malformed register or value with a 400; validating here means the form
// can say *why* next to the field instead of turning a typo into a failed request.

import { REGISTER_KEYS } from './decode';

/** One of the additional registers a box may carry. */
export type RegisterKey = (typeof REGISTER_KEYS)[number];

/**
 * Canonicalises a register name to `R4`..`R9`, or returns null when it is not one.
 * Case-insensitive (`r7` from a hand-edited URL is the same register as `R7`), but the `R`
 * is required — a bare digit is rejected by the API too, so accepting it here would only
 * move the failure later.
 */
export function parseRegisterKey(raw: string | null | undefined): RegisterKey | null {
	if (!raw) return null;
	const upper = raw.trim().toUpperCase();
	return (REGISTER_KEYS as readonly string[]).includes(upper) ? (upper as RegisterKey) : null;
}

export type RegisterValueCheck = { ok: true; hex: string } | { ok: false; error: string };

/**
 * Validates a typed register value: the serialised sigma constant as hex, exactly the bytes
 * the indexer hashed. A `0x` prefix is accepted and dropped — it is how the value is written
 * in most tooling — and the result is lower-cased so two spellings of one value produce one
 * URL (and so one pager, rather than a second identical query).
 */
export function checkRegisterValue(raw: string): RegisterValueCheck {
	let hex = raw.trim();
	if (hex.startsWith('0x') || hex.startsWith('0X')) hex = hex.slice(2);

	if (hex === '') {
		return { ok: false, error: 'Enter a register value — the serialised constant, in hex.' };
	}
	if (!/^[0-9a-fA-F]+$/.test(hex)) {
		return { ok: false, error: 'Hex only: the digits 0–9 and the letters a–f.' };
	}
	if (hex.length % 2 !== 0) {
		return {
			ok: false,
			error: `A byte is two hex characters, so the value needs an even number of them — this one has ${hex.length}.`
		};
	}
	return { ok: true, hex: hex.toLowerCase() };
}

/** A register lookup that is ready to be sent to the API. */
export interface RegisterQuery {
	reg: RegisterKey;
	/** Lower-case, even-length hex, without a `0x` prefix. */
	value: string;
}

/**
 * Reads a register lookup out of the two query-string parameters, or returns null when the
 * URL carries no lookup (neither present) or an unusable one (a bad register, a value that
 * is not hex). Null means "show the form empty" — a hand-edited URL never puts the page into
 * an error state it cannot explain.
 */
export function parseRegisterQuery(
	reg: string | null | undefined,
	value: string | null | undefined
): RegisterQuery | null {
	const key = parseRegisterKey(reg);
	if (key === null || value == null) return null;
	const checked = checkRegisterValue(value);
	return checked.ok ? { reg: key, value: checked.hex } : null;
}
