import { expect, test } from '@playwright/test';
import { newestTx, txCount } from './data.ts';
import { scrollUntilLoaded, useShortViewport } from './helpers.ts';

test('/txs lists transactions newest first and pages through them', async ({ page }) => {
	const cursors: string[] = [];
	page.on('request', (r) => {
		const u = new URL(r.url());
		if (u.pathname === '/v1/txs') cursors.push(u.searchParams.get('cursor') ?? '');
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
