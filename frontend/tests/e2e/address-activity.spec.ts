import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { MOCK_ADDRESS } from './data.ts';
import type { AddressActivityDto, AddressActivityPageDto, TxDto } from '../../src/lib/api/types.ts';

const tx: TxDto = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/receipts/storage-rent-tx.json', import.meta.url),
		'utf8'
	)
);
const token = 'd'.repeat(64);
const item: AddressActivityDto = {
	id: tx.id,
	height: tx.height,
	block_id: tx.block_id!,
	timestamp: tx.timestamp,
	index: tx.index,
	fee: tx.fee,
	input_count: 2,
	output_count: 2,
	coverage: { complete: true, resolved_inputs: 2, total_inputs: 2 },
	erg_delta: '-989200',
	tokens: [{ id: token, name: '=Unverified token', decimals: 0, delta: '900719925474099312346' }],
	direction: 'mixed',
	asset_match: 'definite'
};
const unknown: AddressActivityDto = {
	...item,
	id: 'c'.repeat(64),
	coverage: { complete: false, resolved_inputs: 1, total_inputs: 2 },
	erg_delta: null,
	tokens: [{ ...item.tokens[0], delta: null }],
	direction: 'unknown',
	asset_match: 'uncertain'
};
function result(
	items: AddressActivityDto[] = [item],
	overrides: Partial<AddressActivityPageDto> = {}
): AddressActivityPageDto {
	return {
		items,
		next_cursor: null,
		next_snapshot: 'snapshot-one',
		consistency: 'strict',
		anchor: { height: tx.indexed_height!, block_id: 'b'.repeat(64) },
		observed_anchor: null,
		scanned: items.length,
		scan_limit_reached: false,
		partial_from: null,
		...overrides
	};
}
async function activity(page: Page, items = [item]) {
	await page.route(`**/v1/addresses/${MOCK_ADDRESS}/activity?**`, (route) =>
		route.fulfill({ json: result(items) })
	);
}

test('address activity shows exact mixed deltas and keeps unresolved candidates explicit', async ({
	page
}) => {
	await activity(page, [item, unknown]);
	await page.goto(`/address/${MOCK_ADDRESS}`);
	const section = page.getByRole('region', { name: 'Address activity', exact: true });
	await expect(page.getByRole('tab', { name: 'Activity', exact: true })).toHaveAttribute(
		'aria-selected',
		'true'
	);
	await expect(section.getByText('-0.0009892 ERG', { exact: true })).toBeVisible();
	await expect(section.getByText('+900,719,925,474,099,312,346', { exact: true })).toBeVisible();
	await expect(section.getByText('Exact net changes unavailable')).toBeVisible();
	await expect(section.getByText('Token involvement is uncertain', { exact: false })).toBeVisible();
	await expect(section.getByText('End of the matching indexed activity.')).toBeVisible();
	await expect(section.getByRole('link', { name: 'View receipt ↗' }).first()).toHaveAttribute(
		'href',
		`/tx/${tx.id}?address=${encodeURIComponent(MOCK_ADDRESS)}`
	);
});

test('filters request anchored server pages and an empty candidate scan can continue', async ({
	page
}) => {
	const requests: URL[] = [];
	await page.route(`**/v1/addresses/${MOCK_ADDRESS}/activity?**`, (route) => {
		const url = new URL(route.request().url());
		requests.push(url);
		return route.fulfill({
			json: url.searchParams.has('cursor')
				? result([item], { scanned: 1, partial_from: 100 })
				: result([], {
						next_cursor: 'candidate-200',
						scan_limit_reached: true,
						scanned: 200,
						partial_from: 100
					})
		});
	});
	await page.goto(
		`/address/${MOCK_ADDRESS}?asset=${token}&direction=mixed&from=2024-02-29&to=2024-02-29#activity`
	);
	await expect(
		page.getByText('More history remains to be checked.', { exact: false })
	).toBeVisible();
	await expect(page.getByText('No matching activity in the indexed range.')).toHaveCount(0);
	await expect(
		page.getByText('Indexed history starts at block 100', { exact: false })
	).toBeVisible();
	await page.getByRole('button', { name: 'Continue scanning' }).click();
	await expect(page.getByText('End of the matching indexed activity.')).toBeVisible();
	expect(requests).toHaveLength(2);
	for (const url of requests) {
		expect(url.searchParams.get('asset')).toBe(token);
		expect(url.searchParams.get('direction')).toBe('mixed');
		expect(url.searchParams.get('from_ms')).toBe(String(Date.UTC(2024, 1, 29)));
		expect(url.searchParams.get('to_ms')).toBe(String(Date.UTC(2024, 2, 1)));
		expect(url.searchParams.get('consistency')).toBe('strict');
	}
	expect(requests[1].searchParams.get('cursor')).toBe('candidate-200');
	expect(requests[1].searchParams.get('snapshot')).toBe('snapshot-one');
	await page.getByRole('combobox', { name: 'Asset', exact: true }).selectOption('erg');
	await page.getByRole('combobox', { name: 'Direction', exact: true }).selectOption('sent');
	await page.getByRole('button', { name: 'Apply filters' }).click();
	await expect(page).toHaveURL(new RegExp('asset=erg&direction=sent'));
	await expect.poll(() => requests.length).toBe(3);
	expect(requests[2].searchParams.has('cursor')).toBe(false);
});

