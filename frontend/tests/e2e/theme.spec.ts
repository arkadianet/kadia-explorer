import { expect, test } from '@playwright/test';
import { contrast } from './contrast';
import { block, claimableBox, MOCK_ADDRESS, newestToken, template, tx } from './data';

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

// Composition changes affect every route, not only the homepage. These checks guard
// readable page bounds and content in both modes while manual review judges the design.
for (const appearance of ['original', 'aurora', 'atelier', 'prism']) {
	for (const theme of ['light', 'dark']) {
		for (const width of [320, 1440]) {
			test(`${appearance} page families remain usable at ${width}, ${theme}`, async ({ page }) => {
				test.setTimeout(60_000);
				await page.setViewportSize({ width, height: 900 });
				await page.addInitScript(
					({ appearance, theme }) => {
						localStorage.setItem('xp-appearance', appearance);
						localStorage.setItem('xp-theme', theme);
					},
					{ appearance, theme }
				);
				const errors: string[] = [];
				page.on('pageerror', (error) => errors.push(error.message));
				const routes = [
					['/blocks', 'blocks', 'Blocks'],
					[`/blocks/${block.height}`, 'block', `Block ${block.height}`],
					['/txs', 'txs', 'Transactions'],
					[`/tx/${tx.id}`, 'tx', 'Transaction'],
					[`/address/${MOCK_ADDRESS}`, 'address', 'Address'],
					[`/box/${claimableBox.id}`, 'box', 'Box'],
					['/tokens', 'tokens', 'Find the name. Know the identity.'],
					[`/token/${newestToken.id}`, 'token', newestToken.name!],
					[`/template/${template.hash}`, 'template', 'Script template'],
					['/richlist', 'richlist', 'Rich list'],
					['/rent', 'rent', 'Rent'],
					['/status', 'status', 'Indexer status'],
					['/search', 'search', 'Find boxes by register value'],
					['/page-that-does-not-exist', 'error', '404']
				];
				for (const [url, kind, heading] of routes) {
					await test.step(url, async () => {
						await page.goto(url);
						await expect(page.locator('main')).toHaveAttribute('data-page', kind);
						await expect(page.getByRole('heading', { name: heading, exact: true })).toBeVisible();
						await page.evaluate(() => document.fonts.ready);
						expect(
							await page.evaluate(() => document.documentElement.scrollWidth),
							`${appearance} ${theme} ${url} must not scroll the whole page horizontally`
						).toBe(width);
						// Closed native dialogs keep their headings in the DOM without a layout box.
						for (const title of await page.locator('main h1:visible, main h2:visible').all()) {
							const bounds = await title.boundingBox();
							expect(bounds!.x).toBeGreaterThanOrEqual(0);
							expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
						}
						if (
							appearance === 'prism' &&
							width === 1440 &&
							['blocks', 'txs', 'tokens', 'richlist', 'rent', 'status'].includes(kind)
						) {
							const currentLabel = page
								.getByRole('navigation', { name: 'Primary', exact: true })
								.locator('[aria-current="page"] .nav-label');
							await expect(currentLabel).toBeVisible();
							const colours = await currentLabel.evaluate((element) => {
								// Include the link itself: a selected item may paint its own background.
								let ancestor: Element | null = element;
								while (ancestor) {
									const bg = getComputedStyle(ancestor).backgroundColor;
									const channels = bg.match(/[\d.]+/g)?.map(Number);
									if (channels && (channels.length === 3 || channels[3] === 1)) {
										return { fg: getComputedStyle(element).color, bg };
									}
									ancestor = ancestor.parentElement;
								}
								throw new Error('Current navigation label has no opaque background');
							});
							expect(
								contrast(colours.fg, colours.bg),
								`Current navigation text must remain readable on ${url} in ${theme} mode`
							).toBeGreaterThanOrEqual(4.5);
						}
						if (kind === 'address') {
							const tabs = await page
								.getByRole('tablist', { name: 'Address sections' })
								.boundingBox();
							const activity = await page
								.getByRole('tabpanel')
								.getByRole('region', { name: 'Address activity', exact: true })
								.boundingBox();
							expect(
								tabs!.y + tabs!.height <= activity!.y + 1 ||
									tabs!.x + tabs!.width <= activity!.x + 1 ||
									activity!.x + activity!.width <= tabs!.x + 1,
								'The activity region must not paint over its tabs'
							).toBe(true);
						}
					});
				}
				expect(errors).toEqual([]);
			});
		}
	}
}
