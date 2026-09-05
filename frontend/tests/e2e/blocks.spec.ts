import { expect, test } from '@playwright/test';
import { BLOCK_COUNT, TIP, block, blockTxCount } from './data.ts';
import { scrollUntilLoaded, useShortViewport } from './helpers.ts';

test('/blocks paginates past the first page as the sentinel scrolls in', async ({ page }) => {
	const pages: string[] = [];
	page.on('request', (r) => {
		const u = new URL(r.url());
		if (u.pathname === '/v1/blocks') pages.push(u.searchParams.get('cursor') ?? '');
	});

	await useShortViewport(page);
	await page.goto('/blocks');
	await expect(page.getByRole('heading', { name: 'Blocks' })).toBeVisible();

	const rows = page.locator('table.table tbody tr');
	await expect(rows.first()).toBeVisible();
	await expect(rows.first().locator('td').first()).toHaveText(String(TIP));

	// The mock pages by 5, so reaching every block takes several sentinel intersections.
	await scrollUntilLoaded(page, BLOCK_COUNT);
	// More than one page was fetched, and at least one request carried a cursor.
	expect(pages.length).toBeGreaterThan(1);
	expect(pages.some((c) => c !== '')).toBe(true);
	// Once the last page came back, the "Load more" footer is gone.
	await expect(page.getByRole('button', { name: 'Load more' })).toHaveCount(0);
});

test('/blocks/1866001 shows the header facts and its transactions', async ({ page }) => {
	await page.goto(`/blocks/${block.height}`);

	await expect(page.getByRole('heading', { name: `Block ${block.height}` })).toBeVisible();
	const facts = page.locator('.facts').first();
	await expect(facts).toContainText(String(block.height));
	await expect(facts).toContainText(String(block.version));
	await expect(facts).toContainText(String(block.tx_count));
	await expect(page.locator(`[title="${block.id}"]`).first()).toBeVisible();
	await expect(page.locator(`[title="${block.reward} nanoERG"]`).first()).toBeVisible();

	await expect(page.getByRole('heading', { name: 'Transactions' })).toBeVisible();
	const txRows = page
		.locator('section.panel', {
			has: page.getByRole('heading', { name: 'Transactions' })
		})
		.locator('tbody tr');
	await expect(txRows).toHaveCount(blockTxCount);

	// Prev/next navigation.
	await page.getByRole('link', { name: /Prev/ }).click();
	await expect(page).toHaveURL(`/blocks/${block.height - 1}`);
});

test('an unknown block height renders the 404 error page', async ({ page }) => {
	await page.goto('/blocks/99999999');
	await expect(page.getByRole('heading', { name: '404' })).toBeVisible();
	await expect(page.getByText('Block not found')).toBeVisible();
});
