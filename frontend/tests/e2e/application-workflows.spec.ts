import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { BoxDto, TxDto } from '../../src/lib/api/types.ts';

const raw = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/apps/spectrum-v3-swap.json', import.meta.url),
		'utf8'
	)
) as { order: BoxDto; settlement: TxDto };
// Unrelated size/rent display fields are synthetic; every workflow fact is captured.
const displayBox = (box: BoxDto): BoxDto => ({
	...box,
	size: 0,
	kind: 'box',
	rent: {
		maturity_height: box.creation_height + 1_051_200,
		due_nano: '0',
		claimable_at_tip: false,
		consensus_fee_nano: '0',
		collectible: false
	}
});
const order = displayBox(raw.order);
const settlement: TxDto = {
	...raw.settlement,
	inputs: raw.settlement.inputs.map((input) => ({ ...input, box: displayBox(input.box!) })),
	outputs: raw.settlement.outputs.map(displayBox)
};
async function setup(page: Page, box = order, tx = settlement) {
	const requests: string[] = [];
	const spendId = box.spent_by ?? tx.id;
	await page.route('**/v1/boxes/' + box.id, (route) => {
		requests.push('order');
		return route.fulfill({ json: box });
	});
	await page.route('**/v1/txs/' + spendId, (route) => {
		requests.push('spend');
		return route.fulfill({ json: tx });
	});
	await page.route('**/v1/txs/' + spendId + '/status', (route) =>
		route.fulfill({
			json: {
				id: spendId,
				state: 'confirmed',
				checked_at_ms: 1,
				indexed_height: tx.height,
				inclusion: { block_id: tx.block_id, height: tx.height, confirmations: 1 },
				previous_inclusion: null,
				mempool: {
					observation: 'not_checked',
					checked_at_ms: null,
					first_seen_at_ms: null,
					last_seen_at_ms: null,
					error: null
				},
				pending: null,
				conflicts: [],
				history_scope: 'process_local_requested_transactions',
				retention_seconds: 3600
			}
		})
	);
	return requests;
}

const additions = ['swap-buy', 'deposit', 'redeem'].map((name) => {
	const captured = JSON.parse(
		readFileSync(
			new URL('../../../tests/fixtures/apps/spectrum-v3-' + name + '.json', import.meta.url),
			'utf8'
		)
	) as { order: BoxDto; settlement: TxDto };
	return {
		name,
		order: displayBox(captured.order),
		settlement: {
			...captured.settlement,
			inputs: captured.settlement.inputs.map((input) => ({
				...input,
				box: displayBox(input.box!)
			})),
			outputs: captured.settlement.outputs.map(displayBox)
		},
		region: name === 'swap-buy' ? 'Spectrum swap workflow' : 'Spectrum liquidity workflow',
		stage:
			name === 'swap-buy'
				? 'Matching swap observed'
				: name === 'deposit'
					? 'Matching deposit observed'
					: 'Matching redemption observed'
	};
});

for (const recorded of additions) {
	test(
		'captured ' + recorded.name + ' exposes exact request and result without preloading',
		async ({ page }) => {
			const requests = await setup(page, recorded.order, recorded.settlement);
			await page.goto('/box/' + recorded.order.id);
			const workflow = page.getByRole('region', { name: recorded.region, exact: true });
			await expect(workflow.getByRole('button', { name: 'Inspect workflow' })).toBeVisible();
			expect(requests.filter((request) => request === 'spend')).toHaveLength(0);
			await workflow.getByRole('button', { name: 'Inspect workflow' }).click();
			await expect(workflow.getByText(recorded.stage, { exact: true })).toBeVisible();
			await expect(workflow.getByRole('link', { name: 'Pool before' })).toHaveAttribute(
				'href',
				'/box/' + recorded.settlement.inputs[0].id
			);
			await expect(workflow.getByRole('link', { name: 'Recipient output' })).toHaveAttribute(
				'href',
				'/box/' + recorded.settlement.outputs[1].id
			);
			if (recorded.name === 'swap-buy') {
				await expect(workflow.getByText('0.010457609', { exact: true })).toBeVisible();
				await expect(workflow.getByText('0.010147609 ERG', { exact: true })).toBeVisible();
				await expect(workflow).toContainText('returned order-box funding');
			} else if (recorded.name === 'deposit') {
				await expect(workflow.getByText('4,206', { exact: true })).toBeVisible();
				await expect(workflow.getByText('Minimum LP return', { exact: true })).toBeVisible();
			} else {
				await expect(workflow.getByText('0.03365495', { exact: true })).toBeVisible();
				await expect(workflow.getByText('0.03334495 ERG', { exact: true })).toBeVisible();
				await expect(workflow.getByText('5', { exact: true })).toBeVisible();
			}
		}
	);
}

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	test(
		'liquidity returns remain keyboard accessible at 320px in ' + appearance,
		async ({ page }) => {
			await page.addInitScript(
				(appearance) => localStorage.setItem('xp-appearance', appearance),
				appearance
			);
			await page.setViewportSize({ width: 320, height: 900 });
			const recorded = additions[2];
			await setup(page, recorded.order, recorded.settlement);
			await page.goto('/box/' + recorded.order.id);
			const workflow = page.getByRole('region', { name: recorded.region, exact: true });
			await workflow.getByRole('button', { name: 'Inspect workflow' }).focus();
			await page.keyboard.press('Enter');
			await expect(workflow.getByText(recorded.stage, { exact: true })).toBeVisible();
			await expect(workflow.getByRole('link', { name: 'Spending transaction' })).toBeVisible();
			await expect(workflow).toContainText('not an address’s net gain');
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
		}
	);
}

