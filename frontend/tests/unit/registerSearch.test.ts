import { describe, expect, it } from 'vitest';
import {
	checkRegisterValue,
	parseRegisterKey,
	parseRegisterQuery
} from '../../src/lib/registers/search.ts';

describe('parseRegisterKey', () => {
	it('accepts R4 through R9', () => {
		for (const key of ['R4', 'R5', 'R6', 'R7', 'R8', 'R9']) {
			expect(parseRegisterKey(key)).toBe(key);
		}
	});

	it('canonicalises case and surrounding space', () => {
		expect(parseRegisterKey('r7')).toBe('R7');
		expect(parseRegisterKey('  r4  ')).toBe('R4');
	});

	it('rejects the mandatory registers, a bare digit and nonsense', () => {
		expect(parseRegisterKey('R0')).toBeNull();
		expect(parseRegisterKey('R3')).toBeNull();
		expect(parseRegisterKey('R10')).toBeNull();
		expect(parseRegisterKey('4')).toBeNull();
		expect(parseRegisterKey('RR4')).toBeNull();
	});

	it('rejects absent input rather than throwing', () => {
		expect(parseRegisterKey(null)).toBeNull();
		expect(parseRegisterKey(undefined)).toBeNull();
		expect(parseRegisterKey('')).toBeNull();
	});
});

describe('checkRegisterValue', () => {
	it('accepts even-length hex and lower-cases it', () => {
		expect(checkRegisterValue('0E0A')).toEqual({ ok: true, hex: '0e0a' });
	});

	it('drops a 0x prefix', () => {
		expect(checkRegisterValue('0x0e0a')).toEqual({ ok: true, hex: '0e0a' });
		expect(checkRegisterValue('0X0E0A')).toEqual({ ok: true, hex: '0e0a' });
	});

	it('trims surrounding whitespace from a pasted value', () => {
		expect(checkRegisterValue('  0e0a\n')).toEqual({ ok: true, hex: '0e0a' });
	});

	it('rejects an empty value', () => {
		const r = checkRegisterValue('   ');
		expect(r.ok).toBe(false);
		expect(r.ok === false && r.error).toMatch(/Enter a register value/);
	});

	it('rejects an odd number of hex characters, naming the count', () => {
		const r = checkRegisterValue('0e0');
		expect(r.ok).toBe(false);
		expect(r.ok === false && r.error).toMatch(/even number of them — this one has 3/);
	});

	it('rejects non-hex characters', () => {
		const r = checkRegisterValue('0e0z');
		expect(r.ok).toBe(false);
		expect(r.ok === false && r.error).toMatch(/Hex only/);
	});

	it('rejects a bare 0x prefix', () => {
		expect(checkRegisterValue('0x').ok).toBe(false);
	});
});

describe('parseRegisterQuery', () => {
	it('reads a usable lookup out of the query string', () => {
		expect(parseRegisterQuery('r5', '0E0A')).toEqual({ reg: 'R5', value: '0e0a' });
	});

	it('is null when either half is missing', () => {
		expect(parseRegisterQuery(null, '0e0a')).toBeNull();
		expect(parseRegisterQuery('R5', null)).toBeNull();
		expect(parseRegisterQuery(null, null)).toBeNull();
	});

	it('is null for a register or value the API would reject', () => {
		expect(parseRegisterQuery('R3', '0e0a')).toBeNull();
		expect(parseRegisterQuery('R5', '0e0')).toBeNull();
		expect(parseRegisterQuery('R5', '')).toBeNull();
	});
});
