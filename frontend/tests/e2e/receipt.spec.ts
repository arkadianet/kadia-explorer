import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { TxDto, TxEvidence } from '../../src/lib/api/types.ts';

const tx: TxDto = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/receipts/storage-rent-tx.json', import.meta.url),
		'utf8'
	)
);
const block = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/receipts/storage-rent-block.json', import.meta.url),
		'utf8'
	)
);
const evidence: TxEvidence = {
	tx_id: tx.id,
	block_id: tx.block_id!,
	height: tx.height,
	assurance: 'trusted_node_response',
	inputs: block.blockTransactions.transactions[tx.index].inputs.map(
		(i: {
			boxId: string;
			spendingProof: { proofBytes: string; extension: Record<string, string> };
		}) => ({
			id: i.boxId,
			proof: i.spendingProof.proofBytes === '' ? 'empty' : 'nonempty',
			extension_127: i.spendingProof.extension['127'] ?? null
		})
	)
};

test.beforeEach(async ({ page }) => {
	await page.route(`**/v1/txs/${tx.id}`, (route) => route.fulfill({ json: tx }));
	await page.route(`**/v1/txs/${tx.id}/evidence`, (route) => route.fulfill({ json: evidence }));
});

test('rent receipt explains exact effects from either address perspective', async ({ page }) => {
	await page.goto(`/tx/${tx.id}`);
	const receipt = page.getByRole('region', { name: 'Transaction receipt' });
	await expect(receipt.getByRole('heading', { name: 'Storage rent claim' })).toBeVisible();
	await expect(receipt.getByText('8 confirmations at last check')).toBeVisible();
	await expect(receipt.getByRole('tabpanel').getByText('+2,000', { exact: true })).toBeVisible();
	await expect(
		receipt
			.getByRole('tabpanel', { name: 'Receipt', exact: true })
			.getByText('-0.0009892 ERG', { exact: true })
	).toBeVisible();
	await receipt.getByRole('combobox').selectOption(tx.inputs[0].box!.tree_hash);
	await expect(receipt.getByText('-2,000', { exact: true })).toBeVisible();
	await expect(
		receipt
			.getByRole('tabpanel', { name: 'Receipt', exact: true })
			.getByText('-0.0001108 ERG', { exact: true })
	).toBeVisible();
	await expect(receipt.getByText('Tokens sent', { exact: true })).toHaveCount(0);
	await receipt.getByRole('tab', { name: 'Balance changes' }).click();
	await expect(receipt.getByText('All ERG changes sum to zero.', { exact: false })).toBeVisible();
	await receipt.getByRole('tab', { name: 'Evidence', exact: true }).click();
	await expect(receipt.getByText('Empty proof · rent selector: 0300')).toBeVisible();
});

test('source failure preserves exact balances without inventing a claim label', async ({
	page
}) => {
	await page.route(`**/v1/txs/${tx.id}/evidence`, (route) =>
		route.fulfill({ status: 503, json: { detail: 'Unavailable' } })
	);
	await page.goto(`/tx/${tx.id}`);
	const receipt = page.getByRole('region', { name: 'Transaction receipt' });
	await expect(
		receipt.getByText('Spending evidence is unavailable.', { exact: false })
	).toBeVisible();
	await expect(
		receipt
			.getByRole('tabpanel', { name: 'Receipt', exact: true })
			.getByText('-0.0009892 ERG', { exact: true })
	).toBeVisible();
	await expect(receipt.getByRole('heading', { name: 'Storage rent claim' })).toHaveCount(0);
});

test('missing historical inputs never turn output values into net receipts', async ({ page }) => {
	const partial = structuredClone(tx);
	partial.inputs[0].box = null;
	await page.route(`**/v1/txs/${tx.id}`, (route) => route.fulfill({ json: partial }));
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Balance changes unavailable' })).toBeVisible();
	await expect(page.getByRole('combobox')).toHaveCount(0);
});

test('mobile receipt and evidence stay within the page', async ({ page }) => {
	await page.setViewportSize({ width: 320, height: 900 });
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Storage rent claim' })).toBeVisible();
	for (const tab of ['Receipt', 'Balance changes', 'Evidence']) {
		await page.getByRole('tab', { name: tab, exact: true }).click();
		expect(
			await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
		).toBe(true);
	}
});

test('manual refresh recognizes newly resolved historical inputs', async ({ page }) => {
	let current = structuredClone(tx);
	current.inputs[0].box = null;
	await page.route(`**/v1/txs/${tx.id}`, (route) => route.fulfill({ json: current }));
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Balance changes unavailable' })).toBeVisible();
	current = tx;
	await page.getByRole('button', { name: 'Refresh', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'Storage rent claim' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Balance changes unavailable' })).toHaveCount(0);
});
