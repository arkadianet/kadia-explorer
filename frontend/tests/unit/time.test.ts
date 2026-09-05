import { describe, it, expect } from 'vitest';
import { absTime, relTime } from '$lib/format/time';

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Fixed "now" so the thresholds are exercised without depending on the wall clock. */
const NOW = Date.UTC(2026, 0, 2, 3, 4, 5);

/** `relTime(now - delta)` — reads as "this many ms ago". */
const ago = (delta: number) => relTime(NOW - delta, NOW);

describe('relTime', () => {
	it('reports whole seconds below a minute', () => {
		expect(ago(0)).toBe('0 s ago');
		expect(ago(999)).toBe('0 s ago');
		expect(ago(SECOND)).toBe('1 s ago');
		expect(ago(59 * SECOND)).toBe('59 s ago');
	});

	it('switches to minutes at exactly one minute and floors', () => {
		expect(ago(MINUTE)).toBe('1 min ago');
		expect(ago(MINUTE + 59 * SECOND)).toBe('1 min ago');
		expect(ago(59 * MINUTE + 59 * SECOND)).toBe('59 min ago');
	});

	it('switches to hours at exactly one hour and floors', () => {
		expect(ago(HOUR)).toBe('1 h ago');
		expect(ago(HOUR + 59 * MINUTE)).toBe('1 h ago');
		expect(ago(23 * HOUR + 59 * MINUTE)).toBe('23 h ago');
	});

	it('switches to days at exactly one day and floors', () => {
		expect(ago(DAY)).toBe('1 d ago');
		expect(ago(DAY + 23 * HOUR)).toBe('1 d ago');
		expect(ago(365 * DAY)).toBe('365 d ago');
	});

	it('clamps future timestamps to "just now" rather than a negative age', () => {
		expect(relTime(NOW + SECOND, NOW)).toBe('0 s ago');
		expect(relTime(NOW + DAY, NOW)).toBe('0 s ago');
	});

	it('defaults `now` to the current clock', () => {
		expect(relTime(Date.now())).toBe('0 s ago');
	});
});

describe('absTime', () => {
	it('formats a local timestamp as YYYY-MM-DD HH:MM:SS', () => {
		const d = new Date(2026, 0, 2, 3, 4, 5);
		expect(absTime(d.getTime())).toBe('2026-01-02 03:04:05');
	});

	it('zero-pads every field', () => {
		const d = new Date(2026, 10, 20, 19, 30, 59);
		expect(absTime(d.getTime())).toBe('2026-11-20 19:30:59');
	});

	it('matches the Date the milliseconds denote, second-truncated', () => {
		const d = new Date(2026, 5, 15, 12, 0, 0);
		expect(absTime(d.getTime() + 999)).toBe(absTime(d.getTime()));
	});
});
