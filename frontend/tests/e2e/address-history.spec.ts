import { expect, test, type Page } from '@playwright/test';
import { MOCK_ADDRESS, UNKNOWN_ADDRESS } from './data.ts';
import type { HistoryBalance, HistoryBoxes } from '../../src/lib/addresses/history.ts';

const height = 1_866_000;
const block = 'a'.repeat(64);
const token = 'b'.repeat(64);
const anchor = { height, block_id: block };
const balance: HistoryBalance = {
	address: MOCK_ADDRESS,
	at: anchor,
	indexed_height: height + 2,
	complete: true,
	balance: {
		nano: '9007199254740993123',
		box_count: 1,
		tokens: [{ token_id: token, amount: '900719925474099312345' }]
	}
};
const boxes: HistoryBoxes = {
	at: anchor,
	items: [
		{
			box_id: 'c'.repeat(64),
			inclusion_height: height - 7,
			nano: balance.balance.nano,
			tokens: balance.balance.tokens
		}
	],
	next_cursor: null
};

async function mockHistory(page: Page) {
	await page.route('**/v1/addresses/*/balance/at?**', (route) => route.fulfill({ json: balance }));
	await page.route('**/v1/addresses/*/boxes/at?**', (route) => route.fulfill({ json: boxes }));
}
async function inspect(page: Page) {
	await page.goto(`/address/${MOCK_ADDRESS}?at_height=${height}#history`);
	await page.getByRole('button', { name: 'Inspect snapshot', exact: true }).click();
}

test('history URLs are opt-in and exact balances carry a pinned box-page anchor', async ({
	page
}) => {
	const requests: URL[] = [];
	await mockHistory(page);
	page.on('request', (request) => {
		if (/\/(balance|boxes)\/at$/.test(new URL(request.url()).pathname))
			requests.push(new URL(request.url()));
	});
	await page.goto(`/address/${MOCK_ADDRESS}?at_height=${height}&at_block=${block}#history`);
	await expect(page.getByRole('heading', { name: 'Choose the moment to inspect.' })).toBeVisible();
	expect(requests).toHaveLength(0);
	await page.getByRole('button', { name: 'Inspect snapshot', exact: true }).click();
	const summary = page.getByRole('region', { name: 'Historical balances', exact: true });
	await expect(summary.getByText('9,007,199,254.740993123', { exact: true })).toBeVisible();
	await expect(summary.getByText('900,719,925,474,099,312,345', { exact: true })).toBeVisible();
	await expect(summary.getByText('Complete anchored balance', { exact: true })).toBeVisible();
	await expect(page.getByText('All historical box pages are loaded.')).toBeVisible();
	expect(requests).toHaveLength(2);
	expect(requests.every((url) => url.searchParams.get('block_id') === block)).toBe(true);
	await expect(page.getByRole('link', { name: 'Pinned snapshot link ↗' })).toHaveAttribute(
		'href',
		`/address/${MOCK_ADDRESS}?at_height=${height}&at_block=${block}#history`
	);
	await page.reload();
	await expect(page.getByRole('heading', { name: 'Choose the moment to inspect.' })).toBeVisible();
	expect(requests).toHaveLength(2);
});

test('genesis and an unseen address can have a checked zero balance without fabricated block ID', async ({
	page
}) => {
	await page.route('**/v1/addresses/*/balance/at?**', (route) =>
		route.fulfill({
			json: {
				...balance,
				address: UNKNOWN_ADDRESS,
				at: { height: 0, block_id: null },
				balance: { nano: '0', box_count: 0, tokens: [] }
			}
		})
	);
	await page.route('**/v1/addresses/*/boxes/at?**', (route) =>
		route.fulfill({ json: { at: { height: 0, block_id: null }, items: [], next_cursor: null } })
	);
	await page.goto(`/address/${UNKNOWN_ADDRESS}?at_height=0#history`);
	await page.getByRole('button', { name: 'Inspect snapshot', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'Before block 1' })).toBeVisible();
	await expect(page.getByText('No boxes or balance at this anchor.')).toBeVisible();
	await expect(page.getByRole('link', { name: 'Pinned snapshot link ↗' })).toHaveAttribute(
		'href',
		`/address/${UNKNOWN_ADDRESS}?at_height=0#history`
	);
});

