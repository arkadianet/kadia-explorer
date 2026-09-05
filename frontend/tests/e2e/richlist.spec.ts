import { expect, test } from '@playwright/test';
import { MOCK_ADDRESS, richlist } from './data.ts';

test('/richlist ranks addresses by balance, descending', async ({ page }) => {
	await page.goto('/richlist');
	await expect(page.getByRole('heading', { name: 'Rich list' })).toBeVisible();

	const rows = page.locator('table.table tbody tr');
	await expect(rows.first()).toBeVisible();

	// Rank column counts up from 1 …
	const shown = Math.min(await rows.count(), 5);
	for (let i = 0; i < shown; i++) {
		await expect(rows.nth(i).locator('td').first()).toHaveText(String(i + 1));
	}

	// … and the balances (exact nanoERG lives in the Amount title) count down.
	const titles = await rows
		.locator('.amount')
		.evaluateAll((els) => els.map((e) => e.getAttribute('title') ?? ''));
	const nanos = titles.map((t) => BigInt(t.replace(' nanoERG', '')));
	for (let i = 1; i < nanos.length; i++) {
		expect(nanos[i] <= nanos[i - 1]).toBe(true);
	}
	expect(nanos[0]).toBe(BigInt(richlist[0].nano));

	// The richest tree is the one address the mock can resolve, so it links out.
	await expect(rows.first().getByRole('link')).toHaveAttribute('href', `/address/${MOCK_ADDRESS}`);
});