test('snapshot changes discard incompatible rows and require explicit restart', async ({
	page
}) => {
	let restarted = false;
	await page.route(`**/v1/addresses/${MOCK_ADDRESS}/activity?**`, (route) => {
		if (new URL(route.request().url()).searchParams.has('cursor'))
			return route.fulfill({
				status: 409,
				json: { code: 'snapshot_changed', detail: 'Index changed' }
			});
		return route.fulfill({
			json: result(restarted ? [unknown] : [item], { next_cursor: restarted ? null : 'next' })
		});
	});
	await page.goto(`/address/${MOCK_ADDRESS}`);
	await page.getByRole('button', { name: 'Load more activity' }).click();
	await expect(page.getByText('The indexed snapshot changed.', { exact: false })).toBeVisible();
	await expect(page.locator('.activity-entry')).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Export loaded CSV' })).toBeDisabled();
	restarted = true;
	await page.getByRole('button', { name: 'Restart from latest snapshot' }).click();
	await expect(page.getByText('Exact net changes unavailable')).toBeVisible();
});

test('local labels persist safely and save dialog restores keyboard focus', async ({ page }) => {
	await activity(page);
	await page.goto(`/address/${MOCK_ADDRESS}`);
	const opener = page.getByRole('button', { name: 'Save address', exact: true });
	await opener.click();
	const dialog = page.getByRole('dialog', { name: 'Save address', exact: true });
	await expect(dialog.getByLabel('Local label', { exact: false })).toBeFocused();
	await dialog.getByLabel('Local label', { exact: false }).fill('<script>My local label</script>');
	await dialog.getByRole('button', { name: 'Save in this browser' }).focus();
	await page.keyboard.press('Tab');
	await expect(dialog.getByRole('button', { name: 'Close', exact: true })).toBeFocused();
	await page.keyboard.press('Shift+Tab');
	await expect(dialog.getByRole('button', { name: 'Save in this browser' })).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(opener).toBeFocused();
	await opener.click();
	await dialog.getByLabel('Local label', { exact: false }).fill('<script>My local label</script>');
	await dialog.getByRole('button', { name: 'Save in this browser' }).click();
	await expect(page.getByRole('button', { name: 'Edit saved address', exact: true })).toBeFocused();
	await page.goto('/saved');
	await expect(
		page.getByRole('heading', { name: '<script>My local label</script>' })
	).toBeVisible();
	await page.reload();
	await expect(
		page.getByRole('heading', { name: '<script>My local label</script>' })
	).toBeVisible();
	await page
		.getByRole('button', { name: 'Remove <script>My local label</script>', exact: true })
		.click();
	await expect(page.getByRole('heading', { name: 'No saved addresses yet' })).toBeVisible();
});

