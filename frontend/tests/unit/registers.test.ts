import { describe, it, expect } from 'vitest';
import { decodeRegister } from '$lib/registers/decode';

describe('decodeRegister', () => {
	it('decodes a printable Coll[Byte] as a quoted UTF-8 string', () => {
		expect(decodeRegister('0e0568656c6c6f')).toEqual({ type: 'Coll[Byte]', value: '"hello"' });
	});

	it('decodes a non-printable Coll[Byte] as hex', () => {
		expect(decodeRegister('0e03ff0001')).toEqual({ type: 'Coll[Byte]', value: 'ff0001' });
	});

	it('decodes an empty Coll[Byte]', () => {
		expect(decodeRegister('0e00')).toEqual({ type: 'Coll[Byte]', value: '""' });
	});

	it('decodes Int constants with zigzag VLQ', () => {
		expect(decodeRegister('0400')).toEqual({ type: 'Int', value: '0' });
		expect(decodeRegister('0402')).toEqual({ type: 'Int', value: '1' });
		expect(decodeRegister('0401')).toEqual({ type: 'Int', value: '-1' });
		// 0xac 0x02 -> VLQ 300 -> zigzag 150
		expect(decodeRegister('04ac02')).toEqual({ type: 'Int', value: '150' });
	});

	it('decodes Long constants with zigzag VLQ', () => {
		expect(decodeRegister('0502')).toEqual({ type: 'Long', value: '1' });
		expect(decodeRegister('0501')).toEqual({ type: 'Long', value: '-1' });
		// zigzag(1000000000) = 2000000000, VLQ 80 a8 d6 b9 07 — exact only through BigInt
		expect(decodeRegister('0580a8d6b907')).toEqual({ type: 'Long', value: '1000000000' });
	});

	it('decodes Boolean constants', () => {
		expect(decodeRegister('0100')).toEqual({ type: 'Boolean', value: 'false' });
		expect(decodeRegister('0101')).toEqual({ type: 'Boolean', value: 'true' });
	});

	it('decodes a GroupElement', () => {
		const point = '02' + 'ab'.repeat(32);
		expect(decodeRegister('07' + point)).toEqual({ type: 'GroupElement', value: point });
	});

	it('decodes a P2PK SigmaProp', () => {
		const point = '03' + 'cd'.repeat(32);
		expect(decodeRegister('08cd' + point)).toEqual({ type: 'SigmaProp', value: point });
	});

	it('falls back to raw for unknown type prefixes', () => {
		expect(decodeRegister('63ff00')).toEqual({ type: 'raw', value: '63ff00' });
	});

	it('returns null for non-hex or empty input', () => {
		expect(decodeRegister('zz')).toBeNull();
		expect(decodeRegister('')).toBeNull();
		expect(decodeRegister('0e0')).toBeNull();
	});

	it('returns null for truncated input', () => {
		expect(decodeRegister('0e05ab')).toBeNull();
		expect(decodeRegister('04')).toBeNull();
		expect(decodeRegister('05ff')).toBeNull();
		expect(decodeRegister('01')).toBeNull();
		expect(decodeRegister('07' + '02'.repeat(10))).toBeNull();
		expect(decodeRegister('08cd' + '02'.repeat(10))).toBeNull();
	});
});
