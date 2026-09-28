import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { TxDto, TxEvidence, TxStatusDto } from '../../src/lib/api/types.ts';

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

async function openReceipt(page: Page) {
	await page.goto(`/tx/${tx.id}`);
	await page.getByRole('tab', { name: 'Receipt', exact: true }).click();
}

function expandedFlowTransaction(): TxDto {
	const expanded = structuredClone(tx);
	// Four extra input/change pairs keep the original address deltas and fee unchanged.
	for (let index = 0; index < 4; index++) {
		const input = structuredClone(tx.inputs[1]);
		input.id = (0x100 + index).toString(16).padStart(64, '0');
		input.box!.id = input.id;
		expanded.inputs.push(input);
		const output = structuredClone(tx.outputs[0]);
		output.id = (0x200 + index).toString(16).padStart(64, '0');
		output.index = index + 2;
		output.value = input.box!.value;
		output.tokens = [];
		expanded.outputs.push(output);
	}
	return expanded;
}

test.beforeEach(async ({ page }) => {
	await page.route(`**/v1/txs/${tx.id}`, (route) => route.fulfill({ json: tx }));
	await page.route(`**/v1/txs/${tx.id}/evidence`, (route) => route.fulfill({ json: evidence }));
});

test('Prism opens spatial flow while an explicitly chosen view survives a design switch', async ({
	page
}) => {
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByRole('tab', { name: 'Box flow', exact: true })).toHaveAttribute(
		'aria-selected',
		'true'
	);
	await expect(
		page.getByRole('region', { name: 'Transaction box flow', exact: true })
	).toBeVisible();
	await page.getByRole('button', { name: 'Appearance', exact: true }).click();
	const picker = page.getByRole('dialog', { name: 'Choose appearance', exact: true });
	await picker.getByRole('radio', { name: 'Atelier', exact: true }).check();
	await picker.getByRole('button', { name: 'Close', exact: true }).click();
	await expect(page.getByRole('tab', { name: 'Receipt', exact: true })).toHaveAttribute(
		'aria-selected',
		'true'
	);
	await page.getByRole('tab', { name: 'Evidence', exact: true }).click();
	await page.getByRole('button', { name: 'Appearance', exact: true }).click();
	await picker.getByRole('radio', { name: 'Prism', exact: true }).check();
	await picker.getByRole('button', { name: 'Close', exact: true }).click();
	await expect(page.getByRole('tab', { name: 'Evidence', exact: true })).toHaveAttribute(
		'aria-selected',
		'true'
	);
	await expect(
		page.getByRole('heading', { name: 'How this receipt was determined' })
	).toBeVisible();
});