test('saved-address filtering matches local labels and address text without searching the chain', async ({
	page
}) => {
	await page.addInitScript(
		({ address }) => {
			localStorage.setItem(
				'xp-saved-addresses',
				JSON.stringify({
					version: 1,
					items: [
						{ address, label: 'Alpha collector', added_at: 1, updated_at: 1 },
						{ address: '8'.repeat(51), label: 'Beta savings', added_at: 1, updated_at: 1 }
					]
				})
			);
		},
		{ address: MOCK_ADDRESS }
	);
	const chainSearches: string[] = [];
	page.on('request', (request) => {
		if (new URL(request.url()).pathname === '/v1/search') chainSearches.push(request.url());
	});
	await page.goto('/saved');
	const input = page.getByRole('searchbox', { name: 'Filter saved addresses', exact: true });
	await expect(page.locator('.saved-entry')).toHaveCount(2);
	await input.fill(' ALPHA ');
	await expect(page.getByRole('heading', { name: 'Alpha collector', exact: true })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Beta savings', exact: true })).toHaveCount(0);
	await input.fill(MOCK_ADDRESS.slice(0, 16).toUpperCase());
	await expect(page.locator('.saved-entry')).toHaveCount(1);
	await expect(page.getByRole('link', { name: 'Open activity ↗', exact: true })).toHaveAttribute(
		'href',
		`/address/${MOCK_ADDRESS}#activity`
	);
	await input.fill('No matching saved label');
	await input.press('Enter');
	await expect(page.getByRole('heading', { name: 'No saved addresses match' })).toBeVisible();
	await expect(page.locator('.saved-entry')).toHaveCount(0);
	await expect(page).toHaveURL('/saved');
	expect(chainSearches).toHaveLength(0);
	await page.getByRole('button', { name: 'Clear filter', exact: true }).click();
	await expect(input).toBeFocused();
	await expect(page.locator('.saved-entry')).toHaveCount(2);
	expect(
		await page.evaluate(() => JSON.parse(localStorage.getItem('xp-saved-addresses')!).items.length)
	).toBe(2);
});

test('CSV downloads only loaded snapshot rows with exact integers and safe metadata', async ({
	page
}) => {
	await activity(page, [item, unknown]);
	await page.goto(`/address/${MOCK_ADDRESS}`);
	await expect(page.getByText('End of the matching indexed activity.')).toBeVisible();
	const [download] = await Promise.all([
		page.waitForEvent('download'),
		page.getByRole('button', { name: 'Export loaded CSV' }).click()
	]);
	const stream = await download.createReadStream();
	const chunks: Buffer[] = [];
	for await (const chunk of stream!) chunks.push(chunk);
	const contents = Buffer.concat(chunks).toString('utf8');
	expect(contents).toContain('"900719925474099312346"');
	expect(contents).toContain('"\'=Unverified token"');
	expect(contents).toContain('"-989200"');
	expect(contents).toContain('"unknown","false","1","2"');
	expect(contents.split('\r\n')).toHaveLength(6);
	await expect(page.getByText('Exported 2 loaded transactions', { exact: false })).toBeVisible();
});

test('receipt deep links preserve the originating address in both flow and receipt', async ({
	page
}) => {
	await page.route(`**/v1/txs/${tx.id}`, (route) => route.fulfill({ json: tx }));
	await page.route(`**/v1/txs/${tx.id}/status`, (route) =>
		route.fulfill({ status: 404, json: { detail: 'Legacy status route' } })
	);
	const selected = tx.inputs[0].box!;
	await page.goto(`/tx/${tx.id}?address=${encodeURIComponent(selected.address!)}`);
	const inspector = page.getByRole('region', { name: 'Selected address balance changes' });
	await expect(inspector.getByText('-2,000', { exact: true })).toBeVisible();
	await page.getByRole('tab', { name: 'Receipt', exact: true }).click();
	await expect(page.getByLabel('View changes for an address')).toHaveValue(selected.tree_hash);
	const fee = tx.outputs.find((box) => box.kind === 'fee')!;
	await page.goto(`/tx/${tx.id}?address=${encodeURIComponent(fee.address!)}`);
	await page.getByRole('tab', { name: 'Receipt', exact: true }).click();
	await expect(page.getByLabel('View changes for an address')).toHaveValue(fee.tree_hash);
	await expect(
		page.getByText('The requested address is not among the resolved transaction boxes.', {
			exact: false
		})
	).toHaveCount(0);
	await page.goto(`/tx/${tx.id}?address=unmatched`);
	await expect(
		page.getByText('The requested address is not among the resolved transaction boxes.', {
			exact: false
		})
	).toBeVisible();
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora'])
	for (const mode of ['light', 'dark']) {
		test(`${appearance} ${mode} activity and saved addresses fit a 320px viewport`, async ({
			page
		}) => {
			await page.setViewportSize({ width: 320, height: 900 });
			await page.addInitScript(
				({ appearance, mode, address }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', mode);
					localStorage.setItem(
						'xp-saved-addresses',
						JSON.stringify({
							version: 1,
							items: [
								{
									address,
									label: 'A long local label to check narrow layouts',
									added_at: 1,
									updated_at: 1
								}
							]
						})
					);
				},
				{ appearance, mode, address: MOCK_ADDRESS }
			);
			await activity(page, [item, unknown]);
			await page.goto(`/address/${MOCK_ADDRESS}`);
			await expect(page.getByText('End of the matching indexed activity.')).toBeVisible();
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
			).toBe(true);
			await page.getByRole('button', { name: 'Edit saved address', exact: true }).click();
			await expect(page.getByRole('dialog')).toBeVisible();
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
			).toBe(true);
			await page.keyboard.press('Escape');
			await page.goto('/saved');
			await expect(
				page.getByRole('heading', { name: 'A long local label to check narrow layouts' })
			).toBeVisible();
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
			).toBe(true);
		});
	}
