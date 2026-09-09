import { expect, test } from '@playwright/test';
import { TIP } from './data.ts';

test('/status renders the indexer fields and hides the stall row when healthy', async ({
	page
}) => {
	await page.goto('/status');

	await expect(page.getByRole('heading', { name: 'Indexer status' })).toBeVisible();
	const table = page.locator('table.table');
	await expect(table.getByRole('row', { name: /^Indexed/ })).toContainText(String(TIP));
	await expect(table.getByRole('row', { name: /^Best/ })).toContainText(String(TIP));
	await expect(table.getByRole('row', { name: /^Lag/ })).toContainText('0 blocks');
	await expect(table.getByRole('row', { name: /^Mode/ })).toContainText('tip');
	await expect(table.getByRole('row', { name: /^Source/ })).toContainText('mock');
	await expect(table.getByRole('row', { name: /^Halted/ })).toContainText('—');
	await expect(table.getByRole('row', { name: /^Reads in flight/ })).toContainText('0');
	await expect(table.getByRole('row', { name: /^Rate limited/ })).toContainText('0');

	// `stalled: null` from the mock, so no stall callout.
	await expect(page.locator('.stalled')).toHaveCount(0);
	await expect(page.getByRole('link', { name: 'View raw JSON' })).toBeVisible();
});

test('/status shows the stall callout when the indexer is stuck', async ({ page }) => {
	// `x-mock-stall` makes /v1/status report a non-null `stalled` — see tests/e2e/mock/handlers.ts.
	await page.setExtraHTTPHeaders({ 'x-mock-stall': '1' });
	await page.goto('/status');

	const stalled = page.locator('.stalled');
	await expect(stalled).toBeVisible();
	await expect(stalled).toContainText(String(TIP + 1));
	await expect(stalled).toContainText('137s');
	await expect(stalled).toContainText('node has not served this block yet');
});

test('a failed browser poll removes Live, retains height and recovers', async ({ page }) => {
	await page.clock.install();
	await page.goto('/status');
	await expect(page.locator('a.live')).toHaveText('Live');
	await page.route('**/v1/status', (route) =>
		route.fulfill({ status: 503, json: { detail: 'offline' } })
	);
	await page.clock.fastForward(5000);
	await expect(page.locator('a.live')).toHaveText('Unavailable');
	await expect(page.locator('table.table').getByRole('row', { name: /^Indexed/ })).toContainText(
		String(TIP)
	);
	await page.unroute('**/v1/status');
	await page.clock.fastForward(5000);
	await expect(page.locator('a.live')).toHaveText('Live');
});
