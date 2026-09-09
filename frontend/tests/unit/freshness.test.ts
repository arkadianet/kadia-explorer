import { expect, test } from 'vitest';
import { health } from '$lib/status/health';
import type { StatusDto } from '$lib/api/types';
const healthy = {
	indexed: 10,
	best: 10,
	lag_blocks: 0,
	stalled: null,
	halted: null,
	mode: 'tip',
	source_observed_at_ms: 1000,
	source_error: null
} as unknown as StatusDto;
test('successful, failed and recovered polls cannot leave stale Live status', () => {
	expect(health(healthy, { lastSuccess: 1000, now: 1000, error: null }).live).toBe('Live');
	expect(health(healthy, { lastSuccess: 1000, now: 6000, error: new Error('offline') }).live).toBe(
		'Unavailable'
	);
	expect(health(healthy, { lastSuccess: 1000, now: 16001, error: null }).live).toBe('Unavailable');
	expect(
		health(
			{ ...healthy, source_observed_at_ms: 20000 },
			{ lastSuccess: 20000, now: 20000, error: null }
		).live
	).toBe('Live');
});
test('a stale or failed source observation is unavailable even at zero lag', () => {
	expect(
		health(
			{ ...healthy, source_error: 'source down' },
			{ lastSuccess: 1000, now: 1000, error: null }
		).live
	).toBe('Unavailable');
	expect(health(healthy, { lastSuccess: 20000, now: 20000, error: null }).live).toBe('Unavailable');
});
