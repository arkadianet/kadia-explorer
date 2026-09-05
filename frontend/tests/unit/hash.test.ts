import { describe, it, expect } from 'vitest';
import { truncateMiddle } from '$lib/format/hash';

describe('truncateMiddle', () => {
	it('truncates a 64-char hex id', () => {
		expect(truncateMiddle('aa44ef6a6c08d0b198d65b762abb0181e6ad995654116fbedaa1d3d3eb95a4d6')).toBe(
			'aa44ef6a…95a4d6'
		);
	});
	it('returns short strings unchanged', () => {
		expect(truncateMiddle('short')).toBe('short');
		expect(truncateMiddle('exactly14chars')).toBe('exactly14chars');
	});
});
