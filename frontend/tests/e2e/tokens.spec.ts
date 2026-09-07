import { expect, test } from '@playwright/test';
import {
	MOCK_ADDRESS,
	SYNTHETIC_TOKEN,
	UNKNOWN_HEX,
	mostHeldToken,
	newestToken,
	tokenBoxCount,
	tokenCount,
	tokenHolders,
	tokenUnspentBoxCount
} from './data.ts';
import { scrollUntilCount, scrollUntilLoaded, useShortViewport } from './helpers.ts';

/** The first row's token link, which is what a change of sort reorders. */
const firstTokenLink = (page: import('@playwright/test').Page) =>
	page.locator('table.table tbody tr').first().locator('a[href^="/token/"]').first();

test('/tokens lists tokens and the sort toggle reorders them', async ({ page }) => {
	await useShortViewport(page);
	await page.goto('/tokens');

	// How many pages the observer pulled in before this ran is not the point — which token
	// heads the list is, and that is what the sort changes.
	await expect(page.locator('table.table tbody tr').first()).toBeVisible();
	await expect(page.getByRole('columnheader', { name: 'Holders' })).toBeVisible();

	// Default sort is newest, which the synthetic token heads: it is minted into the tip block.
	await expect(firstTokenLink(page)).toHaveAttribute('href', `/token/${newestToken.id}`);
	await expect(page.getByRole('button', { name: 'Newest' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);

	// The sort lives in the query string, and swapping it starts a fresh pager rather than
	// appending the other order's rows to the ones on screen.
	await page.getByRole('button', { name: 'Most held' }).click();
	await expect(page).toHaveURL('/tokens?sort=holders');
	await expect(firstTokenLink(page)).toHaveAttribute('href', `/token/${mostHeldToken.id}`);

	// A reload keeps the sort — that is the point of it being in the URL.
	await page.reload();
	await expect(page.getByRole('button', { name: 'Most held' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await expect(firstTokenLink(page)).toHaveAttribute('href', `/token/${mostHeldToken.id}`);
});

test('/tokens pulls in further pages as the sentinel scrolls into view', async ({ page }) => {
	await useShortViewport(page);
	await page.goto('/tokens');
	await scrollUntilLoaded(page, tokenCount);
});

test('a named token shows its facts, with the decimal point its mint declares', async ({
	page
}) => {
	await page.goto(`/token/${SYNTHETIC_TOKEN.id}`);

	await expect(page.getByRole('heading', { name: SYNTHETIC_TOKEN.name })).toBeVisible();
	await expect(page.getByText('Supply', { exact: true })).toBeVisible();
	// 123456 units at 2 decimals.
	await expect(page.getByText('1,234.56').first()).toBeVisible();
	await expect(page.getByText(SYNTHETIC_TOKEN.description)).toBeVisible();
	await expect(page.getByRole('link', { name: `${newestToken.mint_height}` })).toBeVisible();
});

test('a token page lists its holders and, on the other tab, its boxes', async ({ page }) => {
	// Short viewport, again so each list stops after the mock's first page — see above.
	await useShortViewport(page);
	await page.goto(`/token/${mostHeldToken.id}`);

	// Holders is the default tab and loads on mount; the mock pages by 5, so the full set only
	// arrives once the sentinel has been scrolled through.
	await scrollUntilCount(page, 'table.table tbody tr', tokenHolders.length);
	// The biggest holder, with the share the mock computed for it.
	await expect(page.locator('table.table tbody tr').first()).toContainText('99.94%');

	await page.getByRole('tab', { name: 'Boxes' }).click();
	await expect(page).toHaveURL(`/token/${mostHeldToken.id}#boxes`);

	// Unspent first — "who holds it now" is the question the page is asked.
	await expect(page.getByText('Boxes still holding the token')).toBeVisible();
	await scrollUntilCount(page, 'article.box', tokenUnspentBoxCount);

	// "All" is a different query, so it restarts the list rather than appending to it — and it
	// is a strictly larger set, spent boxes included.
	await page.getByRole('button', { name: 'All', exact: true }).click();
	await expect(page.getByText(`${tokenBoxCount.toLocaleString('en-US')} boxes ever`)).toBeVisible();
	await scrollUntilCount(page, 'article.box', tokenBoxCount);
});

test('an id with no mint row renders the token-not-found state', async ({ page }) => {
	await page.goto(`/token/${UNKNOWN_HEX}`);

	await expect(page.getByRole('heading', { name: 'Token' })).toBeVisible();
	await expect(page.getByText(/Token not found/)).toBeVisible();
});

test('a token id resolves through the API to the token page', async ({ page }) => {
	await page.goto('/');
	const input = page.getByRole('searchbox');
	await input.fill(mostHeldToken.id);
	await input.press('Enter');

	await expect(page).toHaveURL(`/token/${mostHeldToken.id}`);
	await expect(page.getByRole('tab', { name: 'Holders' })).toBeVisible();
});

test('an address page names the tokens it holds and scales their amounts', async ({ page }) => {
	await page.goto(`/address/${MOCK_ADDRESS}`);

	const named = page.getByRole('link', { name: SYNTHETIC_TOKEN.name });
	await expect(named).toBeVisible();
	await expect(named).toHaveAttribute('href', `/token/${SYNTHETIC_TOKEN.id}`);
	await expect(page.getByText('1,234.56')).toBeVisible();
});

test('the home page top-tokens card lists five tokens', async ({ page }) => {
	await page.goto('/');

	await expect(page.getByRole('heading', { name: 'Top tokens' })).toBeVisible();
	await expect(page.locator('.tokens li')).toHaveCount(5);
	// The card renders rows, not the failed-request state.
	await expect(page.locator('.error')).toHaveCount(0);
	await expect(page.getByRole('link', { name: 'All tokens' })).toBeVisible();
});
