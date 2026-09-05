import { expect, test } from '@playwright/test';
import { claimableBox, maturingBox, spentBox } from './data.ts';

test('a claimable box shows the rent panel with the claimable badge', async ({ page }) => {
	await page.goto(`/box/${claimableBox.id}`);

	await expect(page.getByRole('heading', { name: 'Box', exact: true })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Rent' })).toBeVisible();

	const rent = page.locator('section.panel', { has: page.getByRole('heading', { name: 'Rent' }) });
	await expect(rent).toContainText('Maturity height');
	await expect(rent).toContainText(String(claimableBox.rent.maturity_height));
	await expect(rent.locator(`[title="${claimableBox.rent.due_nano} nanoERG"]`)).toBeVisible();
	await expect(rent.getByText('Claimable now')).toBeVisible();
});

test('a maturing box shows the blocks-to-maturity estimate', async ({ page }) => {
	await page.goto(`/box/${maturingBox.id}`);
	const rent = page.locator('section.panel', { has: page.getByRole('heading', { name: 'Rent' }) });
	await expect(rent.getByText(/Matures in [\d,]+ blocks/)).toBeVisible();
	await expect(rent.getByText('Claimable now')).toHaveCount(0);
});

test('a spent box links to the spending transaction', async ({ page }) => {
	await page.goto(`/box/${spentBox.id}`);
	await expect(page.getByRole('heading', { name: 'Box', exact: true })).toBeVisible();
	await expect(page.locator(`[title="${spentBox.value} nanoERG"]`).first()).toBeVisible();
	await expect(page.getByText('Status').first()).toBeVisible();
	await expect(page.getByText('Spent', { exact: true })).toBeVisible();
});

test('an unknown box id renders the 404 error page', async ({ page }) => {
	await page.goto(`/box/${'b'.repeat(64)}`);
	await expect(page.getByRole('heading', { name: '404' })).toBeVisible();
	await expect(page.getByText('Box not found')).toBeVisible();
});
