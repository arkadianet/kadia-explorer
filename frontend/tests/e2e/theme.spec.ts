import { expect, test } from '@playwright/test';

test('the theme toggle flips the theme and survives a reload', async ({ page }) => {
	await page.goto('/');

	const html = page.locator('html');
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
});
