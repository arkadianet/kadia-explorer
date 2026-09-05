import { describe, it, expect } from 'vitest';
import { formatKb } from '$lib/format/size';

describe('formatKb', () => {
	it('formats byte counts as kibibytes with one decimal', () => {
		expect(formatKb(1536)).toBe('1.5 KB');
		expect(formatKb(1024)).toBe('1.0 KB');
		expect(formatKb(237)).toBe('0.2 KB');
		expect(formatKb(0)).toBe('0.0 KB');
	});

	it('rounds to the nearest tenth', () => {
		expect(formatKb(1996)).toBe('1.9 KB');
		expect(formatKb(2000)).toBe('2.0 KB');
	});
});
