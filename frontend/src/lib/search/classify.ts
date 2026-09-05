export type Classified =
	| { kind: 'height'; value: number }
	| { kind: 'hex32'; value: string }
	| { kind: 'address'; value: string }
	| { kind: 'unknown'; value: string };

const HEIGHT_RE = /^\d{1,9}$/;
const HEX32_RE = /^[0-9a-fA-F]{64}$/;
const ADDRESS_RE = /^[9382][1-9A-HJ-NP-Za-km-z]{30,}$/;

export function classify(q: string): Classified {
	const trimmed = q.trim();

	if (HEIGHT_RE.test(trimmed)) return { kind: 'height', value: Number(trimmed) };
	if (HEX32_RE.test(trimmed)) return { kind: 'hex32', value: trimmed.toLowerCase() };
	if (ADDRESS_RE.test(trimmed)) return { kind: 'address', value: trimmed };

	return { kind: 'unknown', value: trimmed };
}
