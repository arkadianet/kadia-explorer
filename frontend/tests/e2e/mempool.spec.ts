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
function connectedFixture(): MempoolSnapshot {
	const data = fixture();
	data.connections = {
		scope: 'returned_snapshot_only',
		output_count: 4,
		identified_output_count: 3,
		edge_count: 2,
		edges_truncated: false,
		edges: [
			{ producer_id: secondId, consumer_id: firstId, box_id: 'c3'.repeat(32), kind: 'spend' },
			{ producer_id: secondId, consumer_id: firstId, box_id: 'c3'.repeat(32), kind: 'read' }
		],
		shared_input_count: 1,
		shared_inputs_truncated: false,
		shared_inputs: [
			{
				box_id: 'd4'.repeat(32),
				transaction_count: 2,
				transaction_ids: [firstId, secondId],
				truncated: false
			}
		]
	};
	return data;
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
	await list.getByRole('article').first().locator('summary').click();
	await expect(list.getByText('9,007,199,254,740,993 nanoERG', { exact: true })).toBeVisible();
	await expect(
		list.locator('.transaction-fee').getByText('Unknown', { exact: true })
	).toBeVisible();
	await list.getByRole('article').nth(1).locator('summary').click();
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

test('pending connections select later-row producers without additional API requests', async ({
	page
}) => {
	const requests = await setup(page, connectedFixture());
	const graph = page.getByRole('region', { name: 'Pending connections', exact: true });
	await expect(
		graph.getByRole('list', { name: 'Observed producers' }).getByRole('listitem')
	).toHaveCount(2);
	await expect(graph).toContainText('Read-only reference');
	await expect(graph).toContainText('Some output IDs were not supplied');
	await expect(graph).toContainText(
		'This observation does not identify a replacement, rejection or eventual inclusion.'
	);
	const producer = graph
		.getByRole('button', { name: 'Inspect connections for transaction ' + secondId })
		.first();
	await producer.focus();
	await page.keyboard.press('Enter');
	await expect(page).toHaveURL('/mempool?focus=' + secondId);
	await expect(graph.getByRole('combobox', { name: 'Select transaction connections' })).toHaveValue(
		secondId
	);
	await expect(
		graph.getByRole('list', { name: 'Observed consumers' }).getByRole('listitem')
	).toHaveCount(2);
	await expect(
		graph.getByText('No producer reference is included for this transaction.')
	).toBeVisible();
	expect(requests).toHaveLength(1);
	await page
		.getByRole('button', { name: 'Inspect connections for listed transaction ' + firstId })
		.click();
	await expect(page).toHaveURL('/mempool?focus=' + firstId);
	expect(requests).toHaveLength(1);
});

test('shared focus outside the page is not described as missing from the mempool', async ({
	page
}) => {
	let calls = 0;
	await page.route('**/v1/mempool', (route) => {
		calls++;
		return route.fulfill({ json: connectedFixture() });
	});
	const absent = 'f5'.repeat(32);
	await page.goto('/mempool?focus=' + absent);
	const graph = page.getByRole('region', { name: 'Pending connections', exact: true });
	await expect(graph).toContainText(
		'The requested transaction is not in this loaded snapshot. This does not establish its current status.'
	);
	await expect(graph.getByRole('link', { name: 'Check transaction status' })).toHaveAttribute(
		'href',
		'/tx/' + absent
	);
	await graph.getByRole('combobox').selectOption(firstId);
	await expect(graph.getByRole('list', { name: 'Observed producers' })).toBeVisible();
	expect(calls).toBe(1);
});

test('a shared focus and a reloaded local selection inspect the same transaction', async ({
	page
}) => {
	let calls = 0;
	await page.route('**/v1/mempool', (route) => {
		calls++;
		return route.fulfill({ json: connectedFixture() });
	});
	await page.goto('/mempool?focus=' + secondId);
	const graph = page.getByRole('region', { name: 'Pending connections', exact: true });
	await expect(graph.getByRole('list', { name: 'Observed consumers' })).toBeVisible();
	await graph.getByRole('combobox').selectOption(firstId);
	await expect(graph.getByRole('list', { name: 'Observed producers' })).toBeVisible();
	expect(calls).toBe(1);
	await page.reload();
	await expect(graph.getByRole('combobox')).toHaveValue(firstId);
	await expect(graph.getByRole('list', { name: 'Observed producers' })).toBeVisible();
	expect(calls).toBe(2);
});

test('an invalid focus remains a selection problem rather than an API request', async ({
	page
}) => {
	let calls = 0;
	await page.route('**/v1/mempool', (route) => {
		calls++;
		return route.fulfill({ json: connectedFixture() });
	});
	await page.goto('/mempool?focus=not-an-id');
	const graph = page.getByRole('region', { name: 'Pending connections', exact: true });
	await expect(graph).toContainText('The focus must be a 64-character lowercase transaction ID.');
	await expect(graph.getByRole('link')).toHaveCount(0);
	expect(calls).toBe(1);
});

test('older APIs disclose unavailable connections and failed refresh removes all graph evidence', async ({
	page
}) => {
	await setup(page);
	const graph = page.getByRole('region', { name: 'Pending connections', exact: true });
	await expect(graph).toContainText('This API version does not provide pending connections.');
	await page.route('**/v1/mempool', (route) => route.fulfill({ json: connectedFixture() }));
	await page.getByRole('button', { name: 'Refresh mempool', exact: true }).click();
	await expect(graph.getByRole('list', { name: 'Observed producers' })).toBeVisible();
	await page.route('**/v1/mempool', (route) =>
		route.fulfill({
			status: 503,
			json: { code: 'mempool_unavailable', detail: 'Node unavailable' }
		})
	);
	await page.getByRole('button', { name: 'Refresh mempool', exact: true }).click();
	await expect(page.getByRole('alert')).toBeVisible();
	await expect(graph).toHaveCount(0);
});

test('connection display expands locally and retains capped-snapshot coverage', async ({
	page
}) => {
	const data = fixture();
	data.items[1].output_count = 6;
	data.items[0].input_count = 6;
	data.connections = {
		scope: 'returned_snapshot_only',
		output_count: 9,
		identified_output_count: 6,
		edge_count: 7,
		edges_truncated: true,
		edges: Array.from({ length: 6 }, (_, i) => ({
			producer_id: secondId,
			consumer_id: firstId,
			box_id: i.toString(16).padStart(64, '0'),
			kind: 'spend' as const
		})),
		shared_input_count: 1,
		shared_inputs_truncated: true,
		shared_inputs: []
	};
	const requests = await setup(page, data);
	const graph = page.getByRole('region', { name: 'Pending connections', exact: true });
	const list = graph.getByRole('list', { name: 'Observed producers' });
	await expect(list.getByRole('listitem')).toHaveCount(4);
	await graph.getByRole('button', { name: 'Show all 6 producer references' }).click();
	await expect(list.getByRole('listitem')).toHaveCount(6);
	await expect(graph).toContainText('Showing at most 6 of 7 references across this snapshot');
	await expect(graph).toContainText('Shared-input details are limited');
	await graph.getByRole('button', { name: 'Show fewer producer references' }).click();
	await expect(list.getByRole('listitem')).toHaveCount(4);
	expect(requests).toHaveLength(1);
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
						localStorage.setItem('xp-density', 'compact');
					},
					{ appearance, theme }
				);
				await page.setViewportSize({ width: 320, height: 900 });
				await setup(page, connectedFixture());
				await expect(page.getByRole('article')).toHaveCount(2);
				await page.evaluate(() => document.fonts.ready);
				expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
				const refresh = page.getByRole('button', { name: 'Refresh mempool', exact: true });
				await refresh.focus();
				await page.keyboard.press('Enter');
				await expect(
					page.getByRole('complementary', { name: 'Mempool observation details', exact: true })
				).toContainText('Reused node observation');
				const graph = page.getByRole('region', { name: 'Pending connections', exact: true });
				await expect(graph.locator('details.inspector')).not.toHaveAttribute('open');
				await expect(graph.getByRole('combobox')).toBeHidden();
				const inspect = page.getByRole('button', {
					name: 'Inspect connections for listed transaction ' + secondId
				});
				const touchBounds = await inspect.boundingBox();
				expect(touchBounds?.height).toBeGreaterThanOrEqual(44);
				await inspect.focus();
				await page.keyboard.press('Enter');
				await expect(graph.locator('details.inspector')).toHaveAttribute('open', '');
				await expect(graph.getByRole('combobox')).toHaveValue(secondId);
				await expect(graph.getByRole('list', { name: 'Observed consumers' })).toBeVisible();
				expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
				await expect(
					page
						.getByRole('region', { name: 'Pending transactions', exact: true })
						.getByRole('link', { name: firstId, exact: true })
				).toBeVisible();
			}
		);
	}
}

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const density of ['standard', 'compact']) {
		test(`${appearance} ${density} keeps pending records beside the bounded desktop inspector`, async ({
			page
		}) => {
			await page.addInitScript(
				({ appearance, density }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-density', density);
				},
				{ appearance, density }
			);
			await page.setViewportSize({ width: 1440, height: 1000 });
			const requests = await setup(page, connectedFixture());
			const list = page.getByRole('region', { name: 'Pending transactions', exact: true });
			const graph = page.getByRole('region', { name: 'Pending connections', exact: true });
			await expect(graph.getByRole('combobox')).toBeVisible();
			await expect(page.locator('html')).toHaveAttribute('data-appearance', appearance);
			await expect(page.locator('html')).toHaveAttribute('data-density', density);
			await page.evaluate(() => document.fonts.ready);
			// The SPA can expose its controls before the route's first styled paint.
			// Retry the complete geometry contract rather than measuring that intermediate frame.
			await expect(async () => {
				const record = await list.getByRole('article').first().boundingBox();
				const listBounds = await list.boundingBox();
				const inspectorBounds = await graph.boundingBox();
				expect(record!.y).toBeLessThan(600);
				expect(record!.height).toBeLessThan(125);
				expect(Math.abs(listBounds!.y - inspectorBounds!.y)).toBeLessThan(8);
				if (appearance === 'atelier')
					expect(inspectorBounds!.x + inspectorBounds!.width).toBeLessThanOrEqual(listBounds!.x);
				else expect(listBounds!.x + listBounds!.width).toBeLessThanOrEqual(inspectorBounds!.x);
			}).toPass({ timeout: 5_000 });
			await list.getByRole('article').nth(1).getByRole('button').click();
			await expect(graph.getByRole('combobox')).toHaveValue(secondId);
			await expect(graph.getByRole('list', { name: 'Observed consumers' })).toBeVisible();
			expect(requests).toHaveLength(1);
		});
	}
}
