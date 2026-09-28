import { expect, test } from '@playwright/test';
import { contrast } from './contrast';
import mainnet from '../live/mainnet.json' with { type: 'json' };

for (const width of [320, 768, 1440]) {
	for (const theme of ['light', 'dark']) {
		test(`full mainnet token title wraps at ${width}, ${theme}`, async ({ page }) => {
			await page.setViewportSize({ width, height: 900 });
			await page.addInitScript((theme) => localStorage.setItem('xp-theme', theme), theme);
			await page.route(`**/v1/tokens/${mainnet.long.id}`, (route) =>
				route.fulfill({ json: mainnet.long })
			);
			await page.route(`**/v1/tokens/${mainnet.long.id}/holders?*`, (route) =>
				route.fulfill({ json: { items: [], next_cursor: null } })
			);
			await page.goto(`/token/${mainnet.long.id}`);
			const title = page.locator('h1');
			for (const appearance of ['Prism', 'Original', 'Aurora', 'Atelier']) {
				if (appearance !== 'Prism') {
					await page.getByRole('button', { name: 'Appearance', exact: true }).click();
					const picker = page.getByRole('dialog', { name: 'Choose appearance', exact: true });
					await picker.getByRole('radio', { name: appearance, exact: true }).check();
					await picker.getByRole('button', { name: 'Close', exact: true }).click();
				}
				await expect(title).toHaveText(mainnet.long.name);
				expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
				const bounds = await title.boundingBox();
				expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
				await expect(title.locator('bdi')).toHaveAttribute('dir', 'auto');
				const colours = await title.evaluate((element) => {
					// Some designs paint the header; others let the page ground show through.
					// Its subtle body gradient is checked against the opaque base colour here.
					let ancestor: Element | null = element.closest('header');
					while (ancestor) {
						const bg = getComputedStyle(ancestor).backgroundColor;
						const channels = bg.match(/[\d.]+/g)?.map(Number);
						if (channels && (channels.length === 3 || channels[3] === 1)) {
							return { fg: getComputedStyle(element).color, bg };
						}
						ancestor = ancestor.parentElement;
					}
					throw new Error('Title has no opaque ancestor background');
				});
				expect(contrast(colours.fg, colours.bg)).toBeGreaterThanOrEqual(4.5);
			}
		});
	}
}

// Supplemental adversarial text: the live token scan did not find an RTL name.
for (const theme of ['light', 'dark']) {
	test(`mixed RTL and emoji title stays isolated, ${theme}`, async ({ page }) => {
		const name = 'שלום 🪙 ' + 'اسم'.repeat(45) + ' ABC 123';
		await page.setViewportSize({ width: 320, height: 900 });
		await page.addInitScript((theme) => localStorage.setItem('xp-theme', theme), theme);
		await page.route(`**/v1/tokens/${mainnet.long.id}`, (route) =>
			route.fulfill({ json: { ...mainnet.long, name } })
		);
		await page.route(`**/v1/tokens/${mainnet.long.id}/holders?*`, (route) =>
			route.fulfill({ json: { items: [], next_cursor: null } })
		);
		await page.goto(`/token/${mainnet.long.id}`);
		await expect(page.locator('h1')).toHaveText(name);
		const bidi = await page.locator('h1 bdi').evaluate((element) => ({
			direction: getComputedStyle(element).direction,
			isolation: getComputedStyle(element).unicodeBidi
		}));
		expect(bidi).toEqual({ direction: 'rtl', isolation: 'isolate' });
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
	});
}
