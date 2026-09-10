import { expect, test } from '@playwright/test';

test('rich list labels and uses total genesis allocation including reserves', async ({ page }) => {
	await page.route('**/v1/supply', (route) =>
		route.fulfill({
			json: {
				complete: true,
				genesis_total_nano: '97739925000000000',
				emitted_nano: '4330792500000000'
			}
		})
	);
	await page.route('**/v1/richlist*', (route) =>
		route.fulfill({
			json: {
				items: [{ tree_hash: 'a'.repeat(64), address: null, nano: '93409132500000000' }],
				next_cursor: null
			}
		})
	);
	await page.goto('/richlist');
	await expect(page.getByRole('columnheader', { name: '% of genesis allocation' })).toBeVisible();
	await expect(page.getByText('95.56%', { exact: true })).toBeVisible();
	await expect(page.getByText(/This\s+is not circulating supply/)).toBeVisible();
});

test('rich list does not use mainnet supply when its identity is unavailable', async ({ page }) => {
	await page.route('**/v1/supply', (route) =>
		route.fulfill({ json: { complete: false, genesis_total_nano: '97739925000000000' } })
	);
	await page.route('**/v1/richlist*', (route) =>
		route.fulfill({
			json: {
				items: [{ tree_hash: 'a'.repeat(64), address: null, nano: '1000000000' }],
				next_cursor: null
			}
		})
	);
	await page.goto('/richlist');
	await expect(page.locator('tbody tr').first().locator('td').last()).toHaveText('—');
});