test('rent receipt explains exact effects from either address perspective', async ({ page }) => {
	await openReceipt(page);
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

test('a confirmed receipt leads with its net amount and keeps observation detail expandable', async ({
	page
}) => {
	await page.setViewportSize({ width: 1265, height: 714 });
	const status: TxStatusDto = {
		id: tx.id,
		state: 'confirmed',
		checked_at_ms: Date.now(),
		indexed_height: tx.indexed_height!,
		inclusion: { block_id: tx.block_id!, height: tx.height, confirmations: tx.confirmations! },
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
	};
	await page.route(`**/v1/txs/${tx.id}/status`, (route) => route.fulfill({ json: status }));
	await openReceipt(page);
	const receipt = page.getByRole('region', { name: 'Transaction receipt' });
	await expect(receipt.getByRole('heading', { name: 'Storage rent claim' })).toBeVisible();
	const amount = receipt.getByRole('tabpanel').getByText('+2,000', { exact: true });
	const bounds = await amount.boundingBox();
	expect(bounds!.y + bounds!.height).toBeLessThan(714);
	const live = page.getByRole('region', { name: 'Live transaction status' });
	expect((await live.boundingBox())!.height).toBeLessThan(85);
	await live.getByText('Observation details', { exact: true }).click();
	await expect(live.getByText('Checks every 5 seconds', { exact: false })).toBeVisible();
	await expect(live.getByText('Absence from one node’s mempool', { exact: false })).toBeVisible();
});

test('source failure preserves exact balances without inventing a claim label', async ({
	page
}) => {
	await page.route(`**/v1/txs/${tx.id}/evidence`, (route) =>
		route.fulfill({ status: 503, json: { detail: 'Unavailable' } })
	);
	await openReceipt(page);
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
	await openReceipt(page);
	await expect(page.getByRole('heading', { name: 'Balance changes unavailable' })).toBeVisible();
	await expect(page.getByRole('combobox')).toHaveCount(0);
});

test('mobile receipt and evidence stay within the page', async ({ page }) => {
	await page.setViewportSize({ width: 320, height: 900 });
	await openReceipt(page);
	await expect(page.getByRole('heading', { name: 'Storage rent claim' })).toBeVisible();
	for (const tab of ['Receipt', 'Box flow', 'Balance changes', 'Evidence']) {
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
	await openReceipt(page);
	await expect(page.getByRole('heading', { name: 'Balance changes unavailable' })).toBeVisible();
	current = tx;
	await page.getByRole('button', { name: 'Refresh', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'Storage rent claim' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Balance changes unavailable' })).toHaveCount(0);
});

test('selecting an input in Box flow changes the existing address receipt exactly', async ({
	page
}) => {
	await openReceipt(page);
	const receipt = page.getByRole('region', { name: 'Transaction receipt' });
	const summary = receipt.getByRole('tabpanel', { name: 'Receipt', exact: true });
	const flow = receipt.getByRole('tabpanel', { name: 'Box flow', exact: true });
	const inspector = flow.getByRole('region', { name: 'Selected address balance changes' });
	await expect(summary.getByText('+2,000', { exact: true })).toBeVisible();
	await receipt.getByRole('tab', { name: 'Box flow', exact: true }).click();
	await flow.getByRole('button', { name: 'Select input 1 address', exact: true }).click();
	await expect(
		flow.getByRole('button', { name: 'Select input 1 address', exact: true })
	).toHaveAttribute('aria-pressed', 'true');
	await expect(inspector.getByText('-2,000', { exact: true })).toBeVisible();
	await expect(inspector.getByText('-0.0001108', { exact: true })).toBeVisible();
	await receipt.getByRole('tab', { name: 'Receipt', exact: true }).click();
	await expect(summary.getByRole('combobox')).toHaveValue(tx.inputs[0].box!.tree_hash);
	await expect(summary.getByText('-2,000', { exact: true })).toBeVisible();
	await expect(summary.getByText('-0.0001108 ERG', { exact: true })).toBeVisible();
	await receipt.getByRole('tab', { name: 'Box flow', exact: true }).click();
	await flow.getByRole('button', { name: 'Select input 2 address', exact: true }).click();
	await expect(inspector.getByText('+2,000', { exact: true })).toBeVisible();
	await expect(inspector.getByText('-0.0009892', { exact: true })).toBeVisible();
	await receipt.getByRole('tab', { name: 'Receipt', exact: true }).click();
	await expect(summary.getByRole('combobox')).toHaveValue(tx.inputs[1].box!.tree_hash);
	await expect(summary.getByText('+2,000', { exact: true })).toBeVisible();
	await expect(summary.getByText('-0.0009892 ERG', { exact: true })).toBeVisible();
});

test('Box flow keeps known box values separate from unavailable net changes', async ({ page }) => {
	const partial = structuredClone(tx);
	partial.inputs[0].box = null;
	await page.route(`**/v1/txs/${tx.id}`, (route) => route.fulfill({ json: partial }));
	await openReceipt(page);
	const receipt = page.getByRole('region', { name: 'Transaction receipt' });
	await receipt.getByRole('tab', { name: 'Box flow', exact: true }).click();
	const flow = receipt.getByRole('tabpanel', { name: 'Box flow', exact: true });
	const unresolved = flow.getByRole('article', { name: 'Input 1 box', exact: true });
	await expect(unresolved.getByText('Value and tokens unknown', { exact: true })).toBeVisible();
	await expect(unresolved.getByRole('button')).toHaveCount(0);
	await expect(unresolved.getByRole('link')).toHaveAttribute('href', `/box/${tx.inputs[0].id}`);
	await expect(
		flow
			.getByRole('article', { name: 'Input 2 box', exact: true })
			.getByText('1 ERG', { exact: true })
	).toBeVisible();
	await flow.getByRole('button', { name: 'Select input 2 address', exact: true }).click();
	const inspector = flow.getByRole('region', { name: 'Selected address balance changes' });
	await expect(
		inspector.getByRole('heading', { name: 'Exact net changes unavailable', exact: true })
	).toBeVisible();
	await expect(
		inspector.getByText(
			'Every input must be resolved before any address net change can be shown.',
			{
				exact: false
			}
		)
	).toBeVisible();
	await expect(inspector.getByText('+2,000', { exact: true })).toHaveCount(0);
	await expect(inspector.getByText('-0.0009892', { exact: true })).toHaveCount(0);
	await receipt.getByRole('tab', { name: 'Receipt', exact: true }).click();
	await expect(receipt.getByRole('heading', { name: 'Balance changes unavailable' })).toBeVisible();
	await expect(receipt.getByRole('combobox')).toHaveCount(0);
});

test('Box flow expands both sides independently and fits a 320px screen', async ({ page }) => {
	const expanded = expandedFlowTransaction();
	await page.setViewportSize({ width: 320, height: 900 });
	await page.route(`**/v1/txs/${tx.id}`, (route) => route.fulfill({ json: expanded }));
	await page.route(`**/v1/txs/${tx.id}/evidence`, (route) =>
		route.fulfill({
			json: {
				...evidence,
				inputs: expanded.inputs.map((input, index) => ({
					...evidence.inputs[index === 0 ? 0 : 1],
					id: input.id
				}))
			}
		})
	);
	await openReceipt(page);
	const receipt = page.getByRole('region', { name: 'Transaction receipt' });
	await receipt.getByRole('tab', { name: 'Box flow', exact: true }).click();
	const flow = receipt.getByRole('tabpanel', { name: 'Box flow', exact: true });
	const inputs = flow.getByRole('region', { name: 'Inputs', exact: true }).getByRole('article');
	const outputs = flow.getByRole('region', { name: 'Outputs', exact: true }).getByRole('article');
	await expect(inputs).toHaveCount(4);
	await expect(outputs).toHaveCount(4);
	await flow.getByRole('button', { name: 'Show more inputs', exact: true }).click();
	await expect(inputs).toHaveCount(6);
	await expect(outputs).toHaveCount(4);
	await flow.getByRole('button', { name: 'Show more outputs', exact: true }).click();
	await expect(outputs).toHaveCount(6);
	await expect(
		flow.getByRole('button', { name: 'Select input 6 address', exact: true })
	).toBeVisible();
	await expect(
		flow.getByRole('button', { name: 'Select output 6 address', exact: true })
	).toBeVisible();
	expect(await flow.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
	expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
		true
	);
	await flow.getByRole('button', { name: 'Show fewer inputs', exact: true }).click();
	await expect(inputs).toHaveCount(4);
	await expect(outputs).toHaveCount(6);
	await flow.getByRole('button', { name: 'Show fewer outputs', exact: true }).click();
	await expect(outputs).toHaveCount(4);
});