test('a captured Spectrum order is explicit, exact and links the observed workflow evidence', async ({
	page
}) => {
	const requests = await setup(page);
	await page.goto('/box/' + order.id);
	const workflow = page.getByRole('region', { name: 'Spectrum swap workflow', exact: true });
	await expect(workflow.getByRole('button', { name: 'Inspect workflow' })).toBeVisible();
	expect(requests.filter((value) => value === 'spend')).toHaveLength(0);
	const before = requests.length;
	await workflow.getByRole('button', { name: 'Inspect workflow' }).click();
	await expect(workflow.getByText('Matching swap observed', { exact: true })).toBeVisible();
	expect(requests.slice(before)).toEqual(['order', 'spend']);
	await expect(workflow.getByText('5,429,060', { exact: true })).toBeVisible();
	await expect(workflow.getByText('5,405,915 raw units', { exact: true })).toBeVisible();
	await expect(workflow.getByRole('link', { name: 'Spending transaction' })).toHaveAttribute(
		'href',
		'/tx/' + settlement.id
	);
	await expect(workflow.getByRole('link', { name: 'Recipient output' })).toHaveAttribute(
		'href',
		'/box/' + settlement.outputs[1].id
	);
	await expect(workflow).toContainText('not an address’s net gain');
});

test('a transaction receipt discovers supported order inputs without automatically following them', async ({
	page
}) => {
	const requests = await setup(page);
	await page.goto('/tx/' + settlement.id);
	const workflow = page.getByRole('region', { name: 'Spectrum swap workflow', exact: true });
	await expect(workflow.getByRole('button', { name: 'Inspect workflow' })).toBeVisible();
	expect(requests.filter((value) => value === 'order')).toHaveLength(0);
	await workflow.getByRole('button', { name: 'Inspect workflow' }).click();
	await expect(workflow.getByText('Matching swap observed', { exact: true })).toBeVisible();
});

test('unspent is an indexed observation and does not claim pending execution', async ({ page }) => {
	const requests = await setup(page, { ...order, spent_by: null, spent_height: null });
	await page.goto('/box/' + order.id);
	const workflow = page.getByRole('region', { name: 'Spectrum swap workflow', exact: true });
	await workflow.getByRole('button', { name: 'Inspect workflow' }).click();
	await expect(workflow.getByRole('heading', { name: 'No indexed spend observed' })).toBeVisible();
	expect(requests.filter((value) => value === 'spend')).toHaveLength(0);
	await expect(workflow).toContainText('does not check the mempool');
});

test('a nonmatching spend remains unrecognized instead of becoming a fill or cancellation', async ({
	page
}) => {
	const tx = structuredClone(settlement);
	tx.outputs[1].ergo_tree = '00';
	await setup(page, order, tx);
	await page.goto('/box/' + order.id);
	const workflow = page.getByRole('region', { name: 'Spectrum swap workflow', exact: true });
	await workflow.getByRole('button', { name: 'Inspect workflow' }).click();
	await expect(
		workflow.getByRole('heading', { name: 'Spent, with an unrecognized outcome' })
	).toBeVisible();
	await expect(workflow).toContainText('does not establish a fill or cancellation');
	await expect(workflow.getByText('5,429,060', { exact: true })).toHaveCount(0);
});

test('a detail budget error clears an old result and offers only deliberate retry', async ({
	page
}) => {
	const requests = await setup(page);
	await page.goto('/box/' + order.id);
	const workflow = page.getByRole('region', { name: 'Spectrum swap workflow', exact: true });
	await workflow.getByRole('button', { name: 'Inspect workflow' }).click();
	await expect(workflow.getByText('Matching swap observed', { exact: true })).toBeVisible();
	await page.route('**/v1/txs/' + settlement.id, (route) =>
		route.fulfill({
			status: 422,
			json: {
				title: 'Too large',
				detail: 'Transaction detail budget exceeded',
				code: 'detail_budget'
			}
		})
	);
	await workflow.getByRole('button', { name: 'Refresh workflow' }).click();
	await expect(workflow.getByRole('alert')).toContainText('detail budget exceeded');
	await expect(workflow.getByText('Matching swap observed', { exact: true })).toHaveCount(0);
	await expect(workflow.getByText('5,429,060', { exact: true })).toHaveCount(0);
	expect(requests.filter((value) => value === 'spend')).toHaveLength(1);
});

test('unknown contract versions do not acquire an application identity from their token names', async ({
	page
}) => {
	await setup(page, { ...order, ergo_tree: '0008cd02' + 'b'.repeat(64) });
	await page.goto('/box/' + order.id);
	await expect(page.getByRole('heading', { name: 'Box', exact: true })).toBeVisible();
	await expect(
		page.getByRole('region', { name: 'Spectrum swap workflow', exact: true })
	).toHaveCount(0);
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const theme of ['light', 'dark']) {
		test('Spectrum workflow remains readable in ' + appearance + ' ' + theme, async ({ page }) => {
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await setup(page);
			await page.goto('/box/' + order.id);
			const workflow = page.getByRole('region', { name: 'Spectrum swap workflow', exact: true });
			const button = workflow.getByRole('button', { name: 'Inspect workflow' });
			await button.focus();
			await page.keyboard.press('Enter');
			await expect(workflow.getByText('Matching swap observed', { exact: true })).toBeVisible();
			for (const width of [320, 1024, 1440]) {
				await page.setViewportSize({ width, height: 900 });
				await page.evaluate(() => document.fonts.ready);
				expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
				const bounds = await workflow.boundingBox();
				expect(bounds!.x).toBeGreaterThanOrEqual(0);
				expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
				await expect(workflow.getByRole('button', { name: 'Refresh workflow' })).toBeVisible();
			}
		});
	}
}
