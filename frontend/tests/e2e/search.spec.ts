import { expect, test } from '@playwright/test';
import { MOCK_ADDRESS, UNKNOWN_HEX, block, claimableBox, tx } from './data.ts';
import { readFileSync } from 'node:fs';

/** Types a query into the header search box and submits it. */
async function search(page: import('@playwright/test').Page, q: string) {
	const input = page.getByRole('searchbox');
	await input.fill(q);
	await input.press('Enter');
}

test('a height goes straight to the block page', async ({ page }) => {
	await page.goto('/');
	await search(page, String(block.height));
	await expect(page).toHaveURL(`/blocks/${block.height}`);
	await expect(page.getByRole('heading', { name: `Block ${block.height}` })).toBeVisible();
});

test('a transaction id resolves through the API to the tx page', async ({ page }) => {
	await page.goto('/');
	await search(page, tx.id);
	await expect(page).toHaveURL(`/tx/${tx.id}`);
	await expect(page.getByRole('heading', { name: 'Transaction', exact: true })).toBeVisible();
});

test('a box id resolves through the API to the box page', async ({ page }) => {
	await page.goto('/');
	await search(page, claimableBox.id);
	await expect(page).toHaveURL(`/box/${claimableBox.id}`);
	await expect(page.getByRole('heading', { name: 'Box', exact: true })).toBeVisible();
});

test('a block id resolves through the API to the block page', async ({ page }) => {
	await page.goto('/');
	await search(page, block.id);
	await expect(page).toHaveURL(`/blocks/${block.id}`);
	await expect(page.getByRole('heading', { name: `Block ${block.height}` })).toBeVisible();
});

test('an address goes straight to the address page', async ({ page }) => {
	await page.goto('/');
	await search(page, MOCK_ADDRESS);
	await expect(page).toHaveURL(`/address/${MOCK_ADDRESS}`);
	await expect(page.getByRole('heading', { name: 'Address' })).toBeVisible();
});

test('a long H-prefix address reaches server validation', async ({ page }) => {
	const address = readFileSync(
		new URL('../../../tests/fixtures/receipts/p2s-address.txt', import.meta.url),
		'utf8'
	).trim();
	let requested = '';
	await page.route('**/v1/search?*', (route) => {
		requested = new URL(route.request().url()).searchParams.get('q')!;
		return route.fulfill({ json: { kind: 'address', id: MOCK_ADDRESS } });
	});
	await page.goto(`/search?q=${address}`);
	await expect(page).toHaveURL(`/address/${MOCK_ADDRESS}`);
	expect(requested).toBe(address);
});

test('an invalid address checksum receives a format hint', async ({ page }) => {
	await page.route('**/v1/search?*', (route) =>
		route.fulfill({ status: 400, json: { detail: 'Invalid address' } })
	);
	await page.goto(`/search?q=${MOCK_ADDRESS.slice(0, -1)}1`);
	await expect(
		page.getByText('This is not a valid indexed address or ID.', { exact: false })
	).toBeVisible();
});

for (const failure of [400, 429, 503, 'network'] as const) {
	test(`an address-shaped token name still searches names after ${failure}`, async ({ page }) => {
		await page.route('**/v1/search?*', (route) =>
			failure === 'network'
				? route.abort('failed')
				: route.fulfill({ status: failure, json: { detail: 'Address lookup failed' } })
		);
		const names = page.waitForResponse((response) =>
			new URL(response.url()).pathname.endsWith('/v1/tokens/search')
		);
		await page.goto('/search?q=ErgoPad&match=exact');
		const response = await names;
		expect(response.ok()).toBe(true);
		expect(new URL(response.url()).searchParams.get('q')).toBe('ErgoPad');
		expect(new URL(response.url()).searchParams.get('match')).toBe('exact');
		await expect(page.getByRole('textbox', { name: 'Token name', exact: true })).toHaveValue(
			'ErgoPad'
		);
		await expect(
			page.getByText('This is not a valid indexed address or ID.', { exact: false })
		).toHaveCount(0);
	});
}

test('the invalid-address notice starts above the short-name threshold without blocking names', async ({
	page
}) => {
	await page.route('**/v1/search?*', (route) =>
		route.fulfill({ status: 400, json: { detail: 'Invalid address' } })
	);
	for (const length of [32, 33]) {
		const query = 'N'.repeat(length);
		await page.goto(`/search?q=${query}`);
		await expect(page.getByRole('textbox', { name: 'Token name', exact: true })).toHaveValue(query);
		await expect(page.getByRole('heading', { name: 'No indexed name matches' })).toBeVisible();
		await expect(
			page.getByText('This is not a valid indexed address or ID.', { exact: false })
		).toHaveCount(length > 32 ? 1 : 0);
	}
});

