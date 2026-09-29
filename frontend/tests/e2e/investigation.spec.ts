import { expect, test, type Page } from '@playwright/test';
import type { BoxDto, TxDto } from '../../src/lib/api/types.ts';

const id = (n: number) => n.toString(16).padStart(64, '0');
const seed = id(1);
function box(n = 2, index = 0): BoxDto {
	return {
		id: id(n),
		tx_id: seed,
		index,
		value: '9007199254740993123',
		creation_height: 5,
		ergo_tree: null,
		address: null,
		tree_hash: id(90),
		template_hash: null,
		tokens: [{ id: id(80), amount: '18446744073709551615', name: null, decimals: null }],
		registers: null,
		size: 123,
		spent_by: n === 2 ? id(4) : null,
		spent_height: n === 2 ? 11 : null,
		rent: {
			maturity_height: 100,
			due_nano: '1',
			claimable_at_tip: false,
			consensus_fee_nano: '1',
			collectible: true
		}
	};
}
function transaction(): TxDto {
	return {
		id: seed,
		block_id: id(100),
		height: 10,
		indexed_height: 20,
		confirmations: 11,
		index: 0,
		timestamp: 1_000,
		size: 300,
		fee: '1000000',
		inputs: [{ id: id(5), box: null }],
		outputs: [box(), box(3, 1)],
		data_inputs: [id(6)]
	};
}
function spender(): TxDto {
	return {
		...transaction(),
		id: id(4),
		block_id: id(101),
		height: 11,
		inputs: [{ id: id(2), box: box() }],
		outputs: [],
		data_inputs: []
	};
}
async function intercept(page: Page) {
	const requests: string[] = [];
	await page.route(/\/v1\/txs\/[a-f0-9]{64}$/, async (route) => {
		const value = route.request().url().split('/').at(-1)!;
		requests.push(`tx:${value}`);
		await route.fulfill({ json: value === id(4) ? spender() : transaction() });
	});
	await page.route(/\/v1\/boxes\/[a-f0-9]{64}$/, async (route) => {
		const value = route.request().url().split('/').at(-1)!;
		requests.push(`box:${value}`);
		await route.fulfill({ json: value === id(3) ? box(3, 1) : box() });
	});
	return requests;
}
const inspector = (page: Page) =>
	page.getByRole('complementary', { name: 'Selected node inspector' });
const expandOutput = (page: Page) =>
	inspector(page).getByRole('button', { name: 'Expand box 000000…0002', exact: true });

test('investigation entry links are local until explicit load and expand one multi-step path', async ({
	page
}) => {
	const requests = await intercept(page);
	await page.goto(`/investigate?kind=tx&id=${seed}`);
	await expect(
		inspector(page).getByRole('button', { name: 'Load selected node', exact: true })
	).toBeVisible();
	expect(requests).toEqual([]);
	await inspector(page).getByRole('button', { name: 'Load selected node', exact: true }).click();
	await expect(inspector(page)).toContainText('Some input values and scripts are unresolved');
	expect(requests).toEqual([`tx:${seed}`]);
	await expect(inspector(page).getByRole('link', { name: id(100), exact: true })).toHaveAttribute(
		'href',
		`/blocks/${id(100)}`
	);
	await expandOutput(page).click();
	await expect(inspector(page).locator('.box-amount')).toContainText('9,007,199,254.740993123');
	expect(requests).toEqual([`tx:${seed}`, `box:${id(2)}`]);
	await inspector(page)
		.getByRole('button', { name: 'Expand transaction 000000…0004', exact: true })
		.click();
	await expect(inspector(page).getByText('11', { exact: true })).toBeVisible();
	expect(requests).toEqual([`tx:${seed}`, `box:${id(2)}`, `tx:${id(4)}`]);
	await page.getByRole('button', { name: 'Evidence list', exact: true }).click();
	const nodes = page.getByRole('list', { name: 'Investigation nodes' });
	await expect(nodes.getByRole('button')).toHaveCount(3);
	await nodes.getByRole('button').nth(1).focus();
	await page.keyboard.press('Enter');
	await expect(inspector(page).locator('.entity-id')).toHaveText(id(2));
	expect(requests).toHaveLength(3);
});

