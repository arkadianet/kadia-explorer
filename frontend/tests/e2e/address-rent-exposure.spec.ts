import { expect, test } from '@playwright/test';
import { MOCK_ADDRESS } from './data';
import type { RentExposure, RentExposureBox } from '../../src/lib/rent/exposure';

const routePattern = `**/v1/addresses/${MOCK_ADDRESS}/rent?view=exposure`;
const anchor = 'a'.repeat(64);
function row(n: number, maturity: number, fee = '125000000'): RentExposureBox {
	return {
		id: n.toString(16).padStart(64, '0'),
		value: '10000000',
		size: 100,
		creation_height: maturity - 1_051_200,
		token_count: 1,
		rent: {
			maturity_height: maturity,
			due_nano: '10000000',
			consensus_fee_nano: fee,
			collectible: BigInt(fee) > 0n,
			claimable_at_tip: maturity <= 1_882_694
		}
	};
}
function snapshot(): RentExposure {
	return {
		items: [row(1, 1_882_694), row(2, 1_882_700), row(3, 1_882_000, '-1794967296')],
		truncated: false,
		context: {
			scope: 'indexed_unspent_boxes',
			address: MOCK_ADDRESS,
			tree_hash: 'b'.repeat(64),
			indexed_height: 1_882_694,
			anchor: { height: 1_882_694, block_id: anchor },
			full_history: true,
			partial_from: null,
			scanned_count: 3,
			scan_limit: 5000,
			scan_complete: true
		}
	};
}

