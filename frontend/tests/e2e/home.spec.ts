import { expect, test } from '@playwright/test';
import { BLOCK_HEIGHTS, TIP } from './data.ts';

test('home renders the three panels with rows from the mock', async ({ page }) => {
	await page.goto('/');

	await expect(page.getByRole('heading', { name: 'Latest blocks' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Latest transactions' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Rent maturing soon' })).toBeVisible();

	// The tip block heads the "Latest blocks" table.
	const blocks = page.locator('section.panel', {
		has: page.getByRole('heading', { name: 'Latest blocks' })
	});
	await expect(blocks.locator('tbody tr').first().locator('td').first()).toHaveText(String(TIP));
	await expect(blocks.locator('tbody tr')).toHaveCount(5);
	for (const h of BLOCK_HEIGHTS) {
		await expect(blocks.getByRole('link', { name: String(h), exact: true })).toBeVisible();
	}

	const txs = page.locator('section.panel', {
		has: page.getByRole('heading', { name: 'Latest transactions' })
	});
	await expect(txs.locator('tbody tr')).toHaveCount(5);

	const rent = page.locator('section.panel', {
		has: page.getByRole('heading', { name: 'Rent maturing soon' })
	});
	await expect(rent.locator('tbody tr').first()).toBeVisible();

	// "Matures in" counts down from the *indexed* tip (not the node's `best`), so a box that
	// has not matured yet must read as a positive number of blocks — a `best`-based countdown
	// would disagree with /rent and, under lag, could even go negative.
	const maturesIn = rent.locator('tbody tr').first().locator('td').nth(2);
	await expect(maturesIn).toHaveText(/^\d+ blocks$/);
	expect(Number((await maturesIn.innerText()).replace(' blocks', ''))).toBeGreaterThan(0);

	// No lag banner in the default (healthy) mock status.
	await expect(page.getByText(/blocks behind the node/)).toHaveCount(0);
});

test('a block row links through to the block page', async ({ page }) => {
	await page.goto('/');
	await page
		.getByRole('link', { name: String(TIP), exact: true })
		.first()
		.click();
	await expect(page).toHaveURL(`/blocks/${TIP}`);
	await expect(page.getByRole('heading', { name: `Block ${TIP}` })).toBeVisible();
});
