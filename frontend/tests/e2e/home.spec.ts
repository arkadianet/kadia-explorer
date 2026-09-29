import { expect, test } from '@playwright/test';
import { BLOCK_HEIGHTS, TIP } from './data.ts';

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const theme of ['light', 'dark']) {
		test(`${appearance} ${theme} homepage puts pending activity before the block panels and fits mobile`, async ({
			page
		}) => {
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await page.setViewportSize({ width: 1440, height: 1000 });
			await page.goto('/');
			const preview = page.getByRole('region', { name: 'Pending transactions' });
			await expect(preview.locator('.pending-sample li')).toHaveCount(3);
			const bounds = await preview.boundingBox();
			const panels = await page.locator('.panels').first().boundingBox();
			expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(panels!.y);
			await page.setViewportSize({ width: 320, height: 800 });
			const narrow = await preview.boundingBox();
			expect(narrow!.x).toBeGreaterThanOrEqual(0);
			expect(narrow!.x + narrow!.width).toBeLessThanOrEqual(320);
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
		});
	}
}

test('homepage exposes a bounded mempool sample without transaction detail fan-out', async ({
	page
}) => {
	const requests: string[] = [];
	page.on('request', (request) => requests.push(new URL(request.url()).pathname));
	await page.goto('/');
	const preview = page.getByRole('region', { name: 'Pending transactions' });
	await expect(preview.locator('.pending-sample li')).toHaveCount(3);
	await expect(preview).toContainText('0.0011 ERG fee');
	await expect(preview).toContainText('Fee unavailable');
	await expect(preview.locator('time')).toContainText('UTC');
	await expect(preview.getByRole('link', { name: 'Explore mempool' })).toHaveAttribute(
		'href',
		'/mempool'
	);
	expect(requests.filter((path) => path === '/v1/mempool')).toHaveLength(1);
	expect(requests.filter((path) => /^\/v1\/txs\//.test(path))).toEqual([]);
});

test('homepage remains usable during an unavailable mempool check and supports retry', async ({
	page
}) => {
	let attempts = 0;
	await page.route('**/v1/mempool', (route) =>
		++attempts === 1
			? route.fulfill({
					status: 503,
					json: { code: 'mempool_unavailable', detail: 'The node is offline.' }
				})
			: route.continue()
	);
	await page.goto('/');
	const preview = page.getByRole('region', { name: 'Pending transactions' });
	await expect(page.getByRole('heading', { name: 'Recent blocks' })).toBeVisible();
	await expect(preview).toContainText('Observation unavailable');
	await expect(preview.locator('.preview-count')).toHaveCount(0);
	await preview.getByRole('button', { name: 'Refresh pending preview' }).click();
	await expect(preview.locator('.pending-sample li')).toHaveCount(3);
	await expect(preview).not.toContainText('Observation unavailable');
});

test('an empty mempool observation differs from an unavailable observation', async ({ page }) => {
	await page.route('**/v1/mempool', async (route) => {
		const response = await route.fetch();
		const data = await response.json();
		await route.fulfill({
			json: { ...data, connections: undefined, items: [], observed_count: 0 }
		});
	});
	await page.goto('/');
	const preview = page.getByRole('region', { name: 'Pending transactions' });
	await expect(preview).toContainText('No pending transactions returned in this observation.');
	await expect(preview.locator('.preview-count strong')).toHaveText('0');
});

test('home renders the hero and every panel with rows from the mock', async ({ page }) => {
	await page.goto('/');

	// The hero states the chain's tip, and the panels below it name their own sections.
	await expect(page.getByRole('heading', { level: 1, name: 'Ergo mainnet' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Recent blocks' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Live transactions' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Rent maturing soon' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Largest holders' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Go deeper' })).toBeVisible();

	// The tip block heads the "Recent blocks" table.
	const blocks = page.locator('section.panel', {
		has: page.getByRole('heading', { name: 'Recent blocks' })
	});
	await expect(blocks.locator('tbody tr').first().locator('td').first()).toHaveText(String(TIP));
	// The mock serves 13 blocks; the panel shows the newest six of them.
	await expect(blocks.locator('tbody tr')).toHaveCount(6);
	for (const h of BLOCK_HEIGHTS) {
		await expect(blocks.getByRole('link', { name: String(h), exact: true })).toBeVisible();
	}

	// Compact rows show observed counts and fees without expanding transaction boxes.
	const txs = page.locator('section.panel', {
		has: page.getByRole('heading', { name: 'Live transactions' })
	});
	await expect(txs.locator('.txlist li')).toHaveCount(5);
	// A summary does not infer payment intent or a transferred output total.
	await expect(txs.locator('.tx-kind').first()).toHaveText(/^\d+ in · \d+ out$/);

	const rent = page.locator('section.panel', {
		has: page.getByRole('heading', { name: 'Rent maturing soon' })
	});
	await expect(rent.locator('tbody tr').first()).toBeVisible();

	// "Matures in" counts down from the *indexed* tip (not the node's `best`), so a box that
	// has not matured yet must read as a positive number of blocks — a `best`-based countdown
	// would disagree with /rent and, under lag, could even go negative.
	const maturesIn = rent.locator('tbody tr').first().locator('td').nth(2);
	await expect(maturesIn).toHaveText(/^\d+ blocks$/);
	expect(Number((await maturesIn.innerText()).replace(' blocks', ''))).toBeGreaterThan(0);

	// No lag banner in the default (healthy) mock status.
	await expect(page.getByText(/blocks behind the node/)).toHaveCount(0);
});

test('a block row links through to the block page', async ({ page }) => {
	await page.goto('/');
	await page
		.getByRole('link', { name: String(TIP), exact: true })
		.first()
		.click();
	await expect(page).toHaveURL(`/blocks/${TIP}`);
	await expect(page.getByRole('heading', { name: `Block ${TIP}` })).toBeVisible();
});

test('failed statistic dependencies show unavailable instead of zero', async ({ page }) => {
	await page.route('**/v1/network/summary', (route) =>
		route.fulfill({ status: 503, json: { detail: 'offline' } })
	);
	await page.route('**/v1/rent/upcoming?*', (route) =>
		route.fulfill({ status: 503, json: { detail: 'offline' } })
	);
	await page.goto('/');
	for (const label of ['Transactions', 'Blocks', 'Transaction fees', 'Storage rent']) {
		await expect(
			page
				.locator('.stat')
				.filter({ has: page.locator('.stat-label', { hasText: label }) })
				.locator('.stat-value')
		).toContainText('Unavailable');
	}
});

test('capped rent totals are visible lower bounds', async ({ page }) => {
	await page.route('**/v1/rent/upcoming?*', async (route) => {
		const response = await route.fetch();
		const body = await response.json();
		await route.fulfill({ json: { ...body, complete: false } });
	});
	await page.goto('/');
	const card = page.locator('.stat').filter({ hasText: 'Storage rent' });
	await expect(card.locator('.stat-value')).toContainText('≥');
	await expect(card).toContainText('incomplete');
});

test('overview uses bounded summaries instead of expanded transaction and block lists', async ({
	page
}) => {
	const forbidden: string[] = [];
	page.on('request', (request) => {
		const url = new URL(request.url());
		if (url.pathname === '/v1/txs' || url.pathname === '/v1/blocks') forbidden.push(url.pathname);
	});
	await page.goto('/');
	await expect(page.getByRole('heading', { name: 'Recent blocks' })).toBeVisible();
	await expect(page.locator('.txlist li')).toHaveCount(5);
	await expect(page.locator('.stats')).toContainText('latest 13 indexed blocks');
	expect(forbidden).toEqual([]);
});

test('fee precision and sampled history remain explicit', async ({ page }) => {
	await page.route('**/v1/network/summary', async (route) => {
		const response = await route.fetch();
		await route.fulfill({
			json: { ...(await response.json()), fees: '1999123456', partial_from: 123 }
		});
	});
	await page.goto('/');
	const card = page.locator('.stat').filter({ hasText: 'Transaction fees' });
	await expect(card.locator('.stat-value')).toContainText('≈ 1.999');
	await expect(card.locator('.stat-value')).toHaveAttribute('title', /1999123456 nanoERG/);
	await expect(page.locator('.window-caption')).toContainText(
		'Hourly charts cover these blocks only'
	);
	await expect(page.locator('.window-caption')).toContainText(
		'Indexed history begins at block 123'
	);
});

test('a successful live refresh recovers from initial summary failure', async ({ page }) => {
	let summaries = 0;
	let statuses = 0;
	await page.route('**/v1/tx-summaries?*', async (route) => {
		if (++summaries === 1)
			return route.fulfill({ status: 503, json: { detail: 'summary temporarily unavailable' } });
		return route.continue();
	});
	await page.route('**/v1/status', async (route) => {
		const response = await route.fetch();
		const data = await response.json();
		await route.fulfill({ json: { ...data, indexed: data.indexed + (++statuses > 2 ? 1 : 0) } });
	});
	await page.goto('/');
	const panel = page
		.locator('section.panel')
		.filter({ has: page.getByRole('heading', { name: 'Live transactions' }) });
	await expect(panel.getByText('summary temporarily unavailable', { exact: false })).toBeVisible();
	await expect(panel.locator('.txlist li').first()).toBeVisible({ timeout: 12000 });
	await expect(panel.getByText('summary temporarily unavailable', { exact: false })).toHaveCount(0);
});
