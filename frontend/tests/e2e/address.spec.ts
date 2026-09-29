import { expect, test } from '@playwright/test';
import { MOCK_ADDRESS, UNKNOWN_ADDRESS } from './data.ts';

test('a shared mobile rent link reveals its selected tab without scrolling the page', async ({
	page
}) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto(`/address/${MOCK_ADDRESS}#rent`);
	const rentTab = page.getByRole('tab', { name: 'Rent', exact: true });
	await expect(rentTab).toHaveAttribute('aria-selected', 'true');
	await expect(rentTab).toBeInViewport({ ratio: 1 });
	await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(0);
	await rentTab.press('Home');
	await expect(page.getByRole('tab', { name: 'Activity', exact: true })).toBeInViewport({
		ratio: 1
	});
	await expect(page.getByRole('tab', { name: 'Activity', exact: true })).toBeFocused();
});

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
	await expect(page.locator('.address-balance')).toBeVisible();
	await expect(page.getByText('Boxes', { exact: true })).toBeVisible();
	const holdings = page.locator('details.address-holdings');
	await expect(holdings).not.toHaveAttribute('open', '');
	await holdings.locator('summary').click();
	await expect(holdings.getByRole('columnheader', { name: 'Amount', exact: true })).toBeVisible();
	await holdings.locator('summary').click();

	// The compact legacy transaction list remains available beside address activity.
	await page.getByRole('tab', { name: 'Transactions', exact: true }).click();
	const rows = page.getByRole('tabpanel').locator('table.table tbody tr');
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

	// Rent evidence scans only after an explicit request.
	await page.getByRole('tab', { name: 'Rent' }).click();
	await expect(page).toHaveURL(`/address/${MOCK_ADDRESS}#rent`);
	await page.getByRole('button', { name: 'Load rent exposure' }).click();
	await expect(
		page.getByRole('list', { name: 'Scanned rent boxes' }).locator('li').first()
	).toBeVisible();
});
