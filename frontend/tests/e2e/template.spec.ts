import { expect, test } from '@playwright/test';
import { UNKNOWN_HEX, template } from './data.ts';
import { scrollUntilCount, useShortViewport } from './helpers.ts';

test('a script template shows its facts and its unspent boxes', async ({ page }) => {
	// Short viewport so the infinite list stops after one mock page; this template's boxes all
	// fit in one either way, but the assertion should not depend on that.
	await useShortViewport(page);
	await page.goto(`/template/${template.hash}`);

	await expect(page.getByRole('heading', { name: 'Script template' })).toBeVisible();
	await expect(page.getByText(template.hash)).toBeVisible();

	// Facts straight off the mock aggregate.
	await expect(page.getByText('Unspent', { exact: true }).first()).toBeVisible();
	await expect(
		page.getByRole('link', { name: String(template.first_seen), exact: true })
	).toBeVisible();
	await expect(page.getByRole('link', { name: /^9mockAddress/ })).toBeVisible();

	// Unspent is the default filter; "All" is a different query and restarts the list.
	await expect(page.getByText(`${template.unspent_count} unspent`)).toBeVisible();
	await scrollUntilCount(page, 'article.box', template.unspent_count);

	await page.getByRole('button', { name: 'All', exact: true }).click();
	await expect(page.getByText(`${template.box_count} ever`)).toBeVisible();
	await scrollUntilCount(page, 'article.box', template.box_count);
});

test('a hash no indexed box is locked by renders the not-found state', async ({ page }) => {
	await page.goto(`/template/${UNKNOWN_HEX}`);

	await expect(page.getByRole('heading', { name: 'Script template' })).toBeVisible();
	await expect(page.getByText(/Template not found/)).toBeVisible();
});

test('a template hash resolves through the API to the template page', async ({ page }) => {
	await page.goto('/');
	const input = page.getByRole('searchbox');
	await input.fill(template.hash);
	await input.press('Enter');

	await expect(page).toHaveURL(`/template/${template.hash}`);
	await expect(page.getByRole('heading', { name: 'Script template' })).toBeVisible();
});
