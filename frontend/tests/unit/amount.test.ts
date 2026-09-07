import { describe, it, expect } from 'vitest';
import { formatErg, formatNano, formatTokenAmount, sumNano } from '$lib/format/amount';

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

describe('formatTokenAmount', () => {
	it('inserts the decimal point', () => {
		expect(formatTokenAmount('12345', 2)).toBe('123.45');
	});
	it('pads a fraction shorter than the decimals count', () => {
		expect(formatTokenAmount('5', 3)).toBe('0.005');
	});
	it('falls back to a grouped integer when decimals is null', () => {
		expect(formatTokenAmount('1250000', null)).toBe('1,250,000');
	});
	it('trims trailing zeros in the fraction', () => {
		expect(formatTokenAmount('1500', 2)).toBe('15');
		expect(formatTokenAmount('1250', 2)).toBe('12.5');
	});
	it('treats zero decimals like a grouped integer', () => {
		expect(formatTokenAmount('42000', 0)).toBe('42,000');
	});
});

describe('sumNano', () => {
	it('sums an empty list to zero', () => {
		expect(sumNano([])).toBe(0n);
	});
	it('sums decimal strings as BigInt', () => {
		expect(sumNano(['1', '2', '3'])).toBe(6n);
	});
	it('never loses precision above 2^53', () => {
		expect(sumNano(['93409132500000000', '1'])).toBe(93409132500000001n);
	});
});
