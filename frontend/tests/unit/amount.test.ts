import { describe, it, expect } from 'vitest';
import { formatErg, formatNano } from '$lib/format/amount';

describe('formatErg', () => {
	it('handles zero and one nanoERG exactly', () => {
		expect(formatErg('0')).toBe('0');
		expect(formatErg('1')).toBe('0.000000001');
	});
	it('formats whole ERG with grouping', () => {
		expect(formatErg('12000000000')).toBe('12');
		expect(formatErg('1412124000000000')).toBe('1,412,124');
	});
	it('keeps exact fractions, trims trailing zeros', () => {
		expect(formatErg('1500000000')).toBe('1.5');
		expect(formatErg('1000000001')).toBe('1.000000001');
	});
	it('never loses precision above 2^53', () => {
		expect(formatErg('93409132500000000')).toBe('93,409,132.5');
	});
	it('maxFrac rounds down (truncates) not up', () => {
		expect(formatErg('1999999999', { maxFrac: 2 })).toBe('1.99');
	});
});

describe('formatNano', () => {
	it('groups', () => expect(formatNano('1250000')).toBe('1,250,000'));
});
