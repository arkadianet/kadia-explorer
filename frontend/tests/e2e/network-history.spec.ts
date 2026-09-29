import { expect, test, type Page } from '@playwright/test';
import type { NetworkHistory } from '../../src/lib/network/history';

function fixture(): NetworkHistory {
	const first = {
		from_height: 1,
		to_height: 2,
		block_count: 2,
		transaction_count: '7',
		fees: '9007199254740993',
		size_bytes: '100',
		difficulty_min: '100',
		difficulty_max: '300',
		difficulty_end: '100',
		first_timestamp: 4000,
		last_timestamp: 1000,
		earliest_timestamp: 1000,
		latest_timestamp: 4000
	};
	const last = {
		from_height: 3,
		to_height: 4,
		block_count: 2,
		transaction_count: '13',
		fees: '11',
		size_bytes: '300',
		difficulty_min: '200',
		difficulty_max: '400',
		difficulty_end: '400',
		first_timestamp: 3000,
		last_timestamp: 2000,
		earliest_timestamp: 2000,
		latest_timestamp: 3000
	};
	return {
		scope: 'canonical_block_headers',
		consistency: 'single_reader',
		complete: true,
		full_history: false,
		partial_from: 1,
		indexed_height: 8,
		anchor: { height: 4, block_id: 'a'.repeat(64) },
		requested_buckets: 2,
		bucket_width: 2,
		totals: {
			...first,
			to_height: 4,
			block_count: 4,
			transaction_count: '20',
			fees: '9007199254741004',
			size_bytes: '400',
			difficulty_max: '400',
			difficulty_end: '400',
			last_timestamp: 2000
		},
		buckets: [first, last]
	};
}
async function setup(page: Page) {
	const urls: string[] = [];
	await page.route('**/v1/network/history?*', (route) => {
		urls.push(route.request().url());
		return route.fulfill({ json: fixture() });
	});
	await page.goto('/network?from_height=1&to_height=4&buckets=2');
	await expect(page.getByLabel('From height', { exact: true })).toHaveValue('1');
	return urls;
}
test('network range loads explicitly, preserves exact totals and pins refresh', async ({
	page
}) => {
	const urls = await setup(page);
	expect(urls).toHaveLength(0);
	await page.getByRole('button', { name: 'Load network history', exact: true }).click();
	const result = page.getByRole('region', { name: 'Network history results', exact: true });
	await expect(result).toBeVisible();
	await expect(result.getByText('9,007,199.254741004', { exact: false })).toBeVisible();
	await expect(
		result.getByText('The index is partial, from height 1.', { exact: false })
	).toBeVisible();
	await expect(result.getByRole('cell', { name: '9007199254740993', exact: true })).toBeVisible();
	await page
		.getByRole('combobox', { name: 'Chart metric', exact: true })
		.selectOption('difficulty_end');
	await expect(
		result.getByRole('img', { name: 'End-block difficulty by block-height bucket' })
	).toBeVisible();
	await expect(result.getByRole('link', { name: 'Open pinned range' })).toHaveAttribute(
		'href',
		'/network?from_height=1&to_height=4&buckets=2&end_block_id=' + 'a'.repeat(64)
	);
	await expect(result.getByRole('link', { name: 'Inspect mining signals' })).toHaveAttribute(
		'href',
		'/mining?from_height=1&to_height=4&end_block_id=' + 'a'.repeat(64)
	);
	await page.getByRole('button', { name: 'Load network history', exact: true }).click();
	await expect(result).toBeVisible();
	expect(urls).toHaveLength(2);
	expect(new URL(urls[1]).searchParams.get('end_block_id')).toBe('a'.repeat(64));
	const methodology = result.locator('details.coverage');
	await expect(methodology).not.toHaveAttribute('open', '');
	await methodology.getByText('Methodology and snapshot evidence', { exact: true }).click();
	await expect(methodology).toContainText('Difficulty is not a measured hashrate.');
});
test('changed pinned block clears the old chart until an explicit unpinned load', async ({
	page
}) => {
	await setup(page);
	await page.getByRole('button', { name: 'Load network history', exact: true }).click();
	await expect(
		page.getByRole('region', { name: 'Network history results', exact: true })
	).toBeVisible();
	await page.route('**/v1/network/history?*', (route) =>
		route.fulfill({ status: 409, json: { detail: 'The pinned end block changed.' } })
	);
	await page.getByRole('button', { name: 'Load network history', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('The pinned end block changed.');
	await expect(
		page.getByRole('region', { name: 'Network history results', exact: true })
	).toHaveCount(0);
	await page.getByRole('button', { name: 'Clear end-block pin', exact: true }).click();
	await page.route('**/v1/network/history?*', (route) => route.fulfill({ json: fixture() }));
	await page.getByRole('button', { name: 'Load network history', exact: true }).click();
	await expect(
		page.getByRole('region', { name: 'Network history results', exact: true })
	).toBeVisible();
});
test('incomplete history and malformed totals never render a partial graph', async ({ page }) => {
	await setup(page);
	await page.route('**/v1/network/history?*', (route) =>
		route.fulfill({ status: 422, json: { detail: 'The range begins before retained history.' } })
	);
	await page.getByRole('button', { name: 'Load network history', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('before retained history');
	const invalid = fixture();
	invalid.totals.fees = '9007199254741005';
	await page.route('**/v1/network/history?*', (route) => route.fulfill({ json: invalid }));
	await page.getByRole('button', { name: 'Load network history', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('does not match');
	await expect(page.getByRole('img', { name: 'Transactions by block-height bucket' })).toHaveCount(
		0
	);
});
test('editing the selection cancels an in-flight result', async ({ page }) => {
	await setup(page);
	let release!: () => void;
	let entered!: () => void;
	const waiting = new Promise<void>((resolve) => (entered = resolve));
	await page.route('**/v1/network/history?*', async (route) => {
		entered();
		await new Promise<void>((resolve) => (release = resolve));
		await route.fulfill({ json: fixture() }).catch(() => undefined);
	});
	await page.getByRole('button', { name: 'Load network history', exact: true }).click();
	await waiting;
	await page.getByLabel('From height', { exact: true }).fill('2');
	release();
	await expect(
		page.getByRole('region', { name: 'Network history results', exact: true })
	).toHaveCount(0);
	await expect(
		page.getByRole('button', { name: 'Load network history', exact: true })
	).toBeEnabled();
});
for (const appearance of ['original', 'aurora', 'atelier', 'prism']) {
	for (const viewport of [
		{ width: 1440, height: 900 },
		{ width: 390, height: 844 }
	]) {
		test(`compact network brings exact values forward in ${appearance} at ${viewport.width}`, async ({
			page
		}) => {
			await page.setViewportSize(viewport);
			await page.addInitScript((appearance) => {
				localStorage.setItem('xp-appearance', appearance);
				localStorage.setItem('xp-density', 'compact');
			}, appearance);
			await setup(page);
			await page.getByRole('button', { name: 'Load network history', exact: true }).click();
			const result = page.getByRole('region', { name: 'Network history results', exact: true });
			await expect(result).toBeVisible();
			await expect(page.locator('html')).toHaveAttribute('data-density', 'compact');
			await page.evaluate(() => window.scrollTo(0, 0));
			const graph = page.getByRole('img', { name: 'Transactions by block-height bucket' });
			await expect(graph).toBeInViewport({ ratio: 1 });
			const first = result.locator('tbody tr').first();
			const bounds = (await first.boundingBox())!;
			expect(bounds.y).toBeLessThan(viewport.width === 390 ? 950 : 850);
			if (viewport.width === 1440) await expect(first).toBeInViewport({ ratio: 1 });
			await expect(result.locator('.coverage-strip')).toContainText(
				'The index is partial, from height 1.'
			);
			await expect(result.locator('.raw-values')).toHaveAttribute('open', '');
			if (viewport.width === 390) {
				for (const control of [
					page.getByRole('button', { name: 'Load network history', exact: true }),
					page.getByRole('button', { name: 'Clear end-block pin', exact: true }),
					page.getByLabel('From height', { exact: true }),
					page.getByRole('combobox', { name: 'Chart metric', exact: true })
				]) {
					const box = (await control.boundingBox())!;
					expect(box.height).toBeGreaterThanOrEqual(44);
					expect(box.width).toBeGreaterThanOrEqual(44);
				}
			}
		});
	}
	for (const theme of ['light', 'dark']) {
		test(`network history remains readable at 320 in ${appearance} ${theme}`, async ({ page }) => {
			await page.setViewportSize({ width: 320, height: 1000 });
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await setup(page);
			await page.getByRole('button', { name: 'Load network history', exact: true }).click();
			await expect(
				page.getByRole('region', { name: 'Network history results', exact: true })
			).toBeVisible();
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
				321
			);
			await expect(
				page.getByRole('region', { name: 'Exact network history values', exact: true })
			).toHaveAttribute('tabindex', '0');
			await expect(page.getByRole('combobox', { name: 'Chart metric', exact: true })).toBeVisible();
		});
	}
}
