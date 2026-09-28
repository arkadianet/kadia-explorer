import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { BoxDto, PageDto, TxDto } from '../../src/lib/api/types';
const fixture = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/apps/rosen-erg-cardano.json', import.meta.url),
		'utf8'
	)
) as { deposit: BoxDto; sourceTx: TxDto; event: BoxDto; eventSpend: TxDto };
const anchor = { height: 1880000, block_id: 'a'.repeat(64) };
function eventPage(items = [fixture.event]): PageDto<BoxDto> {
	return {
		items,
		next_cursor: null,
		next_snapshot: null,
		consistency: 'strict',
		anchor,
		observed_anchor: anchor
	};
}
async function setup(page: Page, events = eventPage()) {
	const requests: string[] = [];
	await page.route('**/v1/boxes/' + fixture.deposit.id, (route) => {
		requests.push('deposit');
		return route.fulfill({ json: fixture.deposit });
	});
	await page.route('**/v1/txs/' + fixture.sourceTx.id, (route) => {
		requests.push('source');
		return route.fulfill({ json: fixture.sourceTx });
	});
	await page.route('**/v1/txs/' + fixture.eventSpend.id, (route) => {
		requests.push('spend');
		return route.fulfill({ json: fixture.eventSpend });
	});
	await page.route('**/v1/registers/R5/*/boxes?*', (route) => {
		requests.push('events');
		return route.fulfill({ json: events });
	});
	return requests;
}
async function load(page: Page) {
	await page.goto('/applications/rosen?box=' + fixture.deposit.id);
	await expect(page.getByLabel('Deposit box ID', { exact: true })).toHaveValue(fixture.deposit.id);
	await page.getByRole('button', { name: 'Load deposit', exact: true }).click();
	return page.getByRole('region', { name: 'Rosen bridge workflow', exact: true });
}
test('Rosen inspection stays opt-in and exposes a recorded payout with an explicit external boundary', async ({
	page
}) => {
	const requests = await setup(page);
	await page.goto('/applications/rosen?box=' + fixture.deposit.id);
	await expect(page.getByLabel('Deposit box ID', { exact: true })).toHaveValue(fixture.deposit.id);
	expect(requests).toEqual([]);
	await page.getByRole('button', { name: 'Load deposit', exact: true }).click();
	const workflow = page.getByRole('region', { name: 'Rosen bridge workflow', exact: true });
	await expect(workflow).toContainText('418.527417582');
	expect(requests).toEqual(['deposit']);
	const thirdParty: string[] = [];
	page.on('request', (request) => {
		if (/rosen\.tech|cardanoscan\.io|etherscan\.io|bscscan\.com/.test(request.url()))
			thirdParty.push(request.url());
	});
	await workflow.getByRole('button', { name: 'Inspect Rosen workflow', exact: true }).click();
	await expect(
		workflow.getByText('Reward-distribution stage observed', { exact: true })
	).toBeVisible();
	expect(requests).toEqual(['deposit', 'source', 'events', 'spend', 'source']);
	await expect(workflow).toContainText('57 declared watcher commitments');
	await expect(
		workflow.getByRole('heading', { name: 'External confirmation not checked', exact: true })
	).toBeVisible();
	await expect(
		workflow.getByRole('link', { name: 'Open external cardano transaction' })
	).toHaveAttribute(
		'href',
		'https://cardanoscan.io/transaction/852a604ac7e63c595277a6e6074354ba0df00bfaf0ed4b233b727e9b76b2ac19'
	);
	expect(thirdParty).toEqual([]);
});
test('missing events remain missing observations, not rejected or completed transfers', async ({
	page
}) => {
	const requests = await setup(page, eventPage([]));
	const workflow = await load(page);
	await workflow.getByRole('button', { name: 'Inspect Rosen workflow' }).click();
	await expect(
		workflow.getByRole('heading', { name: 'No matching event in this indexed result' })
	).toBeVisible();
	await expect(workflow).toContainText('This is not a rejection');
	expect(requests).toEqual(['deposit', 'source', 'events', 'source']);
});
test('multiple events remain inspectable without selecting a payout', async ({ page }) => {
	const requests = await setup(
		page,
		eventPage([fixture.event, { ...fixture.event, id: 'b'.repeat(64) }])
	);
	const workflow = await load(page);
	await workflow.getByRole('button', { name: 'Inspect Rosen workflow' }).click();
	await expect(
		workflow.getByText('Multiple or incomplete event records', { exact: true })
	).toBeVisible();
	expect(requests).not.toContain('spend');
	await expect(workflow.getByRole('link', { name: /Open external/ })).toHaveCount(0);
});
test('a refresh failure clears previous payout evidence and supports explicit retry', async ({
	page
}) => {
	await setup(page);
	const workflow = await load(page);
	await workflow.getByRole('button', { name: 'Inspect Rosen workflow' }).click();
	await expect(
		workflow.getByText('Reward-distribution stage observed', { exact: true })
	).toBeVisible();
	await page.route('**/v1/registers/R5/*/boxes?*', (route) =>
		route.fulfill({ status: 503, json: { detail: 'unavailable' } })
	);
	await workflow.getByRole('button', { name: 'Refresh Rosen workflow' }).click();
	await expect(workflow.getByRole('alert')).toContainText('HTTP 503');
	await expect(workflow.getByRole('link', { name: /Open external/ })).toHaveCount(0);
	await expect(workflow.getByRole('button', { name: 'Refresh Rosen workflow' })).toBeEnabled();
});
test('a changed deposit inclusion clears correlated event evidence', async ({ page }) => {
	await setup(page);
	let reads = 0;
	await page.route('**/v1/txs/' + fixture.sourceTx.id, (route) =>
		route.fulfill({
			json: ++reads === 1 ? fixture.sourceTx : { ...fixture.sourceTx, block_id: 'b'.repeat(64) }
		})
	);
	const workflow = await load(page);
	await workflow.getByRole('button', { name: 'Inspect Rosen workflow' }).click();
	await expect(workflow.getByRole('alert')).toContainText('inclusion changed');
	await expect(workflow.getByRole('link', { name: /Open external/ })).toHaveCount(0);
});
test('editing the standalone box selection removes loaded workflow evidence', async ({ page }) => {
	await setup(page);
	const workflow = await load(page);
	await expect(workflow).toBeVisible();
	await page.getByLabel('Deposit box ID', { exact: true }).fill('b'.repeat(64));
	await expect(workflow).toHaveCount(0);
});
for (const appearance of ['original', 'aurora', 'atelier', 'prism']) {
	for (const theme of ['light', 'dark']) {
		test(`Rosen ${appearance} ${theme} keeps evidence and controls usable at 320px`, async ({
			page
		}) => {
			await page.setViewportSize({ width: 320, height: 850 });
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await setup(page);
			const workflow = await load(page);
			await workflow.getByRole('button', { name: 'Inspect Rosen workflow' }).click();
			await expect(
				workflow.getByText('Reward-distribution stage observed', { exact: true })
			).toBeVisible();
			await expect(
				workflow.getByRole('link', { name: /Open external cardano transaction/ })
			).toBeVisible();
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
				321
			);
			await expect(workflow.getByRole('button', { name: 'Refresh Rosen workflow' })).toBeEnabled();
		});
	}
}
