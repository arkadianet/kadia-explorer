import { expect, test } from '@playwright/test';
import data from './mainnet.json' with { type: 'json' };

const routes: [string, string][] = [
	['home', '/'],
	['tokens', '/tokens'],
	['richlist', '/richlist'],
	['holders', `/token/${data.holders.id}`],
	['long-name', `/token/${data.long.id}`],
	['emoji', `/token/${data.emoji.id}`],
	['nine-decimal', `/token/${data.nine.id}`],
	['address', `/address/${data.address}`],
	['busy-address', `/address/${data.busyAddress}`],
	['many-tokens-box', `/box/${data.manyBox}`],
	['registers-box', `/box/${data.registerBox}`],
	['transaction', `/tx/${data.long.mint_tx}`],
	['block', `/blocks/${data.long.mint_height}`],
	['blocks', '/blocks'],
	['txs', '/txs'],
	['rent', '/rent'],
	['status', '/status'],
	['template', `/template/${data.template}`],
	['register-search', `/search?reg=R4&value=${data.registerValue}`]
];

test.beforeEach(async ({ page }) => {
	await page.route('**/*', (route) =>
		new URL(route.request().url()).hostname === '127.0.0.1' ? route.continue() : route.abort()
	);
});

for (const theme of ['light', 'dark']) {
	for (const width of [320, 768, 1440]) {
		test(`mainnet geometry and screenshots ${width} ${theme}`, async ({ page }) => {
			await page.setViewportSize({ width, height: 900 });
			await page.emulateMedia({ reducedMotion: 'reduce' });
			await page.addInitScript((theme) => localStorage.setItem('xp-theme', theme), theme);
			const errors: string[] = [];
			page.on('pageerror', (e) => errors.push(e.message));
			for (const [name, path] of routes) {
				await page.goto(path);
				await expect(page.locator('main h1, main h2').first()).toBeVisible();
				await expect(page.locator('main .skeleton')).toHaveCount(0);
				await page.evaluate(() => document.fonts.ready);
				await expect(page.locator('main')).not.toContainText('NaN');
				await expect(page.locator('main .error')).toHaveCount(0);
				expect(await page.evaluate(() => document.documentElement.scrollWidth), path).toBe(width);
				expect(await page.locator('tbody tr').count(), path).toBeLessThanOrEqual(150);
				await page.screenshot({ path: `.uishots/${name}-${width}-${theme}.png`, fullPage: true });
			}
			expect(errors).toEqual([]);
		});
	}
}

test('real token/box collision offers both destinations', async ({ page }) => {
	await page.goto(`/search?q=${data.holders.id}`);
	await expect(page.locator(`main a[href="/token/${data.holders.id}"]`)).toBeVisible();
	await expect(page.locator(`main a[href="/box/${data.holders.id}"]`)).toBeVisible();
	await page.locator(`main a[href="/token/${data.holders.id}"]`).click();
	await expect(page.locator('h1')).toHaveText(data.holders.name);
});

test('slow holders never delay token facts or mobile navigation; pagination stays bounded', async ({
	page
}) => {
	await page.setViewportSize({ width: 320, height: 800 });
	let release!: () => void;
	const gate = new Promise<void>((resolve) => {
		release = resolve;
	});
	await page.route(`**/v1/tokens/${data.holders.id}/holders?*`, async (route) => {
		await gate;
		await route.continue();
	});
	await page.goto(`/token/${data.holders.id}`);
	await expect(page.locator('h1')).toHaveText(data.holders.name);
	await page.locator('.tabbar .more-toggle').press('Enter');
	await expect(
		page.locator('.tabbar').getByRole('link', { name: 'Tokens', exact: true })
	).toBeVisible();
	release();
	await page.keyboard.press('Escape');
	await expect(page.locator('tbody tr')).toHaveCount(50);
	await page.getByRole('button', { name: 'Load more' }).scrollIntoViewIfNeeded();
	await expect(page.locator('tbody tr')).toHaveCount(100);
});

for (const theme of ['light', 'dark']) {
	for (const width of [320, 768, 899]) {
		test(`live mobile section navigation ${width} ${theme}`, async ({ page }) => {
			await page.setViewportSize({ width, height: 900 });
			await page.addInitScript((theme) => localStorage.setItem('xp-theme', theme), theme);
			await page.goto('/');
			const nav = page.locator('.tabbar');
			const more = nav.getByRole('button', { name: 'More', exact: true });
			for (const [label, path] of [
				['Tokens', '/tokens'],
				['Rich list', '/richlist'],
				['Status', '/status']
			]) {
				await more.press('Enter');
				await nav.getByRole('link', { name: label, exact: true }).click();
				await expect(page).toHaveURL(path);
				await expect(more).toHaveAttribute('aria-expanded', 'false');
			}
			await more.press('Enter');
			await page.screenshot({ path: `.uishots/mobile-menu-${width}-${theme}.png`, fullPage: true });
		});
	}
}
