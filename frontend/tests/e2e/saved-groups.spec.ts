import { expect, test, type Page } from '@playwright/test';
import { MOCK_ADDRESS } from './data.ts';

const otherAddress = '8'.repeat(51);
const entry = { address: MOCK_ADDRESS, label: 'Alpha research', added_at: 1, updated_at: 2 };
const other = { ...entry, address: otherAddress, label: 'Beta notes' };
const encode = (items: object[], version = 2) => JSON.stringify({ version, items });

async function seed(page: Page, raw = encode([entry], 1)) {
	await page.addInitScript((raw) => {
		if (localStorage.getItem('xp-saved-addresses') === null)
			localStorage.setItem('xp-saved-addresses', raw);
	}, raw);
}
async function backup(page: Page, raw: string) {
	const tools = page.locator('.backup-tools');
	if (!(await tools.getAttribute('open'))) {
		if (!(await tools.evaluate((element) => (element as HTMLDetailsElement).open)))
			await tools.getByText('Backup and restore', { exact: true }).click();
	}
	await page
		.getByLabel('Choose JSON backup', { exact: true })
		.setInputFiles({ name: 'saved.json', mimeType: 'application/json', buffer: Buffer.from(raw) });
}
const stored = (page: Page) => page.evaluate(() => localStorage.getItem('xp-saved-addresses'));

test('groups migrate v1 on edit, filter locally, and survive reload', async ({ page }) => {
	await seed(page, encode([entry, other], 1));
	const chainRequests: string[] = [];
	page.on('request', (request) => {
		const path = new URL(request.url()).pathname;
		if (path.startsWith('/v1/') && path !== '/v1/status') chainRequests.push(path);
	});
	await page.goto('/saved');
	expect(JSON.parse((await stored(page))!).version).toBe(1);
	const alpha = page
		.locator('.saved-entry')
		.filter({ has: page.getByRole('heading', { name: entry.label, exact: true }) });
	await alpha.getByRole('button', { name: 'Edit saved address', exact: true }).click();
	const dialog = page.getByRole('dialog', { name: 'Edit saved address', exact: true });
	await dialog.getByLabel(/^Group/).fill('Research');
	await dialog.getByRole('button', { name: 'Save changes', exact: true }).click();
	await page.getByLabel('Filter by group', { exact: true }).selectOption('name:Research');
	await expect(page.locator('.saved-entry')).toHaveCount(1);
	await expect(alpha).toContainText('Group: Research');
	await page.getByLabel('Filter by group', { exact: true }).selectOption('ungrouped');
	await expect(page.getByRole('heading', { name: other.label, exact: true })).toBeVisible();
	const migrated = JSON.parse((await stored(page))!);
	expect(migrated.version).toBe(2);
	expect(migrated.items.find((item: { address: string }) => item.address === otherAddress)).toEqual(
		other
	);
	await page.reload();
	await page.getByLabel('Filter by group', { exact: true }).selectOption('name:Research');
	await expect(page.getByRole('heading', { name: entry.label, exact: true })).toBeVisible();
	expect(chainRequests).toEqual([]);
});

test('backup preview preserves existing labels by default and exports the merged local data', async ({
	page
}) => {
	const original = encode([entry], 1);
	await seed(page, original);
	await page.goto('/saved');
	await backup(
		page,
		encode([
			{ ...entry, label: 'Backup label', group: 'Imported' },
			{ ...other, group: 'Research' }
		])
	);
	await expect(page.getByRole('heading', { name: 'Review backup', exact: true })).toBeFocused();
	await expect(
		page.getByRole('radio', { name: 'Keep existing labels and groups', exact: true })
	).toBeChecked();
	await expect(
		page.getByText('1 new · 1 conflicting · 0 unchanged.', { exact: false })
	).toBeVisible();
	expect(await stored(page)).toBe(original);
	await page.getByRole('button', { name: 'Merge backup', exact: true }).click();
	await expect(
		page.getByText('1 conflicting entries kept unchanged.', { exact: false })
	).toBeVisible();
	expect(JSON.parse((await stored(page))!).items).toEqual([entry, { ...other, group: 'Research' }]);
	const [download] = await Promise.all([
		page.waitForEvent('download'),
		page.getByRole('button', { name: 'Export JSON backup', exact: true }).click()
	]);
	const stream = await download.createReadStream();
	const chunks: Buffer[] = [];
	for await (const chunk of stream!) chunks.push(chunk);
	const data = JSON.parse(Buffer.concat(chunks).toString('utf8'));
	expect(data).toMatchObject({
		format: 'kadia.saved-addresses',
		version: 2,
		items: [entry, { ...other, group: 'Research' }]
	});
	expect(Number.isSafeInteger(data.exported_at)).toBe(true);
});

