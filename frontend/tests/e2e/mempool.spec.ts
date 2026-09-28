import { expect, test, type Page } from '@playwright/test';
import type { MempoolSnapshot } from '../../src/lib/mempool/observations';

const firstId = 'a1'.repeat(32);
const secondId = 'b2'.repeat(32);
function fixture(): MempoolSnapshot {
	return {
		scope: 'configured_node_mempool',
		source: 'configured_primary_node',
		checked_at_ms: 1_800_000_000_000,
		expires_at_ms: 1_800_000_005_000,
		cached: false,
		limit: 100,
		limit_reached: false,
		observed_count: 2,
		items: [
			{
				id: firstId,
				input_count: 2,
				data_input_count: 1,
				output_count: 3,
				size: 1024,
				fee: '9007199254740993'
			},
			{ id: secondId, input_count: 1, data_input_count: 0, output_count: 1, size: null, fee: null }
		]
	};
}
async function setup(page: Page, data = fixture()) {
	const requests: string[] = [];
	await page.route('**/v1/mempool', (route) => {
		requests.push(route.request().url());
		return route.fulfill({ json: { ...data, cached: requests.length > 1 } });
	});
	await page.goto('/mempool');
	return requests;
}

test('mempool loads one observation on entry, preserves exact fees and follows deliberate refreshes', async ({
	page
}) => {
	const requests = await setup(page);
	const list = page.getByRole('region', { name: 'Pending transactions', exact: true });
	await expect(list.getByRole('article')).toHaveCount(2);
	expect(requests).toHaveLength(1);
	await expect(list.getByText('9,007,199.254740993 ERG', { exact: true })).toBeVisible();
	await expect(list.getByText('9,007,199,254,740,993 nanoERG', { exact: true })).toBeVisible();
	await expect(list.getByText('Unknown', { exact: true })).toBeVisible();
	await expect(list.getByText('Not supplied', { exact: true })).toBeVisible();
	await expect(list.getByRole('link', { name: firstId, exact: true })).toHaveAttribute(
		'href',
		'/tx/' + firstId
	);
	const source = page.getByRole('complementary', {
		name: 'Mempool observation details',
		exact: true
	});
	await expect(source).toContainText('Configured primary node');
	await expect(source).toContainText('not a network-wide total');
	const refresh = page.getByRole('button', { name: 'Refresh mempool', exact: true });
	await refresh.focus();
	await page.keyboard.press('Enter');
	await expect(source).toContainText('Reused node observation');
	expect(requests).toHaveLength(2);
});

test('loaded-ID filtering does not query the node or turn no match into an empty mempool claim', async ({
	page
}) => {
	const requests = await setup(page);
	const list = page.getByRole('region', { name: 'Pending transactions', exact: true });
	await expect(list.getByRole('article')).toHaveCount(2);
	await page.getByRole('searchbox', { name: 'Filter loaded transaction IDs' }).fill('A1');
	await expect(list.getByRole('article')).toHaveCount(1);
	await expect(list).toContainText('Showing 1 of 2 loaded transactions');
	await page.getByRole('searchbox', { name: 'Filter loaded transaction IDs' }).fill('ff');
	await expect(
		list.getByRole('heading', { name: 'No loaded ID matches this filter.' })
	).toBeVisible();
	await expect(
		page.getByRole('heading', { name: 'No pending transactions observed.' })
	).toHaveCount(0);
	await list.getByRole('button', { name: 'Clear ID filter' }).click();
	await expect(list.getByRole('article')).toHaveCount(2);
	expect(requests).toHaveLength(1);
});

test('only a successful empty response displays an empty node observation', async ({ page }) => {
	await setup(page, { ...fixture(), items: [], observed_count: 0 });
	await expect(
		page.getByRole('heading', { name: 'No pending transactions observed.' })
	).toBeVisible();
	await expect(
		page.getByText('The node successfully returned an empty list at the observation time.', {
			exact: true
		})
	).toBeVisible();
	await expect(page.getByRole('alert')).toHaveCount(0);
});

for (const [code, heading] of [
	['mempool_not_configured', 'No node configured'],
	['mempool_unsupported', 'Mempool access is unsupported'],
	['mempool_unavailable', 'The configured node is unavailable'],
	['mempool_busy', 'A node check is already in progress'],
	['mempool_invalid_response', 'The node response could not be interpreted']
]) {
	test(code + ' is unavailable evidence, never an empty list', async ({ page }) => {
		let calls = 0;
		await page.route('**/v1/mempool', (route) => {
			calls++;
			return route.fulfill({
				status: 503,
				json: { code, detail: 'The node observation is unavailable.' }
			});
		});
		await page.goto('/mempool');
		await expect(page.getByRole('alert').getByRole('heading', { name: heading })).toBeVisible();
		await expect(
			page.getByRole('region', { name: 'Pending transactions', exact: true })
		).toHaveCount(0);
		await expect(
			page.getByRole('heading', { name: 'No pending transactions observed.' })
		).toHaveCount(0);
		expect(calls).toBe(1);
		await page.getByRole('button', { name: 'Refresh mempool', exact: true }).click();
		await expect(page.getByRole('alert')).toBeVisible();
		expect(calls).toBe(2);
	});
}

test('a failed refresh removes the previous successful snapshot', async ({ page }) => {
	await setup(page);
	await expect(
		page.getByRole('region', { name: 'Pending transactions', exact: true })
	).toBeVisible();
	await page.route('**/v1/mempool', (route) =>
		route.fulfill({
			status: 503,
			json: { code: 'mempool_unavailable', detail: 'Node stopped responding.' }
		})
	);
	await page.getByRole('button', { name: 'Refresh mempool', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('Node stopped responding.');
	await expect(page.getByRole('link', { name: firstId, exact: true })).toHaveCount(0);
	await expect(
		page.getByRole('complementary', { name: 'Mempool observation details', exact: true })
	).toHaveCount(0);
});

test('capacity does not claim proof of additional pending transactions', async ({ page }) => {
	const data = fixture();
	data.items = Array.from({ length: 100 }, (_, index) => ({
		...data.items[0],
		id: index.toString(16).padStart(64, '0')
	}));
	data.observed_count = 100;
	data.limit_reached = true;
	await setup(page, data);
	const list = page.getByRole('region', { name: 'Pending transactions', exact: true });
	await expect(list.getByRole('article')).toHaveCount(100);
	await expect(list).toContainText('Additional pending transactions may exist.');
	await expect(list).not.toContainText('more than 100');
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const theme of ['light', 'dark']) {
		test(
			'mempool stays readable in ' + appearance + ' ' + theme + ' at 320px',
			async ({ page }) => {
				await page.addInitScript(
					({ appearance, theme }) => {
						localStorage.setItem('xp-appearance', appearance);
						localStorage.setItem('xp-theme', theme);
					},
					{ appearance, theme }
				);
				await page.setViewportSize({ width: 320, height: 900 });
				await setup(page);
				await expect(page.getByRole('article')).toHaveCount(2);
				await page.evaluate(() => document.fonts.ready);
				expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
				const refresh = page.getByRole('button', { name: 'Refresh mempool', exact: true });
				await refresh.focus();
				await page.keyboard.press('Enter');
				await expect(
					page.getByRole('complementary', { name: 'Mempool observation details', exact: true })
				).toContainText('Reused node observation');
				await expect(page.getByRole('link', { name: firstId, exact: true })).toBeVisible();
			}
		);
	}
}