test('shared rent controls require explicit load; classifications and exact fees use the read anchor', async ({
	page
}) => {
	let calls = 0;
	await page.route(routePattern, (route) => {
		calls++;
		return route.fulfill({ json: snapshot() });
	});
	await page.goto(`/address/${MOCK_ADDRESS}?rent_horizon=72#rent`);
	const view = page.getByRole('region', { name: 'Storage rent exposure' });
	await expect(view.getByText('Ready when you are.', { exact: false })).toBeVisible();
	expect(calls).toBe(0);
	await view.getByRole('button', { name: 'Load rent exposure' }).click();
	await expect(view.getByRole('link', { name: 'Block 1,882,694' })).toHaveAttribute(
		'href',
		`/blocks/${anchor}`
	);
	await expect(view.getByText('6 blocks remaining', { exact: true })).toBeVisible();
	await expect(view.getByText('1 mature and collectible boxes', { exact: true })).toBeVisible();
	await expect(view.getByRole('list', { name: 'Scanned rent boxes' }).locator('li')).toHaveCount(3);
	await view.getByLabel('Box category', { exact: true }).selectOption('non_collectible');
	await expect(page).toHaveURL(/rent_filter=non_collectible#rent$/);
	const rows = view.getByRole('list', { name: 'Scanned rent boxes' }).locator('li');
	await expect(rows).toHaveCount(1);
	await expect(rows).toContainText('Not collectible');
	await rows.getByText('Box evidence', { exact: true }).click();
	await expect(rows.getByText('-1794967296', { exact: true })).toBeVisible();
	await view.getByLabel('Approaching horizon', { exact: true }).selectOption('720');
	await expect(page).toHaveURL(/rent_horizon=720/);
	expect(calls).toBe(1);
});

test('partial and truncated reads remain scoped; refresh failure retains labelled stale evidence', async ({
	page
}) => {
	let calls = 0;
	const data = snapshot();
	data.context.full_history = false;
	data.context.partial_from = 1_800_000;
	data.truncated = true;
	data.context.scan_complete = false;
	await page.route(routePattern, (route) =>
		++calls === 1
			? route.fulfill({ json: data })
			: route.fulfill({ status: 503, json: { detail: 'Store temporarily unavailable.' } })
	);
	await page.goto(`/address/${MOCK_ADDRESS}#rent`);
	const view = page.getByRole('region', { name: 'Storage rent exposure' });
	await view.getByRole('button', { name: 'Load rent exposure' }).click();
	await expect(view.getByText('Scanned boxes only', { exact: true })).toBeVisible();
	await expect(
		view.getByText('Earlier outputs and unresolved pre-index spends', { exact: false })
	).toBeVisible();
	await expect(view.getByText(/unscanned boxes may mature sooner/)).toBeVisible();
	await view.getByRole('button', { name: 'Refresh rent exposure' }).click();
	await expect(view.getByRole('alert')).toContainText('Previous snapshot retained; it is stale.');
	await expect(view.getByText('Previous snapshot · stale', { exact: true })).toBeVisible();
	await expect(view.getByRole('list', { name: 'Scanned rent boxes' }).locator('li')).toHaveCount(3);
});

for (const category of ['toString', '__proto__']) {
	test(`unsupported rent category ${category} falls back to all scanned boxes`, async ({
		page
	}) => {
		await page.route(routePattern, (route) => route.fulfill({ json: snapshot() }));
		await page.goto(`/address/${MOCK_ADDRESS}?rent_filter=${category}#rent`);
		const view = page.getByRole('region', { name: 'Storage rent exposure' });
		await view.getByRole('button', { name: 'Load rent exposure' }).click();
		await expect(view.getByLabel('Box category', { exact: true })).toHaveValue('all');
		await expect(view.getByRole('list', { name: 'Scanned rent boxes' }).locator('li')).toHaveCount(
			3
		);
		await expect(
			view.getByText('No scanned boxes match this category.', { exact: true })
		).toHaveCount(0);
	});
}

test('legacy responses are unavailable, while a successful empty snapshot is explicit evidence', async ({
	page
}) => {
	let calls = 0;
	const empty = snapshot();
	empty.items = [];
	empty.context.scanned_count = 0;
	await page.route(routePattern, (route) =>
		route.fulfill({ json: ++calls === 1 ? { items: [], truncated: false } : empty })
	);
	await page.goto(`/address/${MOCK_ADDRESS}#rent`);
	const view = page.getByRole('region', { name: 'Storage rent exposure' });
	await view.getByRole('button', { name: 'Load rent exposure' }).click();
	await expect(view.getByRole('alert')).toContainText('consistent anchored rent exposure');
	await expect(view.getByText('Mature collectible exposure', { exact: true })).toHaveCount(0);
	await view.getByRole('button', { name: 'Load rent exposure' }).click();
	await expect(
		view.getByText('No unspent boxes were found in this indexed address snapshot.', { exact: true })
	).toBeVisible();
	await expect(view.getByText('Complete indexed unspent set', { exact: true })).toBeVisible();
});

test('rendered boxes are bounded and local paging does not repeat the scan', async ({ page }) => {
	let calls = 0;
	const data = snapshot();
	data.items = Array.from({ length: 101 }, (_, index) => row(index + 1, 1_882_694));
	data.context.scanned_count = 101;
	await page.route(routePattern, (route) => {
		calls++;
		return route.fulfill({ json: data });
	});
	await page.goto(`/address/${MOCK_ADDRESS}#rent`);
	const view = page.getByRole('region', { name: 'Storage rent exposure' });
	await view.getByRole('button', { name: 'Load rent exposure' }).click();
	await expect(view.getByRole('list', { name: 'Scanned rent boxes' }).locator('li')).toHaveCount(
		100
	);
	await view.getByRole('button', { name: 'Next boxes' }).click();
	await expect(view.getByRole('list', { name: 'Scanned rent boxes' }).locator('li')).toHaveCount(1);
	await expect(view.getByText('101–101 of 101', { exact: true })).toBeVisible();
	expect(calls).toBe(1);
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const theme of ['light', 'dark']) {
		test(`${appearance} ${theme} rent inspector preserves evidence and controls at 320px`, async ({
			page
		}) => {
			await page.setViewportSize({ width: 320, height: 740 });
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await page.route(routePattern, (route) => route.fulfill({ json: snapshot() }));
			await page.goto(`/address/${MOCK_ADDRESS}#rent`);
			const view = page.getByRole('region', { name: 'Storage rent exposure' });
			await view.getByRole('button', { name: 'Load rent exposure' }).click();
			await expect(view.getByText('Mature collectible exposure', { exact: true })).toBeVisible();
			await expect(view.getByLabel('Box category', { exact: true })).toBeVisible();
			await expect(
				view.getByRole('link', { name: snapshot().items[0].id, exact: true })
			).toBeVisible();
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
				320
			);
		});
	}
}
