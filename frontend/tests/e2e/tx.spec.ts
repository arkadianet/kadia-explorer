import { expect, test } from '@playwright/test';
import { tx } from './data.ts';

test('a transaction page lists its inputs and outputs', async ({ page }) => {
	await page.goto(`/tx/${tx.id}`);

	await expect(page.getByRole('heading', { name: 'Transaction', exact: true })).toBeVisible();
	await expect(page.getByRole('heading', { name: `Inputs (${tx.inputs.length})` })).toBeVisible();
	await expect(page.getByRole('heading', { name: `Outputs (${tx.outputs.length})` })).toBeVisible();

	// One card per output, each linking to its box page.
	const outputs = page.locator('section.panel', {
		has: page.getByRole('heading', { name: `Outputs (${tx.outputs.length})` })
	});
	await expect(outputs.locator('article.box')).toHaveCount(tx.outputs.length);
	await expect(outputs.getByRole('link', { name: /^\w{10}…/ }).first()).toBeVisible();

	// Every input of this tx spends a box created before the fixture range, so the UI shows
	// its "Unknown box" placeholder and flags the in-total as partial.
	const inputs = page.locator('section.panel', {
		has: page.getByRole('heading', { name: `Inputs (${tx.inputs.length})` })
	});
	await expect(inputs.locator('article.unknown')).toHaveCount(tx.inputs.length);
	await expect(page.getByText('Total in (known boxes)')).toBeVisible();

	// The block link goes back to the containing block.
	await page
		.getByRole('link', { name: String(tx.height), exact: true })
		.first()
		.click();
	await expect(page).toHaveURL(`/blocks/${tx.height}`);
});

test('an unknown transaction id remains watchable on older servers', async ({ page }) => {
	await page.goto(`/tx/${'a'.repeat(64)}`);
	await expect(page.getByRole('heading', { name: 'Transaction', exact: true })).toBeVisible();
	await expect(
		page.getByText('This server does not provide live transaction status.', { exact: false })
	).toBeVisible();
});

test('an invalid transaction id receives a format error', async ({ page }) => {
	await page.goto('/tx/not-a-transaction');
	await expect(page.getByRole('heading', { name: '400' })).toBeVisible();
	await expect(
		page.getByText('A transaction ID must contain 64 hexadecimal characters')
	).toBeVisible();
});