test('an address-shaped query retains not-found behavior after a 404', async ({ page }) => {
	let nameRequests = 0;
	await page.route('**/v1/search?*', (route) =>
		route.fulfill({ status: 404, json: { detail: 'Not found' } })
	);
	page.on('request', (request) => {
		if (new URL(request.url()).pathname.endsWith('/v1/tokens/search')) nameRequests++;
	});
	await page.goto('/search?q=ErgoPad');
	await expect(page.getByText('No match for "ErgoPad".', { exact: false })).toBeVisible();
	await expect(page.getByRole('region', { name: 'Token discovery' })).toHaveCount(0);
	expect(nameRequests).toBe(0);
});

for (const failure of [503, 'network'] as const) {
	test(`an ID lookup still reports ${failure} instead of searching token names`, async ({
		page
	}) => {
		await page.route('**/v1/search?*', (route) =>
			failure === 'network'
				? route.abort('failed')
				: route.fulfill({ status: failure, json: { detail: 'ID lookup failed' } })
		);
		await page.goto(`/search?q=${UNKNOWN_HEX}`);
		await expect(
			page.getByRole('heading', { name: failure === 'network' ? '500' : '503', exact: true })
		).toBeVisible();
		await expect(page.getByRole('region', { name: 'Token discovery' })).toHaveCount(0);
	});
}

test('ordinary text searches minted token names without claiming a match', async ({ page }) => {
	await page.goto('/');
	await search(page, 'not a real query!');
	await expect(page).toHaveURL(/\/search\?q=/);
	await expect(page.getByRole('heading', { name: 'No indexed name matches' })).toBeVisible();
	await expect(page.getByRole('textbox', { name: 'Token name', exact: true })).toHaveValue(
		'not a real query!'
	);
});

test('a well-formed but unknown id reports not found', async ({ page }) => {
	await page.goto(`/search?q=${UNKNOWN_HEX}`);
	await expect(page.getByText(/64-hex ids can be block, transaction or box ids/)).toBeVisible();
});

test('pressing "/" focuses the search input', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByRole('heading', { name: 'Recent blocks' })).toBeVisible();
	await page.locator('body').click();
	await page.keyboard.press('/');
	await expect(page.getByRole('searchbox')).toBeFocused();
	// The key press focuses rather than typing into the field.
	await expect(page.getByRole('searchbox')).toHaveValue('');
});

test('/search has its own search box, and only the header owns #global-search', async ({
	page
}) => {
	await page.goto('/search');
	// Two boxes on the page (header + in-panel), but the id — and the "/" shortcut that goes
	// with it — belongs to exactly one of them.
	await expect(page.getByRole('searchbox')).toHaveCount(2);
	await expect(page.locator('#global-search')).toHaveCount(1);
	await expect(page.locator('#search-page-search')).toHaveCount(1);
});

test('Escape clears the search box it is typed into', async ({ page }) => {
	await page.goto('/');
	const input = page.getByRole('searchbox');
	await input.fill('9abc');
	await expect(input).toHaveValue('9abc');
	await input.press('Escape');
	await expect(input).toHaveValue('');
	await expect(input).not.toBeFocused();
	// Escape is a no-op for navigation.
	await expect(page).toHaveURL('/');
});

test('the in-page search box on /search still submits', async ({ page }) => {
	await page.goto('/search');
	const input = page.locator('#search-page-search');
	await input.fill(String(block.height));
	await input.press('Enter');
	await expect(page).toHaveURL(`/blocks/${block.height}`);
});

test('a shared mint input ID offers both token and box destinations', async ({ page }) => {
	await page.route('**/v1/search?*', (route) =>
		route.fulfill({
			json: {
				kind: 'box',
				id: claimableBox.id,
				matches: [
					{ kind: 'box', id: claimableBox.id },
					{ kind: 'token', id: claimableBox.id }
				]
			}
		})
	);
	await page.goto(`/search?q=${claimableBox.id}`);
	await expect(page.getByRole('link', { name: 'Mint input box', exact: true })).toHaveAttribute(
		'href',
		`/box/${claimableBox.id}`
	);
	await expect(page.getByRole('link', { name: 'Token', exact: true })).toHaveAttribute(
		'href',
		`/token/${claimableBox.id}`
	);
	await page.getByRole('link', { name: 'Mint input box', exact: true }).click();
	await expect(page).toHaveURL(`/box/${claimableBox.id}`);
});
