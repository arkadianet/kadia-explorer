import { expect, test, type Page } from '@playwright/test';
import { tx as fixture } from './data.ts';
import type { TxStatusDto } from '../../src/lib/api/types.ts';

const tx = {
	...fixture,
	block_id: 'b'.repeat(64),
	indexed_height: fixture.height + 2,
	confirmations: 3
};
function status(state: TxStatusDto['state'] = 'pending'): TxStatusDto {
	return {
		id: tx.id,
		state,
		checked_at_ms: Date.now(),
		indexed_height: tx.height + 2,
		inclusion:
			state === 'confirmed' ? { block_id: tx.block_id, height: tx.height, confirmations: 3 } : null,
		previous_inclusion: null,
		mempool: {
			observation: state === 'pending' ? 'present' : 'absent',
			checked_at_ms: Date.now(),
			first_seen_at_ms: Date.now() - 1000,
			last_seen_at_ms: Date.now(),
			error: null
		},
		pending:
			state === 'pending'
				? { input_count: 2, output_count: 2, data_input_count: 0, size: 512, fee: '1100000' }
				: null,
		conflicts: [],
		history_scope: 'process_local_requested_transactions',
		retention_seconds: 3600
	};
}
async function mock(page: Page, first: TxStatusDto = status()) {
	const current = { value: first, fail: false, requests: 0, details: 0, detailValue: tx };
	await page.route(`**/v1/txs/${tx.id}`, (route) => {
		current.details++;
		return current.value.state === 'confirmed'
			? route.fulfill({ json: current.detailValue })
			: route.fulfill({ status: 404, json: { detail: 'Not indexed' } });
	});
	await page.route(`**/v1/txs/${tx.id}/status`, (route) => {
		current.requests++;
		return current.fail
			? route.fulfill({ status: 503, json: { detail: 'Unavailable' } })
			: route.fulfill({ json: current.value });
	});
	return current;
}

test('pending becomes a receipt, then confirmations increase without reloading boxes', async ({
	page
}) => {
	await page.clock.install();
	const current = await mock(page);
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Waiting for a block' })).toBeVisible();
	await expect(page.getByText('0.0011 ERG', { exact: true })).toBeVisible();
	await expect(
		page.getByRole('link', { name: 'Inspect connections in a mempool snapshot' })
	).toHaveAttribute('href', '/mempool?focus=' + tx.id);
	await expect(page.getByText('Read-only data inputs', { exact: true })).toBeVisible();
	await expect(page.getByText('512 bytes', { exact: true })).toBeVisible();
	await expect(page.getByRole('region', { name: 'Transaction receipt' })).toHaveCount(0);
	current.value = status('confirmed');
	await page.clock.runFor(5500);
	await expect(page.getByText('3 confirmations at last check')).toBeVisible();
	const details = current.details;
	current.value.indexed_height!++;
	current.value.inclusion!.confirmations++;
	await page.clock.runFor(5500);
	await expect(page.getByText('4 confirmations at last check')).toBeVisible();
	expect(current.details).toBe(details);
	await expect(
		page.getByText(
			`Box details were checked at indexed block ${tx.indexed_height.toLocaleString('en-US')}.`,
			{ exact: false }
		)
	).toBeVisible();
	current.detailValue = {
		...tx,
		indexed_height: current.value.indexed_height!,
		outputs: tx.outputs.map((box, i) =>
			i === 0
				? { ...box, spent_by: 'd'.repeat(64), spent_height: current.value.indexed_height }
				: box
		)
	};
	await page.getByRole('button', { name: 'Refresh', exact: true }).click();
	await expect(page.locator(`a[href="/tx/${'d'.repeat(64)}"]`)).toBeVisible();
	expect(current.details).toBe(details + 1);
	await expect(
		page.getByText(
			`Box details were checked at indexed block ${current.value.indexed_height!.toLocaleString('en-US')}.`,
			{ exact: false }
		)
	).toBeVisible();
});

