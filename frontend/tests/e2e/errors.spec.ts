import { expect, test } from '@playwright/test';
import { TIP } from './data.ts';
import { useShortViewport } from './helpers.ts';

// `x-mock-fail` / `x-mock-lag` are honoured by the mock API on every /v1 route — see the
// header block in tests/e2e/mock/handlers.ts.

test('a 500 from the API renders ErrorState, and Retry recovers', async ({ page }) => {
	// Short viewport: with the sentinel below the fold, the recovered list settles on exactly
	// the first page instead of racing further ones in.
	await useShortViewport(page);
	await page.setExtraHTTPHeaders({ 'x-mock-fail': '500' });
	await page.goto('/blocks');

	const error = page.getByRole('alert').first();
	await expect(error).toBeVisible();
	await expect(error).toContainText('500 Internal Server Error');
	const retry = page.getByRole('button', { name: 'Retry' });
	await expect(retry).toBeVisible();

	// Heal the API, then retry from the button.
	await page.setExtraHTTPHeaders({});
	await retry.click();
	await expect(page.locator('table.table tbody tr').first()).toBeVisible();
	await expect(page.getByRole('button', { name: 'Retry' })).toHaveCount(0);
	// At least the mock's first page of five came back, tip first; the rest wait on the
	// sentinel, so this stays an inequality rather than a race against further pages.
	const rows = page.locator('table.table tbody tr');
	expect(await rows.count()).toBeGreaterThanOrEqual(5);
	await expect(rows.first().locator('td').first()).toHaveText(String(TIP));
});

test('a lagging index raises the home-page banner', async ({ page }) => {
	await page.setExtraHTTPHeaders({ 'x-mock-lag': '500' });
	await page.goto('/');

	const banner = page.locator('.banner.warn');
	await expect(banner).toBeVisible();
	await expect(banner).toContainText('Index is 500 blocks behind the node.');
});
