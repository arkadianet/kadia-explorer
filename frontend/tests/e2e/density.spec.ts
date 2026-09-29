import { expect, test } from '@playwright/test';

test('density persists independently, changes real row spacing, and keeps keyboard focus contained', async ({
	page
}) => {
	await page.setViewportSize({ width: 1440, height: 900 });
	await page.goto('/blocks');
	const html = page.locator('html');
	await expect(html).toHaveAttribute('data-density', 'standard');
	const row = page.locator('.table tbody tr').first();
	const standard = (await row.boundingBox())!.height;
	await page.getByRole('button', { name: 'Appearance', exact: true }).click();
	const dialog = page.getByRole('dialog', { name: 'Choose appearance' });
	await dialog.getByRole('radio', { name: 'Compact', exact: true }).check();
	await expect(html).toHaveAttribute('data-density', 'compact');
	await dialog.getByRole('radio', { name: 'Atelier', exact: true }).check();
	await expect(html).toHaveAttribute('data-density', 'compact');
	await dialog.getByRole('radio', { name: 'Prism', exact: true }).check();
	await page.keyboard.press('Tab');
	await expect(dialog.getByRole('radio', { name: 'Compact', exact: true })).toBeFocused();
	await page.keyboard.press('Tab');
	await expect(dialog.getByRole('button', { name: 'Close' })).toBeFocused();
	await page.keyboard.press('Shift+Tab');
	await expect(dialog.getByRole('radio', { name: 'Compact', exact: true })).toBeFocused();
	await page.keyboard.press('Escape');
	expect((await row.boundingBox())!.height).toBeLessThan(standard);
	await page.reload();
	await expect(html).toHaveAttribute('data-density', 'compact');
	await expect(html).toHaveAttribute('data-appearance', 'prism');
	await page.setViewportSize({ width: 390, height: 844 });
	expect((await row.boundingBox())!.height).toBeGreaterThanOrEqual(44);
	expect(
		(await page.getByRole('button', { name: 'Appearance', exact: true }).boundingBox())!.height
	).toBeGreaterThanOrEqual(44);
});

test('invalid saved density falls back to the standard layout and selected control', async ({
	page
}) => {
	await page.addInitScript(() => localStorage.setItem('xp-density', 'invalid'));
	await page.goto('/');
	await expect(page.locator('html')).toHaveAttribute('data-density', 'standard');
	await page.getByRole('button', { name: 'Appearance', exact: true }).click();
	await expect(page.getByRole('radio', { name: 'Standard', exact: true })).toBeChecked();
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const width of [390, 1440]) {
		test(`${appearance} puts homepage records in reach at ${width}`, async ({ page }) => {
			await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
			await page.addInitScript((value) => localStorage.setItem('xp-appearance', value), appearance);
			await page.goto('/');
			await expect(page.locator('.pending-sample li')).toHaveCount(3);
			await page.evaluate(() => document.fonts.ready);
			const firstPending = await page.locator('.pending-sample li').first().boundingBox();
			const firstBlock = await page.locator('.panels .table tbody tr').first().boundingBox();
			expect(firstPending!.y + firstPending!.height).toBeLessThan(width === 390 ? 844 : 700);
			expect(firstBlock!.y).toBeLessThan(width === 390 ? 1050 : 900);
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
		});
	}
}
