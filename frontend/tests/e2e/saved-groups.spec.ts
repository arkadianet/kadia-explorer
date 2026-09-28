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

function groupSnapshot(addresses: string[]) {
	const nano = '9007199254740993123';
	const token = { id: 'b'.repeat(64), amount: '18446744073709551615' };
	return {
		scope: 'selected_address_scripts',
		consistency: 'single_reader',
		indexed_height: 1000,
		anchor: { height: 1000, block_id: 'c'.repeat(64) },
		full_history: true,
		partial_from: null as number | null,
		complete: true,
		requested_count: addresses.length,
		resolved_script_count: addresses.length,
		members: addresses.map((address, index) => ({
			address,
			tree_hash: String(index + 1).padStart(64, '0') as string | null,
			status: 'resolved',
			duplicate_of: null as number | null,
			balance: { nano, tokens: [token] } as { nano: string; tokens: (typeof token)[] } | null
		})),
		observed_totals: {
			nano: (BigInt(nano) * BigInt(addresses.length)).toString(),
			tokens: [{ ...token, amount: (BigInt(token.amount) * BigInt(addresses.length)).toString() }]
		} as { nano: string; tokens: (typeof token)[] } | null
	};
}

test('group balances are explicit, send only visible addresses, and clear when filters change membership', async ({
	page
}) => {
	await seed(
		page,
		encode([
			{ ...entry, group: 'Research' },
			{ ...other, group: 'Other' }
		])
	);
	const bodies: unknown[] = [];
	await page.route('**/v1/addresses/balances', async (route) => {
		const body = route.request().postDataJSON();
		bodies.push(body);
		expect(route.request().method()).toBe('POST');
		await route.fulfill({ json: groupSnapshot(body.addresses) });
	});
	await page.goto('/saved');
	await page.getByLabel('Filter by group', { exact: true }).selectOption('name:Research');
	await page.getByLabel('Filter saved addresses', { exact: true }).fill('Alpha');
	await expect(page.locator('.saved-entry')).toHaveCount(1);
	expect(bodies).toEqual([]);
	const dashboard = page.getByRole('region', { name: 'Group balances', exact: true });
	await expect(dashboard).toContainText('Loading sends these visible addresses together');
	await dashboard.getByRole('button', { name: 'Load group balances', exact: true }).click();
	await expect(dashboard.getByText('Complete indexed balance', { exact: true })).toBeVisible();
	await expect(dashboard.getByRole('link', { name: 'Block 1,000', exact: true })).toHaveAttribute(
		'href',
		`/blocks/${'c'.repeat(64)}`
	);
	expect(bodies).toEqual([{ addresses: [entry.address] }]);
	await expect(dashboard.locator('.erg-total')).toContainText('9,007,199,254.740993123');
	await expect(dashboard.locator('.token-balances')).toContainText('18,446,744,073,709,551,615');
	await page.getByLabel('Filter saved addresses', { exact: true }).fill('No matching label');
	await expect(dashboard.locator('.snapshot')).toHaveCount(0);
	await expect(
		dashboard.getByRole('button', { name: 'Load group balances', exact: true })
	).toBeDisabled();
	expect(bodies).toHaveLength(1);
});

test('canonical aliases count once and a failed refresh retains a clearly stale snapshot', async ({
	page
}) => {
	await seed(page, encode([entry, other]));
	let request = 0;
	await page.route('**/v1/addresses/balances', async (route) => {
		if (++request === 2) {
			await route.fulfill({ status: 503, json: { title: 'Busy', detail: 'Try again' } });
			return;
		}
		const data = groupSnapshot([entry.address, other.address]);
		data.members[1] = {
			...data.members[1],
			tree_hash: data.members[0].tree_hash,
			status: 'duplicate',
			duplicate_of: 0,
			balance: null
		};
		data.resolved_script_count = 1;
		data.observed_totals = data.members[0].balance;
		await route.fulfill({ json: data });
	});
	await page.goto('/saved');
	const dashboard = page.getByRole('region', { name: 'Group balances', exact: true });
	await dashboard.getByRole('button', { name: 'Load group balances', exact: true }).click();
	await expect(dashboard.getByText('Same script as entry 1', { exact: true })).toBeVisible();
	await expect(dashboard.locator('.erg-total')).toContainText('9,007,199,254.740993123');
	await dashboard.getByRole('button', { name: 'Refresh group balances', exact: true }).click();
	await expect(dashboard.getByRole('alert')).toContainText('Stale snapshot — refresh failed');
	await expect(dashboard.locator('.snapshot')).toContainText('Previous snapshot');
	await expect(dashboard.locator('.erg-total')).toContainText('9,007,199,254.740993123');
	await dashboard.getByRole('button', { name: 'Refresh group balances', exact: true }).click();
	await expect(dashboard.getByRole('alert')).toHaveCount(0);
	await expect(dashboard.locator('.snapshot')).toContainText('Loaded snapshot');
});

