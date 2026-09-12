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

test('chain change clears visible rows and requires Restart before showing only new rows', async ({
	page
}) => {
	const oldId = 'a'.repeat(64);
	const newId = 'b'.repeat(64);
	const summary = {
		id: oldId,
		height: newestTx.height,
		timestamp: newestTx.timestamp,
		index: 0,
		size: 100,
		input_count: 1,
		output_count: 1,
		data_input_count: 0,
		fee: '0'
	};
	let calls = 0;
	await page.route('**/v1/tx-summaries?*', (route) => {
		calls++;
		const url = new URL(route.request().url());
		expect(url.searchParams.get('consistency')).toBe('strict');
		if (calls === 1)
			return route.fulfill({
				json: {
					items: [summary],
					next_cursor: '1',
					next_snapshot: 'aa'
				}
			});
		if (calls === 2) {
			expect(url.searchParams.get('snapshot')).toBe('aa');
			return route.fulfill({ status: 409, json: { detail: 'anchor changed' } });
		}
		expect(url.searchParams.has('cursor')).toBe(false);
		expect(url.searchParams.has('snapshot')).toBe(false);
		return route.fulfill({ json: { items: [{ ...summary, id: newId }], next_cursor: null } });
	});
	await page.goto('/txs');
	await expect(page.getByRole('alert')).toContainText('The chain changed.');
	await expect(page.locator('table tbody tr')).toHaveCount(0);
	await page.evaluate(() => window.dispatchEvent(new Event('scroll')));
	expect(calls).toBe(2);
	await page.getByRole('button', { name: 'Restart', exact: true }).click();
	await expect(page.locator('table tbody tr')).toHaveCount(1);
	await expect(page.locator('table tbody a').first()).toHaveAttribute('href', `/tx/${newId}`);
	await expect(page.locator(`a[href="/tx/${oldId}"]`)).toHaveCount(0);
	expect(calls).toBe(3);
});
