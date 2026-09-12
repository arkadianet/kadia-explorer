import { expect, test } from '@playwright/test';
import { MOCK_ADDRESS, UNKNOWN_ADDRESS } from './data.ts';

test('an address the index has never seen renders the not-seen state', async ({ page }) => {
	await page.goto(`/address/${UNKNOWN_ADDRESS}`);

	await expect(page.getByRole('heading', { name: 'Address' })).toBeVisible();
	await expect(page.getByText(UNKNOWN_ADDRESS)).toBeVisible();
	await expect(page.getByText(/Address not seen yet/)).toBeVisible();
});

test('a known address shows its balance and per-tab lists', async ({ page }) => {
	await page.goto(`/address/${MOCK_ADDRESS}`);

	await expect(page.getByRole('heading', { name: 'Address' })).toBeVisible();
	// Balance, box count and the first/last-seen heights all come from the mock aggregate.
	await expect(page.locator('.balance')).toBeVisible();
	await expect(page.getByText('Boxes', { exact: true })).toBeVisible();

	// Transactions tab is the default and loads on mount.
	const rows = page.locator('table.table tbody tr');
	await expect(rows.first()).toBeVisible();

	// Unspent boxes tab: hash-driven, so it survives a reload.
	await page.getByRole('tab', { name: 'Unspent boxes' }).click();
	await expect(page).toHaveURL(`/address/${MOCK_ADDRESS}#unspent`);
	await expect(page.getByRole('columnheader', { name: 'Value' })).toBeVisible();
	await expect(rows.first()).toBeVisible();
	await expect(page.locator('p.sum')).toContainText('Total');

	await page.reload();
	await expect(page.getByRole('tab', { name: 'Unspent boxes' })).toHaveAttribute(
		'aria-selected',
		'true'
	);

	// Rent tab lists the same unspent boxes with their maturity.
	await page.getByRole('tab', { name: 'Rent' }).click();
	await expect(page).toHaveURL(`/address/${MOCK_ADDRESS}#rent`);
	await expect(page.locator('table.table tbody tr').first()).toBeVisible();
});

for (const truncated of [true, false]) {
	test(`address_rent_${truncated ? 'truncated_shows_sample_notice' : 'complete_hides_sample_notice'}`, async ({
		page
	}) => {
		await page.route(`**/v1/addresses/${MOCK_ADDRESS}/rent`, async (route) => {
			// Read the body to completion BEFORE fulfilling, and fulfil from the captured
			// text rather than handing the live APIResponse back: passing `response`
			// alongside `json` lets Playwright dispose it mid-read under parallel load,
			// which failed this test intermittently ("Response has been disposed") while
			// passing every time in isolation.
			const response = await route.fetch();
			const status = response.status();
			const data = JSON.parse(await response.text());
			await route.fulfill({ status, json: { ...data, truncated } });
		});
		await page.goto(`/address/${MOCK_ADDRESS}#rent`);
		await expect(page.locator('table.table tbody tr').first()).toBeVisible();
		const notice = page.locator('p.notice');
		if (truncated) {
			await expect(notice).toHaveText(
				'Showing a partial sample of this address’s rent-bearing boxes, sorted by maturity among those scanned; boxes not shown may mature sooner.'
			);
		} else {
			await expect(notice).toHaveCount(0);
		}
	});
}
