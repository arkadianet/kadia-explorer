import { expect, test } from '@playwright/test';
import { BLOCK_HEIGHTS, TIP } from './data.ts';

test('home renders the hero and every panel with rows from the mock', async ({ page }) => {
	await page.goto('/');

	// The hero states the chain's tip, and the panels below it name their own sections.
	await expect(
		page.getByRole('heading', { level: 1, name: 'Transparent by design.' })
	).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Recent blocks' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Live transactions' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Rent maturing soon' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Largest holders' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Go deeper' })).toBeVisible();

	// The tip block heads the "Recent blocks" table.
	const blocks = page.locator('section.panel', {
		has: page.getByRole('heading', { name: 'Recent blocks' })
	});
	await expect(blocks.locator('tbody tr').first().locator('td').first()).toHaveText(String(TIP));
	// The mock serves 13 blocks; the panel shows the newest six of them.
	await expect(blocks.locator('tbody tr')).toHaveCount(6);
	for (const h of BLOCK_HEIGHTS) {
		await expect(blocks.getByRole('link', { name: String(h), exact: true })).toBeVisible();
	}

	// Live transactions is a classified list, not a table: one row per transaction, each
	// naming what kind of transaction it is.
	const txs = page.locator('section.panel', {
		has: page.getByRole('heading', { name: 'Live transactions' })
	});
	await expect(txs.locator('.txlist li')).toHaveCount(5);
	await expect(txs.locator('.tx-kind').first()).toHaveText(
		/Storage rent claim|Token transfer|Payment/
	);

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