test('share plans restore without fetching and exact JSON exports retain independent evidence', async ({
	page
}) => {
	const requests = await intercept(page);
	await page.goto(`/investigate?kind=tx&id=${seed}`);
	await inspector(page).getByRole('button', { name: 'Load selected node', exact: true }).click();
	await expandOutput(page).click();
	await expect(inspector(page).locator('.box-amount')).toBeVisible();
	const downloading = page.waitForEvent('download');
	await page.getByRole('button', { name: 'Export evidence JSON', exact: true }).click();
	const download = await downloading;
	const stream = await download.createReadStream();
	const chunks: Buffer[] = [];
	for await (const chunk of stream!) chunks.push(Buffer.from(chunk));
	const exported = JSON.parse(Buffer.concat(chunks).toString('utf8'));
	expect(exported.scope).toBe('independent_indexed_reads');
	expect(exported.nodes[1].data.value).toBe('9007199254740993123');
	expect(exported.nodes[0].data.inputs[0].box).toBeNull();
	expect(exported.edges[0].relation).toBe('creates');
	await page.getByRole('button', { name: 'Copy investigation link', exact: true }).click();
	const link = await page.getByRole('textbox', { name: /Shareable plan/ }).inputValue();
	expect(link).toContain('/investigate?plan=');
	const count = requests.length;
	await page.goto(link);
	await expect(page.getByText('This plan is local and unloaded.', { exact: false })).toBeVisible();
	expect(requests).toHaveLength(count);
	await expect(inspector(page)).toContainText('Inclusion pin: 10');
	await page.getByText('Connection evidence (1)', { exact: true }).click();
	await expect(page.getByText('Unverified shared plan', { exact: true })).toBeVisible();
});

test('changed inclusion clears old evidence and offers an explicit unpinned restart', async ({
	page
}) => {
	let reads = 0;
	await page.route(`**/v1/txs/${seed}`, (route) =>
		route.fulfill({ json: { ...transaction(), block_id: ++reads === 1 ? id(100) : id(102) } })
	);
	await page.goto(`/investigate?kind=tx&id=${seed}`);
	await inspector(page).getByRole('button', { name: 'Load selected node', exact: true }).click();
	await inspector(page).getByRole('button', { name: 'Refresh selected node', exact: true }).click();
	await expect(
		page.getByRole('alert').filter({ hasText: 'Conflicting indexed evidence' })
	).toContainText('does not establish a network reorganization');
	await expect(
		page.getByRole('button', { name: 'Export evidence JSON', exact: true })
	).toBeDisabled();
	await expect(inspector(page).getByText('Fee', { exact: true })).toHaveCount(0);
	await page
		.getByRole('button', { name: 'Restart from seed without old pins', exact: true })
		.click();
	await expect(
		inspector(page).getByRole('button', { name: 'Refresh selected node', exact: true })
	).toBeVisible();
	await expect(
		page.getByRole('alert').filter({ hasText: 'Conflicting indexed evidence' })
	).toHaveCount(0);
});

test('refresh outages retain stale evidence and prevent following it until retry succeeds', async ({
	page
}) => {
	let reads = 0;
	await page.route(`**/v1/txs/${seed}`, (route) =>
		++reads === 2
			? route.fulfill({ status: 503, json: { detail: 'Busy' } })
			: route.fulfill({ json: transaction() })
	);
	await page.goto(`/investigate?kind=tx&id=${seed}`);
	await inspector(page).getByRole('button', { name: 'Load selected node', exact: true }).click();
	await inspector(page).getByRole('button', { name: 'Refresh selected node', exact: true }).click();
	await expect(inspector(page).getByRole('alert')).toContainText('Stale evidence');
	await expect(expandOutput(page)).toBeDisabled();
	await expect(inspector(page).getByText('Fee', { exact: true })).toBeVisible();
	await inspector(page).getByRole('button', { name: 'Refresh selected node', exact: true }).click();
	await expect(expandOutput(page)).toBeEnabled();
});

test('invalid share plans fail locally and a fresh seed can recover', async ({ page }) => {
	const requests = await intercept(page);
	await page.goto('/investigate?plan=invalid');
	await expect(page.getByRole('alert')).toContainText('invalid or exceeds');
	expect(requests).toHaveLength(0);
	await page.getByLabel('Transaction or box ID', { exact: true }).fill(seed);
	await page.getByRole('button', { name: 'Load seed', exact: true }).click();
	await expect(
		inspector(page).getByRole('button', { name: 'Refresh selected node', exact: true })
	).toBeVisible();
	await expect(page.getByRole('alert')).toHaveCount(0);
	expect(requests).toEqual([`tx:${seed}`]);
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora'])
	for (const mode of ['light', 'dark'])
		test(`${appearance} ${mode} investigation graph and keyboard list fit 320px`, async ({
			page
		}) => {
			await page.setViewportSize({ width: 320, height: 900 });
			await page.addInitScript(
				({ appearance, mode }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', mode);
				},
				{ appearance, mode }
			);
			await intercept(page);
			await page.goto(`/investigate?kind=tx&id=${seed}`);
			await inspector(page)
				.getByRole('button', { name: 'Load selected node', exact: true })
				.click();
			await expandOutput(page).click();
			await expect(inspector(page).locator('.box-amount')).toBeVisible();
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
			).toBe(true);
			const canvas = page.getByRole('region', {
				name: 'Investigation graph; scroll to explore',
				exact: true
			});
			await canvas.focus();
			await expect(canvas).toBeFocused();
			await page.getByRole('button', { name: 'Evidence list', exact: true }).click();
			const nodes = page.getByRole('list', { name: 'Investigation nodes' });
			await nodes.getByRole('button').first().focus();
			await page.keyboard.press('Enter');
			await expect(inspector(page).locator('.entity-id')).toHaveText(seed);
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
			).toBe(true);
		});
