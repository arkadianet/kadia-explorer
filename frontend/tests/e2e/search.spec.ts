import { expect, test } from '@playwright/test';
import { MOCK_ADDRESS, UNKNOWN_HEX, block, claimableBox, tx } from './data.ts';

/** Types a query into the header search box and submits it. */
async function search(page: import('@playwright/test').Page, q: string) {
	const input = page.getByRole('searchbox');
	await input.fill(q);
	await input.press('Enter');
}

test('a height goes straight to the block page', async ({ page }) => {
	await page.goto('/');
	await search(page, String(block.height));
	await expect(page).toHaveURL(`/blocks/${block.height}`);
	await expect(page.getByRole('heading', { name: `Block ${block.height}` })).toBeVisible();
});

test('a transaction id resolves through the API to the tx page', async ({ page }) => {
	await page.goto('/');
	await search(page, tx.id);
	await expect(page).toHaveURL(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Transaction', exact: true })).toBeVisible();
});

test('a box id resolves through the API to the box page', async ({ page }) => {
	await page.goto('/');
	await search(page, claimableBox.id);
	await expect(page).toHaveURL(`/box/${claimableBox.id}`);
	await expect(page.getByRole('heading', { name: 'Box', exact: true })).toBeVisible();
});

test('a block id resolves through the API to the block page', async ({ page }) => {
	await page.goto('/');
	await search(page, block.id);
	await expect(page).toHaveURL(`/blocks/${block.id}`);
	await expect(page.getByRole('heading', { name: `Block ${block.height}` })).toBeVisible();
});

test('an address goes straight to the address page', async ({ page }) => {
	await page.goto('/');
	await search(page, MOCK_ADDRESS);
	await expect(page).toHaveURL(`/address/${MOCK_ADDRESS}`);
	await expect(page.getByRole('heading', { name: 'Address' })).toBeVisible();
});

test('an unparseable query lands on the search page with a format hint', async ({ page }) => {
	await page.goto('/');
	await search(page, 'not a real query!');
	await expect(page).toHaveURL(/\/search\?q=/);
	await expect(page.getByText(/No match for "not a real query!"/)).toBeVisible();
	await expect(page.getByText(/Enter a block height, an id or an address\./)).toBeVisible();
});

test('a well-formed but unknown id reports not found', async ({ page }) => {
	await page.goto(`/search?q=${UNKNOWN_HEX}`);
	await expect(page.getByText(/64-hex ids can be block, transaction or box ids/)).toBeVisible();
});

test('pressing "/" focuses the search input', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByRole('heading', { name: 'Latest blocks' })).toBeVisible();
	await page.locator('body').click();
	await page.keyboard.press('/');
	await expect(page.getByRole('searchbox')).toBeFocused();
	// The key press focuses rather than typing into the field.
	await expect(page.getByRole('searchbox')).toHaveValue('');
});

test('/search has its own search box, and only the header owns #global-search', async ({
	page
}) => {
	await page.goto('/search');
	// Two boxes on the page (header + in-panel), but the id — and the "/" shortcut that goes
	// with it — belongs to exactly one of them.
	await expect(page.getByRole('searchbox')).toHaveCount(2);
	await expect(page.locator('#global-search')).toHaveCount(1);
	await expect(page.locator('#search-page-search')).toHaveCount(1);
});

test('Escape clears the search box it is typed into', async ({ page }) => {
	await page.goto('/');
	const input = page.getByRole('searchbox');
	await input.fill('9abc');
	await expect(input).toHaveValue('9abc');
	await input.press('Escape');
	await expect(input).toHaveValue('');
	await expect(input).not.toBeFocused();
	// Escape is a no-op for navigation.
	await expect(page).toHaveURL('/');
});

test('the in-page search box on /search still submits', async ({ page }) => {
	await page.goto('/search');
	const input = page.locator('#search-page-search');
	await input.fill(String(block.height));
	await input.press('Enter');
	await expect(page).toHaveURL(`/blocks/${block.height}`);
});
