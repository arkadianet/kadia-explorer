import { expect, test } from '@playwright/test';

test('density persists independently, changes real row spacing, and keeps keyboard focus contained', async ({
	page
}) => {
	await page.setViewportSize({ width: 1440, height: 900 });
	await page.goto('/blocks');
	const html = page.locator('html');
	await expect(html).toHaveAttribute('data-density', 'compact');
	const row = page.locator('.table tbody tr').first();
	await page.getByRole('button', { name: 'Appearance', exact: true }).click();
	const dialog = page.getByRole('dialog', { name: 'Choose appearance' });
	await expect(dialog.getByRole('radio', { name: 'Compact', exact: true })).toBeChecked();
	await dialog.getByRole('radio', { name: 'Standard', exact: true }).check();
	const standard = (await row.boundingBox())!.height;
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
	await page.getByRole('button', { name: 'Appearance', exact: true }).click();
	await dialog.getByRole('radio', { name: 'Standard', exact: true }).check();
	await page.keyboard.press('Escape');
	await page.reload();
	await expect(html).toHaveAttribute('data-density', 'standard');
});

test('invalid saved density falls back to compact and its selected control', async ({ page }) => {
	await page.addInitScript(() => localStorage.setItem('xp-density', 'invalid'));
	await page.goto('/');
	await expect(page.locator('html')).toHaveAttribute('data-density', 'compact');
	await page.getByRole('button', { name: 'Appearance', exact: true }).click();
	await expect(page.getByRole('radio', { name: 'Compact', exact: true })).toBeChecked();
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const density of ['standard', 'compact']) {
		for (const width of [390, 1440]) {
			test(`${appearance} ${density} shows homepage records on the first screen at ${width}`, async ({
				page
			}) => {
				await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
				await page.addInitScript(
					({ appearance, density }) => {
						localStorage.setItem('xp-appearance', appearance);
						localStorage.setItem('xp-density', density);
					},
					{ appearance, density }
				);
				await page.goto('/');
				await expect(page.locator('.pending-sample li')).toHaveCount(3);
				await page.evaluate(() => document.fonts.ready);
				const rows = page.locator('.recent-blocks .table tbody tr');
				const lastRequired = await rows.nth(width === 390 ? 2 : 7).boundingBox();
				const bottom = width === 390 ? (await page.locator('.tabbar').boundingBox())!.y : 900;
				expect(lastRequired!.y + lastRequired!.height).toBeLessThanOrEqual(bottom - 8);
				await expect(rows.nth(width === 390 ? 2 : 7)).toBeInViewport({ ratio: 1 });
				if (width === 1440) {
					await expect(page.locator('.live .txlist li').nth(2)).toBeInViewport({ ratio: 1 });
				} else {
					const search = await page
						.getByRole('searchbox', { name: 'Search', exact: true })
						.boundingBox();
					expect(search!.height).toBeGreaterThanOrEqual(44);
					for (const name of ['Appearance', 'Toggle theme']) {
						const control = await page.getByRole('button', { name, exact: true }).boundingBox();
						expect(control!.width).toBeGreaterThanOrEqual(44);
						expect(control!.height).toBeGreaterThanOrEqual(44);
					}
				}
				expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
				if (appearance === 'prism' && density === 'standard') {
					await page.screenshot({ path: `../artifacts/density-prism-${width}.png` });
				}
			});
		}
	}
}
