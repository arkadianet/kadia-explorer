import { expect, test } from '@playwright/test';
import { contrast } from './contrast';

for (const theme of ['light', 'dark']) {
	for (const width of [320, 768, 899]) {
		test(`mobile navigation reaches every section at ${width}, ${theme}`, async ({ page }) => {
			await page.setViewportSize({ width, height: 800 });
			await page.addInitScript((theme) => localStorage.setItem('xp-theme', theme), theme);
			await page.goto('/');
			const nav = page.locator('.tabbar');
			const more = nav.getByRole('button', { name: 'More', exact: true });
			await expect(nav.locator(':scope > a')).toHaveCount(4);
			await more.focus();
			await page.keyboard.press('Enter');
			await expect(nav.getByRole('link', { name: 'Tokens', exact: true })).toBeVisible();
			await page.keyboard.press('Tab');
			await expect(nav.getByRole('link', { name: 'Rich list', exact: true })).toBeFocused();
			await page.keyboard.press('Escape');
			await expect(more).toBeFocused();
			await expect(nav.getByRole('link', { name: 'Tokens', exact: true })).toBeHidden();
			for (const [label, path] of [
				['Tokens', '/tokens'],
				['Rich list', '/richlist'],
				['Status', '/status']
			]) {
				await more.press('Space');
				const link = nav.getByRole('link', { name: label, exact: true });
				await link.focus();
				const colours = await link.evaluate((element) => {
					const style = getComputedStyle(element);
					return {
						fg: style.color,
						bg: getComputedStyle(element.parentElement!).backgroundColor,
						focus: style.outlineColor,
						width: style.outlineWidth,
						outline: style.outlineStyle,
						transition: style.transitionDuration
					};
				});
				expect(contrast(colours.fg, colours.bg)).toBeGreaterThanOrEqual(4.5);
				expect(contrast(colours.focus, colours.bg)).toBeGreaterThanOrEqual(3);
				expect(colours.width).toBe('2px');
				expect(colours.outline).toBe('solid');
				expect(colours.transition).toBe('0s');
				const geometry = await link.boundingBox();
				expect(geometry!.height).toBeGreaterThanOrEqual(44);
				expect(geometry!.x + geometry!.width).toBeLessThanOrEqual(width);
				await page.keyboard.press('Enter');
				await expect(page).toHaveURL(path);
				await expect(more).toHaveAttribute('aria-expanded', 'false');
				await expect(more).toHaveClass(/current/);
				await more.press('Enter');
				await expect(link).toHaveAttribute('aria-current', 'page');
				await page.keyboard.press('Escape');
			}
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
		});
	}
}
