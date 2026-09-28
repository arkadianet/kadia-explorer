import { expect, test } from '@playwright/test';

test('the theme toggle flips the theme and survives a reload', async ({ page }) => {
	await page.goto('/');

	const html = page.locator('html');
	await expect(html).toHaveAttribute('data-appearance', 'prism');
	await expect(html).toHaveAttribute('data-theme', /light|dark/);
	const before = await html.getAttribute('data-theme');
	const after = before === 'dark' ? 'light' : 'dark';

	await page.getByRole('button', { name: 'Toggle theme' }).click();
	await expect(html).toHaveAttribute('data-theme', after);
	expect(await page.evaluate(() => localStorage.getItem('xp-theme'))).toBe(after);

	await page.reload();
	await expect(html).toHaveAttribute('data-theme', after);

	// And back again.
	await page.getByRole('button', { name: 'Toggle theme' }).click();
	await expect(html).toHaveAttribute('data-theme', before!);
	await expect(html).toHaveAttribute('data-appearance', 'prism');
});

test('appearance and light/dark choices persist independently through navigation and reload', async ({
	page
}) => {
	await page.emulateMedia({ colorScheme: 'light' });
	await page.goto('/');
	const html = page.locator('html');
	const opener = page.getByRole('button', { name: 'Appearance', exact: true });
	const dialog = page.getByRole('dialog', { name: 'Choose appearance', exact: true });
	await expect(html).toHaveAttribute('data-theme', 'light');
	await opener.click();
	await dialog.getByRole('radio', { name: 'Aurora', exact: true }).check();
	await expect(html).toHaveAttribute('data-appearance', 'aurora');
	await expect(html).toHaveAttribute('data-theme', 'light');
	await dialog.getByRole('button', { name: 'Close', exact: true }).click();
	await page.getByRole('button', { name: 'Toggle theme' }).click();
	await expect(html).toHaveAttribute('data-theme', 'dark');
	await expect(html).toHaveAttribute('data-appearance', 'aurora');
	expect(
		await page.evaluate(() => ({
			theme: localStorage.getItem('xp-theme'),
			appearance: localStorage.getItem('xp-appearance')
		}))
	).toEqual({ theme: 'dark', appearance: 'aurora' });

	await page
		.getByRole('navigation', { name: 'Primary', exact: true })
		.getByRole('link', { name: 'Tokens', exact: true })
		.click();
	await expect(page).toHaveURL('/tokens');
	await expect(html).toHaveAttribute('data-theme', 'dark');
	await expect(html).toHaveAttribute('data-appearance', 'aurora');
	await page.reload();
	await expect(html).toHaveAttribute('data-theme', 'dark');
	await expect(html).toHaveAttribute('data-appearance', 'aurora');
	await opener.click();
	await expect(dialog.getByRole('radio', { name: 'Aurora', exact: true })).toBeChecked();
	await dialog.getByRole('radio', { name: 'Original', exact: true }).check();
	await expect(html).toHaveAttribute('data-appearance', 'original');
	await expect(html).toHaveAttribute('data-theme', 'dark');
	expect(await page.evaluate(() => localStorage.getItem('xp-theme'))).toBe('dark');
	await dialog.getByRole('button', { name: 'Close', exact: true }).click();
	await page.getByRole('button', { name: 'Toggle theme' }).click();
	await expect(html).toHaveAttribute('data-theme', 'light');
	await expect(html).toHaveAttribute('data-appearance', 'original');
	expect(await page.evaluate(() => localStorage.getItem('xp-appearance'))).toBe('original');
});

test('an invalid saved appearance falls back to Prism without replacing the saved theme', async ({
	page
}) => {
	await page.addInitScript(() => {
		localStorage.setItem('xp-appearance', 'unavailable-style');
		localStorage.setItem('xp-theme', 'dark');
	});
	await page.goto('/');
	await expect(page.locator('html')).toHaveAttribute('data-appearance', 'prism');
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
	await page.getByRole('button', { name: 'Appearance', exact: true }).click();
	await expect(
		page
			.getByRole('dialog', { name: 'Choose appearance', exact: true })
			.getByRole('radio', { name: 'Prism', exact: true })
	).toBeChecked();
	expect(await page.evaluate(() => localStorage.getItem('xp-theme'))).toBe('dark');
});

test('appearance supports radio keys, contained focus, Escape and reopening the selected style', async ({
	page
}) => {
	await page.goto('/');
	const opener = page.getByRole('button', { name: 'Appearance', exact: true });
	const dialog = page.getByRole('dialog', { name: 'Choose appearance', exact: true });
	const close = dialog.getByRole('button', { name: 'Close', exact: true });
	const prism = dialog.getByRole('radio', { name: 'Prism', exact: true });
	const atelier = dialog.getByRole('radio', { name: 'Atelier', exact: true });
	await opener.focus();
	await page.keyboard.press('Enter');
	await expect(dialog).toBeVisible();
	await expect(prism).toBeFocused();
	await page.keyboard.press('ArrowLeft');
	await expect(atelier).toBeFocused();
	await expect(atelier).toBeChecked();
	await expect(page.locator('html')).toHaveAttribute('data-appearance', 'atelier');
	expect(await page.evaluate(() => localStorage.getItem('xp-appearance'))).toBe('atelier');
	await page.keyboard.press('Tab');
	await expect(close).toBeFocused();
	await page.keyboard.press('Tab');
	await expect(atelier).toBeFocused();
	await page.keyboard.press('Shift+Tab');
	await expect(close).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(dialog).toBeHidden();
	await expect(opener).toBeFocused();
	await opener.press('Enter');
	await expect(atelier).toBeFocused();
	await expect(atelier).toBeChecked();
	await page.keyboard.press('Escape');
	await expect(opener).toBeFocused();
});

for (const appearance of ['original', 'aurora', 'atelier', 'prism']) {
	for (const theme of ['light', 'dark']) {
		test(`${appearance} appearance and its picker fit 320px in ${theme}`, async ({ page }) => {
			await page.setViewportSize({ width: 320, height: 800 });
			await page.emulateMedia({ reducedMotion: 'reduce' });
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await page.goto('/');
			await expect(page.locator('html')).toHaveAttribute('data-appearance', appearance);
			await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
			const opener = page.getByRole('button', { name: 'Appearance', exact: true });
			await opener.click();
			const dialog = page.getByRole('dialog', { name: 'Choose appearance', exact: true });
			await expect(dialog).toBeVisible();
			for (const name of ['Original', 'Aurora', 'Atelier', 'Prism']) {
				await expect(dialog.getByRole('radio', { name, exact: true })).toBeVisible();
			}
			const bounds = await dialog.boundingBox();
			expect(bounds!.x).toBeGreaterThanOrEqual(0);
			expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(320);
			expect(await dialog.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(
				true
			);
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
			await page.keyboard.press('Escape');
			await expect(opener).toBeFocused();
		});
	}
}
