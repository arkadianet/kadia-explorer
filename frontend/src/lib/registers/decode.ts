// Minimal decoder for the common Ergo register constants. Registers arrive from the node as
// hex of the *serialized* constant: a type code followed by the value's bytes. We decode the
// handful of shapes an explorer actually shows (byte arrays, ints, longs, booleans, group
// elements, P2PK sigma props) and fall back to raw hex for everything else — a full ErgoTree
// deserializer does not belong in the frontend bundle.

export interface DecodedRegister {
	type: string;
	value: string;
}

/** The additional registers a box may carry, in display order. */
export const REGISTER_KEYS = ['R4', 'R5', 'R6', 'R7', 'R8', 'R9'] as const;

/** True when a box's `registers` map has at least one non-empty R4–R9 entry to show. */
export function hasDisplayableRegisters(registers: Record<string, unknown> | null): boolean {
	if (!registers) return false;
	return REGISTER_KEYS.some((key) => typeof registers[key] === 'string' && registers[key] !== '');
}

/** Type codes we recognise; see sigmastate `SType`/`ConstantSerializer`. */
const TYPE_BOOLEAN = 0x01;
const TYPE_INT = 0x04;
const TYPE_LONG = 0x05;
const TYPE_GROUP_ELEMENT = 0x07;
const TYPE_SIGMA_PROP = 0x08;
const TYPE_COLL_BYTE = 0x0e;

/** Compressed EC point / P2PK public key length. */
const POINT_BYTES = 33;

function hexToBytes(hex: string): Uint8Array | null {
	if (hex.length === 0 || hex.length % 2 !== 0 || !/^[0-9a-fA-F]+$/.test(hex)) return null;
	const out = new Uint8Array(hex.length / 2);
	for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
	return out;
}

function bytesToHex(bytes: Uint8Array): string {
	let s = '';
	for (const b of bytes) s += b.toString(16).padStart(2, '0');
	return s;
}

interface Vlq {
	value: bigint;
	next: number;
}

/** Unsigned VLQ: 7 bits per byte, little-endian, high bit marks "more bytes follow".
 * Returns null when the value runs off the end of the buffer. */
function readVlq(bytes: Uint8Array, offset: number): Vlq | null {
	let value = 0n;
	let shift = 0n;
	let i = offset;
	// A 64-bit value needs at most 10 continuation bytes; anything longer is malformed.
	for (; i < bytes.length && i - offset < 10; i++) {
		const b = bytes[i];
		value |= BigInt(b & 0x7f) << shift;
		if ((b & 0x80) === 0) return { value, next: i + 1 };
		shift += 7n;
	}
	return null;
}

/** Zigzag: even numbers map to non-negatives, odd to negatives — `(n >>> 1) ^ -(n & 1)`. */
function zigzag(n: bigint): bigint {
	return (n >> 1n) ^ -(n & 1n);
}

function isPrintableAscii(bytes: Uint8Array): boolean {
	if (bytes.length === 0) return true;
	return bytes.every((b) => b >= 0x20 && b <= 0x7e);
}

/**
 * Decodes one serialized register constant.
 *
 * Returns `null` when the input is not hex or is truncated, and
 * `{ type: 'raw', value: hex }` for constants outside the recognised set.
 */
export function decodeRegister(hex: string): DecodedRegister | null {
	const bytes = hexToBytes(hex);
	if (bytes === null || bytes.length < 1) return null;

	const code = bytes[0];

	switch (code) {
		case TYPE_BOOLEAN: {
			if (bytes.length < 2) return null;
			const b = bytes[1];
			if (b !== 0 && b !== 1) return null;
			return { type: 'Boolean', value: b === 1 ? 'true' : 'false' };
		}

		case TYPE_INT:
		case TYPE_LONG: {
			const vlq = readVlq(bytes, 1);
			if (vlq === null) return null;
			return {
				type: code === TYPE_INT ? 'Int' : 'Long',
				value: zigzag(vlq.value).toString()
			};
		}

		case TYPE_GROUP_ELEMENT: {
			if (bytes.length < 1 + POINT_BYTES) return null;
			return { type: 'GroupElement', value: bytesToHex(bytes.slice(1, 1 + POINT_BYTES)) };
		}

		case TYPE_SIGMA_PROP: {
			// Only the P2PK sigma prop (`08cd` + compressed point) is decoded; other sigma
			// propositions (AND/OR trees, DHTuple) fall through to raw.
			if (bytes.length >= 2 && bytes[1] === 0xcd) {
				if (bytes.length < 2 + POINT_BYTES) return null;
				return { type: 'SigmaProp', value: bytesToHex(bytes.slice(2, 2 + POINT_BYTES)) };
			}
			break;
		}

		case TYPE_COLL_BYTE: {
			const len = readVlq(bytes, 1);
			if (len === null) return null;
			const start = len.next;
			const end = start + Number(len.value);
			if (len.value > BigInt(bytes.length) || end > bytes.length) return null;
			const data = bytes.slice(start, end);
			return {
				type: 'Coll[Byte]',
				value: isPrintableAscii(data) ? `"${new TextDecoder().decode(data)}"` : bytesToHex(data)
			};
		}
	}

	return { type: 'raw', value: hex.toLowerCase() };
}
