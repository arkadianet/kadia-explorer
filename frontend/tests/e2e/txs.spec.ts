import { expect, test } from '@playwright/test';
import { newestTx, txCount } from './data.ts';
import { scrollUntilLoaded, useShortViewport } from './helpers.ts';

test('/txs lists transactions newest first and pages through them', async ({ page }) => {
	const cursors: string[] = [];
	const fullRequests: string[] = [];
	page.on('request', (r) => {
		const u = new URL(r.url());
		if (u.pathname === '/v1/txs') fullRequests.push(r.url());
		if (u.pathname === '/v1/tx-summaries') cursors.push(u.searchParams.get('cursor') ?? '');
	});

	await useShortViewport(page);
	await page.goto('/txs');
	await expect(page.getByRole('heading', { name: 'Transactions' })).toBeVisible();

	const rows = page.locator('table.table tbody tr');
	await expect(rows.first()).toBeVisible();
	// The newest transaction heads the list, linking to its own page.
	await expect(rows.first().getByRole('link').first()).toHaveAttribute(
		'href',
		`/tx/${newestTx.id}`
	);

	await scrollUntilLoaded(page, txCount);
	expect(fullRequests).toEqual([]);
	expect(cursors.length).toBeGreaterThan(1);
	expect(cursors.some((c) => c !== '')).toBe(true);
	await expect(page.getByRole('button', { name: 'Load more' })).toHaveCount(0);
});

test('a row on /txs navigates to the transaction page', async ({ page }) => {
	await page.goto('/txs');
	await page.locator('table.table tbody tr').first().getByRole('link').first().click();
	await expect(page).toHaveURL(`/tx/${newestTx.id}`);
	await expect(page.getByRole('heading', { name: 'Transaction', exact: true })).toBeVisible();
});

test('summary list preserves a fee above the safe integer range', async ({ page }) => {
	await page.route('**/v1/tx-summaries?*', (route) =>
		route.fulfill({
			json: {
				items: [
					{
						id: newestTx.id,
						height: newestTx.height,
						timestamp: newestTx.timestamp,
						index: 0,
						size: 100,
						input_count: 7,
						output_count: 9,
						data_input_count: 0,
						fee: '9007199254999999'
					}
				],
				next_cursor: null
			}
		})
	);
	await page.goto('/txs');
	const row = page.locator('table tbody tr').first();
	await expect(row.locator('td').nth(3)).toHaveText('7');
	await expect(row.locator('td').nth(4)).toHaveText('9');
	await expect(row.locator('[title="9007199254999999 nanoERG"]')).toHaveText('9,007,199.254ERG');
});