test('mempool disappearance explains uncertainty and offers another check', async ({ page }) => {
	const current = await mock(page);
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Waiting for a block' })).toBeVisible();
	current.value = status('no_longer_observed');
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(
		page.getByRole('heading', { name: 'No longer in our node’s mempool' })
	).toBeVisible();
	await expect(
		page.getByText('This does not establish that it was rejected.', { exact: false })
	).toBeVisible();
	await page.getByText('What this observation covers').click();
	await expect(page.getByText('60 minutes without a check', { exact: false })).toBeVisible();
});

test('a lost local inclusion removes the old receipt instead of retaining confirmations', async ({
	page
}) => {
	const current = await mock(page, status('confirmed'));
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByText('3 confirmations at last check')).toBeVisible();
	current.value = status('not_observed');
	current.value.previous_inclusion = { block_id: tx.block_id, height: tx.height };
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(page.getByRole('heading', { name: 'Previous inclusion changed' })).toBeVisible();
	await expect(page.getByText('is no longer in the local index.', { exact: false })).toBeVisible();
	await expect(page.getByRole('region', { name: 'Transaction receipt' })).toHaveCount(0);
	await expect(page.getByText('3 confirmations at last check')).toHaveCount(0);
});

test('network outage preserves the last receipt with a visible stale notice', async ({ page }) => {
	const current = await mock(page, status('confirmed'));
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByText('3 confirmations at last check')).toBeVisible();
	current.fail = true;
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(page.getByText('Live check failed.', { exact: false })).toBeVisible();
	await expect(page.getByText('3 confirmations at last check')).toBeVisible();
	current.fail = false;
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(page.getByText('Live check failed.', { exact: false })).toHaveCount(0);
});

test('polling pauses when hidden and resumes immediately', async ({ page }) => {
	await page.clock.install();
	const current = await mock(page);
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Waiting for a block' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Check now' })).toBeEnabled();
	await page.evaluate(() => {
		Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' });
		document.dispatchEvent(new Event('visibilitychange'));
	});
	const hiddenRequests = current.requests;
	await page.clock.runFor(20000);
	expect(current.requests).toBe(hiddenRequests);
	current.value = status('confirmed');
	await page.evaluate(() => {
		Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
		document.dispatchEvent(new Event('visibilitychange'));
	});
	await expect(page.getByText('3 confirmations at last check')).toBeVisible();
});

test('unknown IDs and source outages remain honest and fit a narrow screen', async ({ page }) => {
	await page.setViewportSize({ width: 320, height: 900 });
	const current = await mock(page, status('not_observed'));
	await page.goto(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Not observed yet' })).toBeVisible();
	current.value = status('unavailable');
	current.value.mempool.observation = 'unavailable';
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(page.getByRole('heading', { name: 'Live status unavailable' })).toBeVisible();
	await expect(
		page.getByText('This does not mean the transaction failed.', { exact: false })
	).toBeVisible();
	expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
		true
	);
});