for (const [code, status, wording] of [
	['history_unavailable', 503, 'requires a complete mainnet index'],
	['height_not_indexed', 400, 'above the server’s indexed tip'],
	['', 404, 'not available on this server yet']
] as const)
	test(`history ${code || 'legacy route'} remains unavailable rather than zero`, async ({
		page
	}) => {
		await page.route('**/v1/addresses/*/balance/at?**', (route) =>
			route.fulfill({ status, json: { code, detail: 'Unavailable' } })
		);
		await inspect(page);
		await expect(page.getByRole('alert')).toContainText(wording);
		await expect(
			page.getByRole('region', { name: 'Historical balances', exact: true })
		).toHaveCount(0);
	});

test('a budget-limited balance uses incomplete page totals and continues an empty candidate page', async ({
	page
}) => {
	await page.route('**/v1/addresses/*/balance/at?**', (route) =>
		route.fulfill({ status: 422, json: { code: 'history_scan_limit', detail: 'Too much work' } })
	);
	const requests: URL[] = [];
	await page.route('**/v1/addresses/*/boxes/at?**', (route) => {
		const url = new URL(route.request().url());
		requests.push(url);
		return route.fulfill({
			json: url.searchParams.has('cursor')
				? boxes
				: { at: anchor, items: [], next_cursor: 'older-candidates' }
		});
	});
	await inspect(page);
	await expect(page.getByText('Loaded boxes only · incomplete', { exact: true })).toBeVisible();
	await expect(page.getByText('No boxes or balance at this anchor.')).toHaveCount(0);
	await page.getByRole('button', { name: 'Continue historical scan', exact: true }).click();
	await expect(page.getByText('Complete anchored balance', { exact: true })).toBeVisible();
	expect(requests[1].searchParams.get('cursor')).toBe('older-candidates');
	expect(requests[1].searchParams.get('block_id')).toBe(block);
});

test('a changed historical anchor clears balances and pages until an explicit restart', async ({
	page
}) => {
	await mockHistory(page);
	let restart = false;
	await page.route('**/v1/addresses/*/boxes/at?**', (route) => {
		if (new URL(route.request().url()).searchParams.has('cursor'))
			return route.fulfill({ status: 409, json: { code: 'snapshot_changed', detail: 'Changed' } });
		return route.fulfill({ json: { ...boxes, next_cursor: restart ? null : 'next' } });
	});
	await inspect(page);
	await page.getByRole('button', { name: 'Load more historical boxes', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('The old results were cleared');
	await expect(page.getByRole('region', { name: 'Historical balances', exact: true })).toHaveCount(
		0
	);
	await expect(page.locator('.historical-box')).toHaveCount(0);
	restart = true;
	await page.getByRole('button', { name: 'Restart at this height', exact: true }).click();
	await expect(page.getByText('All historical box pages are loaded.')).toBeVisible();
});

test('an invalid shared selector is rejected before any historical request', async ({ page }) => {
	let calls = 0;
	page.on('request', (request) => {
		if (/\/(balance|boxes)\/at$/.test(new URL(request.url()).pathname)) calls++;
	});
	await page.goto(`/address/${MOCK_ADDRESS}?at_height=4294967296#history`);
	await page.getByRole('button', { name: 'Inspect snapshot', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('whole block height');
	expect(calls).toBe(0);
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora'])
	for (const theme of ['light', 'dark'])
		test(`${appearance} ${theme} history fits 320px with readable exact amounts`, async ({
			page
		}) => {
			await page.setViewportSize({ width: 320, height: 900 });
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await mockHistory(page);
			await inspect(page);
			await expect(page.getByText('All historical box pages are loaded.')).toBeVisible();
			await page.getByText('1 raw token balance', { exact: true }).click();
			await expect(
				page
					.getByRole('region', { name: 'Historical unspent boxes' })
					.getByText('900,719,925,474,099,312,345 raw units', { exact: true })
			).toBeVisible();
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
			).toBe(true);
		});
