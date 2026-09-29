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
			await expect(nav.getByRole('link', { name: 'Mempool', exact: true })).toBeFocused();
			await page.keyboard.press('Escape');
			await expect(more).toBeFocused();
			await expect(nav.getByRole('link', { name: 'Tokens', exact: true })).toBeHidden();
			for (const [label, path] of [
				['Mempool', '/mempool'],
				['Mining', '/mining'],
				['Tokens', '/tokens'],
				['Rich list', '/richlist'],
				['Status', '/status']
			]) {
				await more.press('Space');
				const link = nav.getByRole('link', { name: label, exact: true });
				await expect(link).toBeVisible();
				await link.focus();
				await expect(link).toBeFocused();
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

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	test(`${appearance} desktop navigation keeps every section reachable at 900 by 500`, async ({
		page
	}) => {
		await page.setViewportSize({ width: 900, height: 500 });
		await page.addInitScript((appearance) => {
			localStorage.setItem('xp-appearance', appearance);
			localStorage.setItem('xp-theme', 'light');
		}, appearance);
		await page.goto('/status');
		const nav = page.locator('.rail .nav');
		const links = nav.getByRole('link');
		await expect(links).toHaveCount(14);
		for (const [full, compact] of [
			['Storage rent', 'Rent'],
			['Saved addresses', 'Saved'],
			['Network history', 'Network'],
			['API playground', 'API']
		]) {
			await expect(nav.getByRole('link', { name: full, exact: true })).toHaveText(
				appearance === 'prism' || appearance === 'atelier' ? compact : full
			);
		}
		await links.first().focus();
		for (let index = 0; index < 14; index++) {
			const link = links.nth(index);
			await expect(link).toBeFocused();
			await expect(link).toBeInViewport({ ratio: 1 });
			const outline = await link.evaluate((element) => {
				const style = getComputedStyle(element);
				return { width: style.outlineWidth, style: style.outlineStyle };
			});
			expect(outline).toEqual({ width: '2px', style: 'solid' });
			if (index < 13) await page.keyboard.press('Tab');
		}
		if (appearance === 'original' || appearance === 'aurora') {
			await expect(nav.getByText('Scroll ↕', { exact: true })).toBeVisible();
			expect(await nav.locator('ul').evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
		} else {
			await expect(nav.getByText('Scroll ↕', { exact: true })).toBeHidden();
		}
		expect(
			await page.evaluate(
				() => document.documentElement.scrollWidth <= document.documentElement.clientWidth
			)
		).toBe(true);
	});

	test(`${appearance} landscape More navigation scrolls without clipping keyboard targets`, async ({
		page
	}) => {
		await page.setViewportSize({ width: 760, height: 320 });
		await page.addInitScript((appearance) => {
			localStorage.setItem('xp-appearance', appearance);
			localStorage.setItem('xp-theme', 'dark');
		}, appearance);
		await page.goto('/status');
		const nav = page.locator('.tabbar');
		const more = nav.getByRole('button', { name: 'More', exact: true });
		await more.focus();
		await page.keyboard.press('Enter');
		const menu = page.locator('#more-navigation');
		const geometry = await menu.boundingBox();
		expect(geometry!.y).toBeGreaterThanOrEqual(8);
		expect(geometry!.y + geometry!.height).toBeLessThanOrEqual((await nav.boundingBox())!.y - 7);
		const links = menu.getByRole('link');
		await expect(links).toHaveCount(10);
		for (let index = 0; index < 10; index++) {
			await page.keyboard.press('Tab');
			await expect(links.nth(index)).toBeFocused();
			await expect(links.nth(index)).toBeInViewport({ ratio: 1 });
		}
		expect(await menu.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
		await page.keyboard.press('Escape');
		await expect(menu).toBeHidden();
		await expect(more).toBeFocused();
		expect(
			await page.evaluate(
				() => document.documentElement.scrollWidth <= document.documentElement.clientWidth
			)
		).toBe(true);
	});
}
