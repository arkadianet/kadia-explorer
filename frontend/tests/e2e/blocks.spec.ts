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
	await expect(page.getByRole('heading', { name: 'Emission reward explained' })).toBeVisible();

	await expect(page.getByRole('heading', { name: 'Transactions' })).toBeVisible();
	const txRows = page
		.locator('section.panel', {
			has: page.getByRole('heading', { name: 'Transactions' })
		})
		.locator('tbody tr');
	await scrollUntilLoaded(page, blockTxCount);
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

test('block page stays usable when legacy expanded transactions are unavailable', async ({
	page
}) => {
	let expandedRequests = 0;
	await page.route('**/v1/blocks/*/txs', (route) => {
		expandedRequests++;
		return route.fulfill({ status: 422, json: { detail: 'expansion limit' } });
	});
	await page.goto(`/blocks/${block.height}`);
	await expect(page.getByRole('heading', { name: `Block ${block.height}` })).toBeVisible();
	await expect(page.locator('tbody tr').first()).toBeVisible();
	expect(expandedRequests).toBe(0);
});
test('block reward evidence explains gross, obligation and miner subsidy', async ({ page }) => {
	await page.route('**/v1/blocks/*/rewards', (route) =>
		route.fulfill({
			json: {
				block_id: block.id,
				height: block.height,
				basis: 'observed_eip27_reward_box',
				gross_reward: '12000000000',
				reemission_obligation: '9000000000',
				miner_subsidy: '3000000000',
				transaction_fees: block.fees,
				reward_box_id: 'a'.repeat(64),
				transaction_id: 'b'.repeat(64),
				note: 'Observed obligation; fees are separate.'
			}
		})
	);
	await page.goto(`/blocks/${block.height}`);
	const panel = page
		.locator('section.panel')
		.filter({ has: page.getByRole('heading', { name: 'Emission reward explained' }) });
	await expect(panel).toContainText('Gross reward box');
	await expect(panel).toContainText('Re-emission obligation');
	await expect(panel.locator('.net [title="3000000000 nanoERG"]')).toBeVisible();
	await expect(panel.locator('[title="12000000000 nanoERG"]')).toBeVisible();
	await expect(panel.locator('[title="9000000000 nanoERG"]')).toBeVisible();
	await expect(panel.getByRole('link', { name: 'EIP-27 definition' })).toBeVisible();
});