test('replacement of local fields is explicit, while cancel leaves every stored byte unchanged', async ({
	page
}) => {
	const original = encode([{ ...entry, group: 'Work' }]);
	await seed(page, original);
	await page.goto('/saved');
	const raw = encode([{ ...entry, label: '' }]);
	await backup(page, raw);
	await page.getByRole('button', { name: 'Cancel preview', exact: true }).click();
	expect(await stored(page)).toBe(original);
	await expect(page.getByLabel('Choose JSON backup', { exact: true })).toBeFocused();
	await backup(page, raw);
	await page
		.getByRole('radio', {
			name: 'Use backup labels and groups, including blank values',
			exact: true
		})
		.check();
	await expect(page.getByRole('region', { name: 'Backup entries', exact: true })).toContainText(
		'Update from backup'
	);
	await page.getByRole('button', { name: 'Merge backup', exact: true }).click();
	const changed = JSON.parse((await stored(page))!).items[0];
	expect(changed.label).toBe('');
	expect(changed.group).toBeUndefined();
	expect(changed.added_at).toBe(1);
});

test('a stale import preview cannot overwrite a newer local edit', async ({ page }) => {
	await seed(page);
	await page.goto('/saved');
	await backup(page, encode([other]));
	// File.text() is asynchronous: create the original review before simulating a
	// later edit, otherwise the new bytes are legitimately part of the first preview.
	await expect(page.getByRole('heading', { name: 'Review backup', exact: true })).toBeFocused();
	const concurrent = encode([{ ...entry, label: 'Changed elsewhere' }]);
	await page.evaluate((raw) => localStorage.setItem('xp-saved-addresses', raw), concurrent);
	await page.getByRole('button', { name: 'Merge backup', exact: true }).click();
	await expect(page.getByRole('alert')).toContainText('changed after this preview');
	expect(await stored(page)).toBe(concurrent);
	await page.getByRole('button', { name: 'Preview again', exact: true }).click();
	await page.getByRole('button', { name: 'Merge backup', exact: true }).click();
	expect(JSON.parse((await stored(page))!).items).toEqual([
		{ ...entry, label: 'Changed elsewhere' },
		other
	]);
});

test('malformed and oversized backups never modify browser data', async ({ page }) => {
	const original = encode([entry]);
	await seed(page, original);
	await page.goto('/saved');
	await backup(page, encode([{ ...other, group: { unexpected: true } }]));
	await expect(page.getByRole('alert')).toContainText('invalid entry');
	await expect(page.getByRole('button', { name: 'Merge backup', exact: true })).toHaveCount(0);
	expect(await stored(page)).toBe(original);
	await backup(page, 'x'.repeat(512_001));
	await expect(
		page.getByText('Backup exceeds the 512,000-byte limit.', { exact: true })
	).toBeVisible();
	expect(await stored(page)).toBe(original);
});

test('corrupt browser state is retained and cannot be replaced by importing a valid backup', async ({
	page
}) => {
	await seed(page, '{broken');
	await page.goto('/saved');
	await expect(
		page.getByRole('heading', { name: 'Saved data could not be read', exact: true })
	).toBeVisible();
	await backup(page, encode([other]));
	await expect(page.getByRole('alert')).toContainText('Existing browser data has not been changed');
	await expect(
		page.getByRole('button', { name: 'Export JSON backup', exact: true })
	).toBeDisabled();
	await expect(page.getByRole('button', { name: 'Merge backup', exact: true })).toHaveCount(0);
	expect(await stored(page)).toBe('{broken');
});

for (const appearance of ['original', 'aurora', 'atelier', 'prism'])
	for (const mode of ['light', 'dark'])
		test(`${appearance} ${mode} group editing and backup preview fit 320px and retain keyboard focus`, async ({
			page
		}) => {
			await page.setViewportSize({ width: 320, height: 900 });
			await page.addInitScript(
				({ appearance, mode }) => {
					localStorage.setItem('xp-appearance', appearance);
					localStorage.setItem('xp-theme', mode);
				},
				{ appearance, mode }
			);
			await seed(
				page,
				encode([
					{
						...entry,
						label: 'A long local label for a saved address',
						group: 'Research and contract notes'
					}
				])
			);
			await page.goto('/saved');
			const opener = page.getByRole('button', { name: 'Edit saved address', exact: true });
			await opener.click();
			const dialog = page.getByRole('dialog', { name: 'Edit saved address', exact: true });
			await expect(dialog.getByLabel(/^Local label/)).toBeFocused();
			await page.keyboard.press('Tab');
			await expect(dialog.getByLabel(/^Group/)).toBeFocused();
			await page.keyboard.press('Escape');
			await expect(opener).toBeFocused();
			await backup(
				page,
				encode([
					{
						...entry,
						label: 'An imported label that conflicts with the existing one',
						group: 'Another organizational group'
					},
					{ ...other, group: 'Other records' }
				])
			);
			await expect(page.getByRole('heading', { name: 'Review backup', exact: true })).toBeVisible();
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
			).toBe(true);
			const region = page.getByRole('region', { name: 'Backup entries', exact: true });
			await region.focus();
			await expect(region).toBeFocused();
			await page.getByRole('button', { name: 'Cancel preview', exact: true }).click();
			await expect(page.getByLabel('Choose JSON backup', { exact: true })).toBeFocused();
		});
