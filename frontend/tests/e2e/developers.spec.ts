import { expect, test } from '@playwright/test';

test('a shared query stays idle until run, encodes parameters and shows actual errors', async ({
	page
}) => {
	const requests: string[] = [];
	await page.route('**/v1/tokens/search?**', async (route) => {
		requests.push(route.request().url());
		await route.fulfill({
			status: 503,
			json: { code: 'token_search_preparing', detail: 'Names are preparing' }
		});
	});
	await page.goto('/developers?endpoint=token-search&p.q=SigUSD');
	await expect(page.getByRole('textbox', { name: 'Token name', exact: true })).toHaveValue(
		'SigUSD'
	);
	await expect(page.getByText('A request away.', { exact: true })).toBeVisible();
	expect(requests).toHaveLength(0);
	await page.getByRole('button', { name: 'Run request' }).click();
	await expect(page.getByText('HTTP 503', { exact: true })).toBeVisible();
	await expect(page.getByLabel('JSON response')).toContainText('token_search_preparing');
	expect(new URL(requests[0]).searchParams.get('q')).toBe('SigUSD');
	await expect(page.getByRole('button', { name: 'Next page', exact: true })).toHaveCount(0);
});

test('continuation carries both snapshot and cursor and does not carry them into a different endpoint', async ({
	page
}) => {
	const requests: URL[] = [];
	await page.route('**/v1/tx-summaries?**', async (route) => {
		requests.push(new URL(route.request().url()));
		await route.fulfill({
			json: {
				items: [],
				consistency: 'strict',
				next_cursor: requests.length === 1 ? '12' : null,
				next_snapshot: requests.length === 1 ? 'snapshot-token' : null
			}
		});
	});
	await page.goto('/developers?endpoint=summaries');
	await page.getByRole('button', { name: 'Run request' }).click();
	await page.getByRole('button', { name: 'Next page', exact: true }).click();
	await expect(page.getByLabel('Cursor', { exact: true })).toHaveValue('12');
	await expect(page.getByLabel('Snapshot token', { exact: true })).toHaveValue('snapshot-token');
	await expect.poll(() => requests.length).toBe(2);
	expect(requests[1].searchParams.get('cursor')).toBe('12');
	expect(requests[1].searchParams.get('snapshot')).toBe('snapshot-token');
	await page.getByRole('button', { name: 'Block list', exact: true }).click();
	await expect(page.getByLabel('Cursor', { exact: true })).toHaveValue('');
	await expect(page.getByText('A request away.', { exact: true })).toBeVisible();
});

test('HTTP 409 can restart from page one and download retains exact integers', async ({ page }) => {
	await page.route('**/v1/blocks?**', async (route) => {
		const continuation = new URL(route.request().url()).searchParams.has('cursor');
		await route.fulfill({
			status: continuation ? 409 : 200,
			json: continuation
				? { code: 'snapshot_changed' }
				: { fee: '9007199254740993', next_cursor: null }
		});
	});
	await page.goto('/developers?endpoint=blocks&p.cursor=12&p.snapshot=old');
	await page.getByRole('button', { name: 'Run request' }).click();
	await expect(page.getByText('HTTP 409', { exact: true })).toBeVisible();
	await page.getByRole('button', { name: 'Restart first page' }).click();
	await expect(page.getByLabel('JSON response')).toContainText('9007199254740993');
	await expect(page.getByLabel('Cursor', { exact: true })).toHaveValue('');
	const download = page.waitForEvent('download');
	await page.getByRole('button', { name: 'Download JSON' }).click();
	const stream = await (await download).createReadStream();
	let body = '';
	for await (const chunk of stream!) body += chunk.toString();
	expect(JSON.parse(body).fee).toBe('9007199254740993');
});

test('changing endpoints while a request is pending discards its eventual response', async ({
	page
}) => {
	let release!: () => void;
	let started!: () => void;
	const pending = new Promise<void>((resolve) => {
		release = resolve;
	});
	const requestStarted = new Promise<void>((resolve) => {
		started = resolve;
	});
	await page.route('**/v1/network/summary', async (route) => {
		started();
		await pending;
		await route.fulfill({ json: { stale_marker: true } }).catch(() => {});
	});
	await page.goto('/developers?endpoint=network');
	await page.getByRole('button', { name: 'Run request' }).click();
	await requestStarted;
	await page.getByRole('button', { name: 'Block list', exact: true }).click();
	release();
	await expect(page.getByText('A request away.', { exact: true })).toBeVisible();
	await expect(page.getByLabel('JSON response')).toHaveCount(0);
});

for (const appearance of ['original', 'aurora', 'atelier', 'prism']) {
	for (const theme of ['light', 'dark']) {
		test(`API playground fits ${appearance} ${theme} at phone, tablet and desktop sizes`, async ({
			page
		}) => {
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			for (const width of [320, 1024, 1440]) {
				await page.setViewportSize({ width, height: 900 });
				await page.goto('/developers?endpoint=network');
				await page.getByRole('button', { name: 'Run request' }).click();
				await expect(page.getByText('HTTP 200', { exact: true })).toBeVisible();
				expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
				await expect(page.getByLabel('cURL example')).toContainText('/v1/network/summary');
			}
		});
	}
}