test('unseen and invalid members withhold totals and partial observed totals remain explicitly incomplete', async ({
	page
}) => {
	const third = { ...entry, address: '7'.repeat(51), label: 'Invalid saved format' };
	await seed(page, encode([entry, other, third]));
	let request = 0;
	await page.route('**/v1/addresses/balances', async (route) => {
		const data = groupSnapshot([entry.address, other.address, third.address]);
		data.full_history = false;
		data.complete = false;
		data.partial_from = 500;
		if (++request === 1) {
			data.members[1] = { ...data.members[1], status: 'unseen', balance: null };
			data.members[2] = { ...data.members[2], tree_hash: null, status: 'invalid', balance: null };
			data.resolved_script_count = 1;
			data.observed_totals = null;
		}
		await route.fulfill({ json: data });
	});
	await page.goto('/saved');
	const dashboard = page.getByRole('region', { name: 'Group balances', exact: true });
	await dashboard.getByRole('button', { name: 'Load group balances', exact: true }).click();
	await expect(dashboard.getByText('Combined balance unavailable', { exact: true })).toBeVisible();
	await expect(dashboard.getByText('Unseen in this index', { exact: true })).toBeVisible();
	await expect(dashboard.getByText('Invalid address', { exact: true })).toBeVisible();
	await expect(dashboard.locator('.erg-total')).toHaveCount(0);
	await expect(dashboard.locator('.coverage')).toContainText('unresolved pre-index spends');
	await dashboard.getByRole('button', { name: 'Refresh group balances', exact: true }).click();
	await expect(dashboard.getByText('Observed balance only', { exact: true })).toBeVisible();
	await expect(dashboard.getByText('Complete indexed balance', { exact: true })).toHaveCount(0);
});

test('an in-flight group response cannot restore totals after membership changes', async ({
	page
}) => {
	await seed(page, encode([entry, other]));
	let release!: () => void;
	const held = new Promise<void>((resolve) => (release = resolve));
	let requested!: () => void;
	const started = new Promise<void>((resolve) => (requested = resolve));
	await page.route('**/v1/addresses/balances', async (route) => {
		requested();
		await held;
		await route.fulfill({ json: groupSnapshot([entry.address, other.address]) }).catch(() => {});
	});
	await page.goto('/saved');
	const dashboard = page.getByRole('region', { name: 'Group balances', exact: true });
	await dashboard.getByRole('button', { name: 'Load group balances', exact: true }).click();
	await started;
	await page.getByLabel('Filter saved addresses', { exact: true }).fill('Alpha');
	await expect(
		dashboard.getByRole('button', { name: 'Load group balances', exact: true })
	).toBeEnabled();
	release();
	await expect(dashboard.locator('.snapshot')).toHaveCount(0);
	await expect(page.locator('.saved-entry')).toHaveCount(1);
});

test('an older server shows an unavailable state without treating it as an empty balance', async ({
	page
}) => {
	await seed(page);
	await page.route('**/v1/addresses/balances', (route) => route.fulfill({ status: 404, json: {} }));
	await page.goto('/saved');
	const dashboard = page.getByRole('region', { name: 'Group balances', exact: true });
	await dashboard.getByRole('button', { name: 'Load group balances', exact: true }).click();
	await expect(dashboard.getByRole('alert')).toContainText('not available on this server yet');
	await expect(dashboard.locator('.erg-total')).toHaveCount(0);
});

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
			await page.route('**/v1/addresses/balances', (route) =>
				route.fulfill({ json: groupSnapshot([entry.address]) })
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
			await page.getByRole('button', { name: 'Load group balances', exact: true }).click();
			await expect(page.getByText('Complete indexed balance', { exact: true })).toBeVisible();
			await page.getByText('Inspect 1 token balances', { exact: true }).click();
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
			).toBe(true);
		});
