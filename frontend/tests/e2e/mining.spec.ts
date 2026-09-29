import { expect, test, type Page } from '@playwright/test';
import example from '../fixtures/mining-overview.json' with { type: 'json' };

async function setup(page: Page) {
	const urls: string[] = [];
	await page.route('**/v1/mining?*', (route) => {
		urls.push(route.request().url());
		return route.fulfill({ json: example });
	});
	await page.goto('/mining?from_height=1&to_height=4&top=2');
	await expect(page.getByLabel('From height', { exact: true })).toHaveValue('1');
	return urls;
}
const result = (page: Page) =>
	page.getByRole('region', { name: 'Mining signals results', exact: true });
async function load(page: Page) {
	await page.getByRole('button', { name: 'Load mining signals', exact: true }).click();
	await expect(result(page)).toBeVisible();
}
test('mining loads explicitly with exact full-range shares, fee precision and unknown votes', async ({
	page
}) => {
	const urls = await setup(page);
	expect(urls).toHaveLength(0);
	await load(page);
	const keys = page.getByRole('region', { name: 'Public key block shares', exact: true });
	await expect(keys.getByRole('listitem').first()).toContainText(
		example.miner_keys.items[0].public_key
	);
	await expect(keys.getByRole('listitem').first()).toContainText('2 / 4');
	await expect(keys).toContainText('Other 1 public keys');
	await expect(keys).toContainText('1 / 4 blocks');
	await expect(keys.getByText('9007199254740996 nanoERG', { exact: true })).toBeVisible();
	const votes = page.getByRole('region', { name: 'Observed vote bytes', exact: true });
	await expect(votes.getByText('1 unknown vote fields.', { exact: true })).toBeVisible();
	await expect(votes).toContainText('never as zero votes');
	await expect(votes).toContainText('tuple appears in 1 blocks');
	await expect(result(page)).toContainText('The index is partial, from height 1.');
	await expect(page.getByRole('link', { name: 'Open pinned range' })).toHaveAttribute(
		'href',
		'/mining?from_height=1&to_height=4&top=2&end_block_id=' + 'a'.repeat(64)
	);
	await expect(page.getByRole('link', { name: 'Network activity for this range' })).toHaveAttribute(
		'href',
		'/network?from_height=1&to_height=4&buckets=60&end_block_id=' + 'a'.repeat(64)
	);
	await load(page);
	expect(urls).toHaveLength(2);
	expect(new URL(urls[1]).searchParams.get('end_block_id')).toBe('a'.repeat(64));
});
test('a changed anchor removes old shares until an explicit unpinned reload', async ({ page }) => {
	await setup(page);
	await load(page);
	await page.route('**/v1/mining?*', (route) =>
		route.fulfill({ status: 409, json: { detail: 'The pinned end block changed.' } })
	);
	await page.getByRole('button', { name: 'Load mining signals', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('The pinned end block changed.');
	await expect(result(page)).toHaveCount(0);
	await page.getByRole('button', { name: 'Clear end-block pin', exact: true }).click();
	await page.route('**/v1/mining?*', (route) => route.fulfill({ json: example }));
	await load(page);
});
test('incomplete or inconsistent responses never become partial rankings', async ({ page }) => {
	await setup(page);
	await page.route('**/v1/mining?*', (route) =>
		route.fulfill({ status: 422, json: { detail: 'The range starts before retained history.' } })
	);
	await page.getByRole('button', { name: 'Load mining signals', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('before retained history');
	const bad = structuredClone(example);
	bad.miner_keys.other_block_count = 0;
	await page.route('**/v1/mining?*', (route) => route.fulfill({ json: bad }));
	await page.getByRole('button', { name: 'Load mining signals', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('does not match');
	await expect(result(page)).toHaveCount(0);
});
test('editing the range discards a late in-flight response', async ({ page }) => {
	await setup(page);
	let release!: () => void;
	let entered!: () => void;
	const waiting = new Promise<void>((resolve) => {
		entered = resolve;
	});
	await page.route('**/v1/mining?*', async (route) => {
		entered();
		await new Promise<void>((resolve) => {
			release = resolve;
		});
		await route.fulfill({ json: example }).catch(() => undefined);
	});
	await page.getByRole('button', { name: 'Load mining signals', exact: true }).click();
	await waiting;
	await page.getByLabel('From height', { exact: true }).fill('2');
	release();
	await expect(result(page)).toHaveCount(0);
	await expect(
		page.getByRole('button', { name: 'Load mining signals', exact: true })
	).toBeEnabled();
});
for (const appearance of ['original', 'aurora', 'atelier', 'prism']) {
	for (const theme of ['light', 'dark']) {
		test(`mining is usable at 320 in ${appearance} ${theme}`, async ({ page }) => {
			await page.setViewportSize({ width: 320, height: 1000 });
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await setup(page);
			await load(page);
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
				321
			);
			await expect(
				page.getByRole('region', { name: 'Public key block shares', exact: true })
			).toBeVisible();
			await expect(
				page.getByRole('region', { name: 'Observed vote bytes', exact: true })
			).toBeVisible();
			const code = page
				.getByRole('region', { name: 'Public key block shares', exact: true })
				.locator('code')
				.first();
			expect(await code.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
			await page.getByRole('button', { name: 'Clear end-block pin', exact: true }).click();
			await expect(result(page)).toHaveCount(0);
		});
	}
	test(`mining keeps its ${appearance} desktop composition`, async ({ page }) => {
		await page.setViewportSize({ width: 1440, height: 1000 });
		await page.addInitScript(
			(appearance) => localStorage.setItem('xp-appearance', appearance),
			appearance
		);
		await setup(page);
		await load(page);
		const keys = (await page
			.getByRole('region', { name: 'Public key block shares', exact: true })
			.boundingBox())!;
		const versions = (await page
			.getByRole('region', { name: 'Header versions', exact: true })
			.boundingBox())!;
		const summary = (await result(page).locator('dl').boundingBox())!;
		if (appearance === 'prism') {
			expect(versions.x).toBeGreaterThan(keys.x + keys.width);
			expect(Math.abs(versions.y - keys.y)).toBeLessThan(2);
		} else if (appearance === 'atelier') {
			expect(keys.x).toBeGreaterThan(summary.x + summary.width);
			expect(Math.abs(summary.y - keys.y)).toBeLessThan(2);
		} else {
			expect(versions.y).toBeGreaterThanOrEqual(keys.y + keys.height);
			const rows = page
				.getByRole('region', { name: 'Public key block shares', exact: true })
				.getByRole('listitem');
			const first = (await rows.nth(0).boundingBox())!,
				second = (await rows.nth(1).boundingBox())!;
			if (appearance === 'aurora') expect(Math.abs(first.y - second.y)).toBeLessThan(2);
			else expect(second.y).toBeGreaterThanOrEqual(first.y + first.height);
		}
	});
}
