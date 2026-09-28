import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { BoxDto, TxDto } from '../../src/lib/api/types.ts';

const producer: TxDto = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/receipts/storage-rent-tx.json', import.meta.url),
		'utf8'
	)
);
const spendingId = 'c'.repeat(64);
const focused: BoxDto = {
	...producer.outputs[0],
	creation_height: 1,
	spent_by: spendingId,
	spent_height: producer.height + 1
};
const following: BoxDto = {
	...focused,
	id: '1'.repeat(64),
	tx_id: spendingId,
	index: 0,
	spent_by: null,
	spent_height: null
};
const spender: TxDto = {
	...producer,
	id: spendingId,
	height: producer.height + 1,
	indexed_height: producer.height + 8,
	inputs: [{ id: focused.id, box: focused }],
	outputs: [following]
};

async function setup(page: Page, box = focused, before = producer, after = spender) {
	const requests: string[] = [];
	await page.route(`**/v1/boxes/${box.id}`, (route) => route.fulfill({ json: box }));
	await page.route(`**/v1/boxes/${following.id}`, (route) => route.fulfill({ json: following }));
	await page.route(`**/v1/txs/${before.id}`, (route) => {
		requests.push('producer');
		return route.fulfill({ json: before });
	});
	await page.route(`**/v1/txs/${after.id}`, (route) => {
		requests.push('spender');
		return route.fulfill({ json: after });
	});
	return requests;
}

test('lineage loads only requested neighbours, validates inclusion, and provides adjacent box links', async ({
	page
}) => {
	const requests = await setup(page);
	await page.goto(`/box/${focused.id}`);
	const section = page.getByRole('region', { name: 'Box lineage', exact: true });
	await expect(
		section.getByRole('heading', { name: 'One box. Its immediate history.' })
	).toBeVisible();
	expect(requests).toEqual([]);
	const producing = section.getByRole('article', { name: 'Producing transaction lineage' });
	await producing.getByRole('button', { name: 'Inspect producing transaction' }).focus();
	await page.keyboard.press('Enter');
	await expect(
		producing.getByText(`Included in block ${producer.height.toLocaleString('en-US')}.`, {
			exact: true
		})
	).toBeVisible();
	await expect(producing.getByRole('list', { name: 'Previous boxes' })).toBeVisible();
	await expect(producing.getByRole('list', { name: 'Sibling outputs' })).toBeVisible();
	expect(requests).toEqual(['producer']);
	const spending = section.getByRole('article', { name: 'Spending transaction lineage' });
	await spending.getByRole('button', { name: 'Inspect spending transaction' }).click();
	await expect(spending.getByRole('list', { name: 'Following outputs' })).toBeVisible();
	expect(requests).toEqual(['producer', 'spender']);
	const next = spending.getByRole('link', { name: /open box/ }).first();
	await expect(next).toHaveAttribute('href', `/box/${following.id}`);
	await next.click();
	await expect(page).toHaveURL(`/box/${following.id}`);
	await expect(
		page
			.getByRole('region', { name: 'Box lineage' })
			.getByText('Unspent in this box record', { exact: true })
	).toBeVisible();
	await expect(page.getByRole('list', { name: 'Following outputs' })).toHaveCount(0);
});

test('lineage caps visible references and keeps unresolved boxes explicit', async ({ page }) => {
	const large: TxDto = {
		...producer,
		inputs: Array.from({ length: 25 }, (_, index) => ({
			id: index.toString(16).padStart(64, '0'),
			box: null
		}))
	};
	await setup(page, focused, large);
	await page.goto(`/box/${focused.id}`);
	const branch = page.getByRole('article', { name: 'Producing transaction lineage' });
	await branch.getByRole('button', { name: 'Inspect producing transaction' }).click();
	await expect(
		branch.getByText('25 unresolved inputs. No complete input total is claimed.')
	).toBeVisible();
	const list = branch.getByRole('list', { name: 'Previous boxes' });
	await expect(list.locator('li')).toHaveCount(4);
	await branch.getByRole('button', { name: 'Show more previous boxes' }).click();
	await expect(list.locator('li')).toHaveCount(20);
	await expect(branch.getByText('Showing 20 of 25 boxes.', { exact: false })).toBeVisible();
	await expect(list.getByText('Input value unavailable', { exact: true })).toHaveCount(20);
	await branch.getByRole('button', { name: 'Show fewer previous boxes' }).click();
	await expect(list.locator('li')).toHaveCount(4);
});

test('budget and mismatched membership never produce invented lineage edges', async ({ page }) => {
	await setup(page);
	await page.route(`**/v1/txs/${producer.id}`, (route) =>
		route.fulfill({ status: 422, json: { title: 'Budget', detail: 'too large' } })
	);
	await page.goto(`/box/${focused.id}`);
	const branch = page.getByRole('article', { name: 'Producing transaction lineage' });
	await branch.getByRole('button', { name: 'Inspect producing transaction' }).click();
	await expect(
		branch.getByText('This transaction exceeds the available detail budget.', { exact: false })
	).toBeVisible();
	await expect(branch.getByRole('list')).toHaveCount(0);
	await page.route(`**/v1/txs/${producer.id}`, (route) =>
		route.fulfill({ json: { ...producer, outputs: [] } })
	);
	await branch.getByRole('button', { name: 'Retry producing transaction' }).click();
	await expect(
		branch.getByText('This transaction does not contain the selected box', { exact: false })
	).toBeVisible();
	await expect(branch.getByRole('list')).toHaveCount(0);
});

test('genesis and unspent terminal states do not request a transaction', async ({ page }) => {
	const requests = await setup(page, {
		...focused,
		tx_id: '0'.repeat(64),
		spent_by: null,
		spent_height: null
	});
	await page.goto(`/box/${focused.id}`);
	const section = page.getByRole('region', { name: 'Box lineage' });
	await expect(section.getByText('Genesis allocation', { exact: true })).toBeVisible();
	await expect(section.getByText('Unspent in this box record', { exact: true })).toBeVisible();
	await expect(section.getByRole('button', { name: /Inspect .* transaction/ })).toHaveCount(0);
	expect(requests).toEqual([]);
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	test(`lineage remains keyboard accessible without overflow at 320px in ${appearance}`, async ({
		page
	}) => {
		await page.addInitScript((value) => {
			localStorage.setItem('xp-appearance', value);
		}, appearance);
		await page.setViewportSize({ width: 320, height: 820 });
		await setup(page);
		await page.goto(`/box/${focused.id}`);
		const section = page.getByRole('region', { name: 'Box lineage' });
		const button = section.getByRole('button', { name: 'Inspect producing transaction' });
		await button.focus();
		await page.keyboard.press('Enter');
		await expect(section.getByRole('list', { name: 'Previous boxes' })).toBeVisible();
		expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
			true
		);
	});
}
