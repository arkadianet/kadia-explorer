export type Classified =
	| { kind: 'height'; value: number }
	| { kind: 'hex32'; value: string }
	| { kind: 'address'; value: string }
	| { kind: 'unknown'; value: string };

const HEIGHT_RE = /^\d{1,9}$/;
const HEX32_RE = /^[0-9a-fA-F]{64}$/;
// Candidate only. The API validates the network, checksum and script before routing.
// Even very short P2S scripts have addresses; a prefix whitelist loses valid scripts.
const ADDRESS_RE = /^[1-9A-HJ-NP-Za-km-z]{7,}$/;

export function classify(q: string): Classified {
	const trimmed = q.trim();

	if (HEIGHT_RE.test(trimmed)) return { kind: 'height', value: Number(trimmed) };
	if (HEX32_RE.test(trimmed)) return { kind: 'hex32', value: trimmed.toLowerCase() };
	if (ADDRESS_RE.test(trimmed)) return { kind: 'address', value: trimmed };

	return { kind: 'unknown', value: trimmed };
}

/**
 * Pure routing decision for a classified query. Returns the destination path when the
 * kind alone determines it (height); address candidates require server validation.
 */
export function routeFor(c: Classified): string | null {
	switch (c.kind) {
		case 'height':
			return `/blocks/${c.value}`;
		case 'address':
		case 'hex32':
		case 'unknown':
			return null;
	}
}
