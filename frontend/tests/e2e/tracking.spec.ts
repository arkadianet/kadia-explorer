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
