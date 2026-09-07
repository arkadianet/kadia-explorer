import { expect, test } from '@playwright/test';
import { UNKNOWN_REGISTER_VALUE, registerLookup } from './data.ts';
import { scrollUntilCount, useShortViewport } from './helpers.ts';

const RESULT_URL = `/search?reg=${registerLookup.reg}&value=${registerLookup.value}`;

test('the register form finds the boxes carrying a value and puts the lookup in the URL', async ({
	page
}) => {
	// The lookup returns at most one mock page, and a short viewport keeps it that way.
	await useShortViewport(page);
	await page.goto('/search');

	await page.getByLabel('Register').selectOption(registerLookup.reg);
	await page.getByLabel('Value').fill(registerLookup.value);
	await page.getByRole('button', { name: 'Find boxes' }).click();

	// The lookup goes into the query string, which is what makes the result list linkable.
	await expect(page).toHaveURL(RESULT_URL);

	await scrollUntilCount(page, 'article.box', registerLookup.boxIds.length);
	await expect(
		page.locator(`article.box a[href="/box/${registerLookup.boxIds[0]}"]`)
	).toBeVisible();
});

test('a linked register lookup renders straight from the URL, form and all', async ({ page }) => {
	await useShortViewport(page);
	await page.goto(RESULT_URL);

	await expect(page.getByLabel('Value')).toHaveValue(registerLookup.value);
	await expect(page.getByLabel('Register')).toHaveValue(registerLookup.reg);
	await scrollUntilCount(page, 'article.box', registerLookup.boxIds.length);
});

test('a value no box carries is an empty result, not an error', async ({ page }) => {
	await page.goto(`/search?reg=R5&value=${UNKNOWN_REGISTER_VALUE}`);

	await expect(page.getByText('No boxes carry this value.')).toBeVisible();
	await expect(page.locator('.error')).toHaveCount(0);
});

test('a malformed value is rejected beside the field, without a request', async ({ page }) => {
	await page.goto('/search');
	const value = page.getByLabel('Value');

	// Not hex at all.
	await value.fill('zzzz');
	await page.getByRole('button', { name: 'Find boxes' }).click();
	await expect(page.getByRole('alert')).toContainText('Hex only');
	await expect(page).toHaveURL('/search');
	await expect(value).toHaveAttribute('aria-invalid', 'true');

	// Hex, but half a byte short.
	await value.fill('0e0a4');
	await page.getByRole('button', { name: 'Find boxes' }).click();
	await expect(page.getByRole('alert')).toContainText('even number of them');
	await expect(page).toHaveURL('/search');
});