test('a pending hash resolves from search and unknown hashes offer explicit tracking', async ({
	page
}) => {
	const current = await mock(page);
	await page.route('**/v1/search?*', (route) =>
		route.fulfill({ status: 404, json: { detail: 'Not indexed' } })
	);
	await page.goto(`/search?q=${tx.id}`);
	await expect(page).toHaveURL(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Waiting for a block' })).toBeVisible();
	current.value = status('not_observed');
	await page.goto(`/search?q=${tx.id}`);
	await expect(page.getByRole('link', { name: 'Track this ID as a transaction' })).toHaveAttribute(
		'href',
		`/tx/${tx.id}`
	);
});

test('canonical conflicts link to the spending transaction without calling a mempool absence rejection', async ({
	page
}) => {
	const observation = status('conflicted');
	observation.conflicts = [{ input_id: 'a'.repeat(64), tx_id: 'c'.repeat(64), height: tx.height }];
	await mock(page, observation);
	await page.goto(`/tx/${tx.id}`);
	await expect(
		page.getByRole('heading', { name: 'An input was spent by another transaction' })
	).toBeVisible();
	await expect(page.locator(`a[href="/tx/${'c'.repeat(64)}"]`)).toBeVisible();
});

function detailedStatus(): TxStatusDto {
	const value = status();
	value.pending!.data_input_count = 1;
	value.pending!.details = {
		inputs: ['1'.repeat(64), '2'.repeat(64)],
		data_inputs: ['3'.repeat(64)],
		outputs: [
			{
				index: 0,
				id: '4'.repeat(64),
				address: '9f'.repeat(25),
				value: '9007199254740993',
				ergo_tree: '0008d3',
				tokens: [{ id: '5'.repeat(64), amount: '9007199254740993' }],
				token_count: 1,
				tokens_truncated: false,
				ergo_tree_truncated: false
			},
			{
				index: 1,
				id: null,
				value: '1100000',
				ergo_tree: '0008d3',
				tokens: [],
				token_count: null,
				tokens_truncated: false,
				ergo_tree_truncated: false
			}
		],
		inputs_truncated: false,
		data_inputs_truncated: false,
		outputs_truncated: false,
		complete: false
	};
	return value;
}

test('pending details expose exact outputs and unresolved inputs before inclusion, then yield to the receipt', async ({
	page
}) => {
	await page.clock.install();
	const current = await mock(page, detailedStatus());
	await page.goto(`/tx/${tx.id}`);
	const details = page.getByRole('region', { name: 'Pending transaction details', exact: true });
	await expect(details).toBeVisible();
	const liveStatus = page.getByRole('region', { name: 'Live transaction status', exact: true });
	await expect(liveStatus.getByText('0.0011 ERG', { exact: true })).toBeVisible();
	await expect(liveStatus.getByText('512 bytes', { exact: true })).toBeHidden();
	await liveStatus.getByText('Observation details', { exact: true }).click();
	await expect(liveStatus.getByText('512 bytes', { exact: true })).toBeVisible();
	await liveStatus.getByText('Observation details', { exact: true }).click();
	await expect(
		details.getByText('Input values and assets: unknown', { exact: true })
	).toBeVisible();
	await expect(details.getByText('9,007,199.254740993 ERG', { exact: true })).toBeVisible();
	await expect(details.getByText('9,007,199,254,740,993 raw units', { exact: true })).toBeVisible();
	await expect(
		details.getByRole('link', { name: 'Token ' + '5'.repeat(64), exact: true })
	).toHaveAttribute('href', '/token/' + '5'.repeat(64));
	await expect(details.getByRole('link', { name: '9f'.repeat(25), exact: true })).toHaveAttribute(
		'href',
		'/address/' + '9f'.repeat(25)
	);
	const second = details.getByRole('article', { name: 'Pending output 2', exact: true });
	await expect(second).toContainText('Output box ID not supplied');
	await expect(second).toContainText('Token entries not supplied; token contents are unknown.');
	await expect(second.getByRole('link')).toHaveCount(0);
	await expect(page.getByRole('region', { name: 'Transaction receipt' })).toHaveCount(0);
	const initialFetches = current.details;
	await details.getByRole('article').first().locator('summary').click();
	await expect(details.getByText('9,007,199,254,740,993 nanoERG', { exact: true })).toBeVisible();
	await expect(details.getByRole('button', { name: 'Copy output 1 ErgoTree' })).toBeVisible();
	expect(current.details).toBe(initialFetches);
	current.value = status('confirmed');
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(page.getByRole('region', { name: 'Transaction receipt' })).toBeVisible();
	await expect(details).toHaveCount(0);
});

test('retained pending detail evidence stays visibly stale on outage or disappearance', async ({
	page
}) => {
	await page.clock.install();
	const current = await mock(page, detailedStatus());
	await page.goto(`/tx/${tx.id}`);
	const details = page.getByRole('region', { name: 'Pending transaction details', exact: true });
	await expect(details.getByRole('article')).toHaveCount(2);
	current.fail = true;
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(details).toContainText(
		'These details are from the last successful pending observation'
	);
	await expect(details).toContainText('They do not establish the transaction’s current status.');
	current.fail = false;
	const retained = current.value.pending;
	current.value = status('no_longer_observed');
	current.value.pending = retained;
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(details).toContainText(
		'These details are from the last successful pending observation'
	);
	await expect(details.getByRole('article')).toHaveCount(2);
	current.value.pending = null;
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(details).toHaveCount(0);
});

test('pending expansion is local and capped projection is never presented as the whole transaction', async ({
	page
}) => {
	await page.clock.install();
	const observation = detailedStatus();
	const pending = observation.pending!;
	const detail = pending.details!;
	pending.output_count = 40;
	detail.outputs = Array.from({ length: 6 }, (_, index) => ({
		...detail.outputs[0],
		index,
		id: null,
		address: null,
		tokens: [],
		token_count: 0,
		ergo_tree: null,
		ergo_tree_truncated: true
	}));
	detail.outputs_truncated = true;
	const current = await mock(page, observation);
	await page.goto(`/tx/${tx.id}`);
	const details = page.getByRole('region', { name: 'Pending transaction details', exact: true });
	await expect(details.getByRole('article')).toHaveCount(4);
	await expect(details).toContainText(
		'Additional outputs were omitted. Listed values are not the complete output total.'
	);
	const requests = current.requests;
	const expand = details.getByRole('button', { name: 'Show all 6 loaded pending outputs' });
	await expand.focus();
	await page.keyboard.press('Enter');
	await expect(details.getByRole('article')).toHaveCount(6);
	await details.getByRole('article').first().locator('summary').click();
	await expect(
		details
			.getByText('Script omitted because it exceeds the detail limit.', { exact: true })
			.first()
	).toBeVisible();
	expect(current.requests).toBe(requests);
});

test('legacy summaries and malformed extended evidence remain distinct unavailable detail states', async ({
	page
}) => {
	await page.clock.install();
	const current = await mock(page);
	await page.goto(`/tx/${tx.id}`);
	const details = page.getByRole('region', { name: 'Pending transaction details', exact: true });
	await expect(details).toContainText('This server provides pending summary counts only.');
	current.value = detailedStatus();
	current.value.pending!.details!.outputs[0].value = 'not-an-amount';
	await page.getByRole('button', { name: 'Check now' }).click();
	await expect(details).toContainText('Pending details could not be interpreted safely.');
	await expect(details.getByRole('article')).toHaveCount(0);
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	test(`a normal pending output exposes its amount, address and token before mobile navigation in ${appearance}`, async ({
		page
	}) => {
		await page.clock.install();
		await page.addInitScript((appearance) => {
			localStorage.setItem('xp-appearance', appearance);
			localStorage.setItem('xp-density', 'standard');
		}, appearance);
		await page.setViewportSize({ width: 390, height: 844 });
		const observation = detailedStatus();
		const projection = observation.pending!.details!;
		projection.outputs[0].value = '2000000000';
		projection.outputs[0].tokens[0].amount = '1000';
		projection.outputs[1].token_count = 0;
		projection.complete = true;
		await mock(page, observation);
		await page.goto(`/tx/${tx.id}`);
		const details = page.getByRole('region', { name: 'Pending transaction details', exact: true });
		const output = details.getByRole('article', { name: 'Pending output 1', exact: true });
		await expect(output).toBeVisible();
		await expect(
			page.getByRole('heading', { level: 1, name: 'Transaction', exact: true })
		).toHaveCount(1);
		await expect(
			page.getByRole('button', { name: 'Copy transaction ID', exact: true })
		).toBeVisible();
		await page.evaluate(() => document.fonts.ready);
		const navigationBounds = await page.locator('.tabbar').boundingBox();
		for (const fact of [
			output.getByText('2 ERG', { exact: true }),
			output.getByRole('link', { name: '9f'.repeat(25), exact: true }),
			output.getByText('1,000 raw units', { exact: true })
		]) {
			const bounds = await fact.boundingBox();
			expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(navigationBounds!.y - 8);
			await expect(fact).toBeInViewport({ ratio: 1 });
		}
		const liveStatus = page.getByRole('region', { name: 'Live transaction status', exact: true });
		await liveStatus.getByText('Observation details', { exact: true }).focus();
		await page.keyboard.press('Enter');
		await expect(liveStatus.locator('code')).toHaveText(tx.id);
		await expect(liveStatus.locator('code')).toBeVisible();
		await expect(liveStatus).toContainText('Pending status reflects one configured node.');
	});
	for (const density of ['standard', 'compact']) {
		test(`pending output values precede unresolved references above mobile navigation in ${appearance} ${density}`, async ({
			page
		}) => {
			await page.clock.install();
			await page.addInitScript(
				({ appearance, density }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-density', density);
				},
				{ appearance, density }
			);
			await page.setViewportSize({ width: 390, height: 844 });
			const observation = detailedStatus();
			observation.pending!.input_count = 4;
			observation.pending!.details!.inputs.push('6'.repeat(64), '7'.repeat(64));
			await mock(page, observation);
			await page.goto(`/tx/${tx.id}`);
			const details = page.getByRole('region', {
				name: 'Pending transaction details',
				exact: true
			});
			await expect(details.getByRole('article')).toHaveCount(2);
			await page.evaluate(() => document.fonts.ready);
			const amount = details.getByText('9,007,199.254740993 ERG', { exact: true });
			const amountBounds = await amount.boundingBox();
			const inputBounds = await details
				.getByRole('region', { name: 'Pending inputs', exact: true })
				.boundingBox();
			const navigationBounds = await page.locator('.tabbar').boundingBox();
			expect(amountBounds!.y).toBeLessThanOrEqual(650);
			expect(amountBounds!.y + amountBounds!.height).toBeLessThanOrEqual(navigationBounds!.y - 8);
			expect(inputBounds!.y).toBeGreaterThan(amountBounds!.y + amountBounds!.height);
			await expect(amount).toBeInViewport({ ratio: 1 });
		});
	}
	test(`pending details stay usable and exact at 320px in ${appearance}`, async ({ page }) => {
		await page.clock.install();
		await page.addInitScript((appearance) => {
			localStorage.setItem('xp-appearance', appearance);
			localStorage.setItem('xp-density', 'compact');
		}, appearance);
		await page.setViewportSize({ width: 320, height: 900 });
		const observation = detailedStatus();
		const longAddress = '9f'.repeat(300);
		observation.pending!.details!.outputs[0].address = longAddress;
		await mock(page, observation);
		await page.goto(`/tx/${tx.id}`);
		const details = page.getByRole('region', { name: 'Pending transaction details', exact: true });
		await expect(details.getByRole('article')).toHaveCount(2);
		await page.evaluate(() => document.fonts.ready);
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
		const output = details.getByRole('article').first();
		const address = output.getByRole('link', { name: longAddress, exact: true });
		await expect(address).toHaveAttribute('href', '/address/' + longAddress);
		expect((await address.innerText()).length).toBeLessThan(30);
		expect((await output.boundingBox())!.height).toBeLessThan(450);
		await details.getByRole('article').first().locator('summary').focus();
		await page.keyboard.press('Enter');
		await expect(details.getByText('9,007,199,254,740,993 nanoERG', { exact: true })).toBeVisible();
		await expect(output.locator('dd').filter({ hasText: longAddress })).toBeVisible();
		await expect(
			output.getByRole('button', { name: 'Copy output 1 address', exact: true })
		).toBeVisible();
		const copy = await details
			.getByRole('button', { name: 'Copy output 1 ErgoTree' })
			.boundingBox();
		expect(copy!.height).toBeGreaterThanOrEqual(44);
		expect(copy!.width).toBeGreaterThanOrEqual(44);
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
	});
}
