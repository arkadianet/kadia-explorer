import { expect, test } from '@playwright/test';
const order = 'f027c8ec3632686b6f13a3f9bc7841114db759d0901c3efe5976ea6d3729c905';
const tx = '3e9d39dba8d67644572407c5a1d4ebaa0992bb0edfe561fc44872aa5e7180ac3';

test('a shared application inspection is opt-in and discovers the supported order', async ({
	page
}) => {
	const requests: string[] = [];
	page.on('request', (request) => {
		if (request.url().includes('/v1/boxes/' + order)) requests.push(request.url());
	});
	await page.goto(`/applications?kind=box&id=${order}`);
	await expect(page.getByRole('heading', { name: 'Supported journeys.' })).toBeVisible();
	expect(requests).toHaveLength(0);
	await page.getByRole('button', { name: 'Inspect activity' }).click();
	await expect(page.getByRole('region', { name: 'Spectrum swap workflow' })).toBeVisible();
	expect(requests).toHaveLength(1);
	await expect(page.getByRole('link', { name: 'Share this inspection' })).toHaveAttribute(
		'href',
		`/applications?kind=box&id=${order}`
	);
	await page.getByLabel('Transaction or box ID').fill('f'.repeat(64));
	await expect(page.getByRole('region', { name: 'Spectrum swap workflow' })).toHaveCount(0);
});

test('pinned mismatches stay unavailable until the user removes the pin', async ({ page }) => {
	await page.goto(`/applications?kind=tx&id=${tx}&block=${'f'.repeat(64)}`);
	await page.getByRole('button', { name: 'Inspect activity' }).click();
	await expect(page.getByRole('alert')).toContainText('pinned inclusion');
	await page.getByRole('button', { name: 'Clear block pin' }).click();
	await page.getByRole('button', { name: 'Inspect activity' }).click();
	await expect(page.getByRole('region', { name: 'Spectrum swap workflow' })).toBeVisible();
});

test('developer resources are downloadable and the contract preserves amount strings', async ({
	page,
	request
}) => {
	await page.goto('/developers');
	const resources = page.getByRole('region', { name: 'Developer integration resources' });
	await expect(resources.getByRole('link', { name: 'Download OpenAPI' })).toHaveAttribute(
		'href',
		'/openapi.json'
	);
	await expect(resources.getByRole('link', { name: 'Client JS' })).toHaveAttribute('download', '');
	const response = await request.get('/openapi.json');
	expect(response.ok()).toBe(true);
	const contract = await response.json();
	expect(Object.keys(contract.paths)).toHaveLength(23);
	expect(contract.components.schemas.TokenDto.properties.amount.type).toBe('string');
	for (const asset of [
		'kadia-client.js',
		'operations.js',
		'kadia-client.d.ts',
		'api-types.d.ts',
		'inspect-transaction.mjs'
	])
		expect((await request.get('/sdk/' + asset)).ok()).toBe(true);
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const theme of ['light', 'dark']) {
		test(`${appearance} ${theme} application discovery and developer resources fit a narrow screen`, async ({
			page
		}) => {
			await page.addInitScript(
				({ appearance, theme }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', theme);
				},
				{ appearance, theme }
			);
			await page.setViewportSize({ width: 320, height: 900 });
			await page.goto(`/applications?kind=box&id=${order}`);
			await page.getByRole('button', { name: 'Inspect activity' }).click();
			await expect(page.getByRole('region', { name: 'Spectrum swap workflow' })).toBeVisible();
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
			await page.goto('/developers');
			await expect(
				page.getByRole('region', { name: 'Developer integration resources' })
			).toBeVisible();
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
		});
	}
}
