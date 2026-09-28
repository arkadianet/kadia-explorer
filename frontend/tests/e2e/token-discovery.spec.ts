import { expect, test, type Page } from '@playwright/test';
import type { TokenInfoDto, TokenSearchDto } from '../../src/lib/api/types.ts';
import { newestToken } from './data.ts';

const first: TokenInfoDto = {
	...newestToken,
	id: 'a'.repeat(64),
	name: 'Sigma Test',
	mint_tx: 'c'.repeat(64),
	mint_height: 100,
	holder_count: 42
};
const second: TokenInfoDto = {
	...first,
	id: 'b'.repeat(64),
	mint_tx: 'd'.repeat(64),
	mint_height: 200,
	holder_count: 3
};
function response(items = [first, second], over: Partial<TokenSearchDto> = {}): TokenSearchDto {
	return {
		items,
		next_cursor: null,
		search: {
			query: 'Sigma',
			normalized_query: 'sigma',
			match: 'prefix',
			index_version: 1,
			coverage: 'complete',
			partial_from: null,
			indexed_names: 2,
			total_tokens: 2,
			unindexed_tokens: 0
		},
		...over
	};
}
async function mock(page: Page, value = response()) {
	await page.route('**/v1/tokens/search?*', (route) => route.fulfill({ json: value }));
}

test('ordinary names produce distinct identities without redirecting a duplicate name', async ({
	page
}) => {
	await mock(page);
	await page.goto('/search?q=Sigma');
	await expect(page).toHaveURL('/search?q=Sigma');
	await expect(page.getByRole('article', { name: `Token ${first.id}` })).toBeVisible();
	await expect(page.getByRole('article', { name: `Token ${second.id}` })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Inspect identity', exact: true })).toHaveCount(2);
	await expect(
		page.getByText('Matching names, holder counts and age do not verify a token.', { exact: false })
	).toBeVisible();
});

test('the explicit token-name form keeps numeric and hash-shaped names in token discovery', async ({
	page
}) => {
	const calls: string[] = [];
	await page.route('**/v1/tokens/search?*', (route) => {
		calls.push(new URL(route.request().url()).searchParams.get('q')!);
		return route.fulfill({ json: response() });
	});
	for (const query of ['123', 'e'.repeat(64)]) {
		await page.goto('/search?q=Sigma');
		await page.getByRole('textbox', { name: 'Token name', exact: true }).fill(query);
		await page.getByRole('button', { name: 'Find tokens', exact: true }).click();
		await expect(page).toHaveURL(`/tokens?q=${query}&match=prefix`);
		await expect(page.getByRole('article', { name: `Token ${first.id}` })).toBeVisible();
		expect(calls.at(-1)).toBe(query);
	}
});

test('global and explicit name searches preserve non-ASCII spacing and case', async ({ page }) => {
	const calls: string[] = [];
	await page.route('**/v1/tokens/search?*', (route) => {
		calls.push(new URL(route.request().url()).searchParams.get('q')!);
		return route.fulfill({ json: response() });
	});
	await page.goto('/');
	const query = '\u00a0Σigma\u00a0';
	await page.getByRole('searchbox', { name: 'Search', exact: true }).fill(query);
	await page.getByRole('searchbox', { name: 'Search', exact: true }).press('Enter');
	await expect(page.getByRole('textbox', { name: 'Token name', exact: true })).toHaveValue(query);
	await expect(page.getByRole('article', { name: `Token ${first.id}` })).toBeVisible();
	expect(calls.at(-1)).toBe(query);
	await page.getByRole('combobox', { name: 'Match', exact: true }).selectOption('exact');
	await page.getByRole('button', { name: 'Find tokens', exact: true }).click();
	await expect(page).toHaveURL(`/tokens?q=${encodeURIComponent(query)}&match=exact`);
	await expect(page.getByRole('article', { name: `Token ${first.id}` })).toBeVisible();
	expect(calls.at(-1)).toBe(query);
});

test('the name limit counts UTF-8 bytes before submitting', async ({ page }) => {
	await page.goto('/tokens');
	await page.getByRole('textbox', { name: 'Token name', exact: true }).fill('名'.repeat(200));
	await page.getByRole('button', { name: 'Find tokens', exact: true }).click();
	await expect(page.getByRole('alert')).toHaveText(
		'Use a shorter name or prefix (up to 512 UTF-8 bytes).'
	);
	await expect(page).toHaveURL('/tokens');
});

test('Inspect opens an accessible native dialog, traps focus both ways and restores its opener', async ({
	page
}) => {
	await page.setViewportSize({ width: 1265, height: 714 });
	await mock(page);
	await page.goto('/tokens?q=Sigma');
	const opener = page.getByRole('button', { name: 'Inspect identity', exact: true }).first();
	await opener.focus();
	await page.keyboard.press('Enter');
	const dialog = page.getByRole('dialog', { name: 'Token identity', exact: true });
	await expect(dialog).toBeVisible();
	await expect(dialog.getByRole('heading', { name: 'Token identity', exact: true })).toBeFocused();
	await expect(dialog.getByText(first.id, { exact: true })).toBeVisible();
	const openLink = dialog.getByRole('link', { name: 'Open token explorer' });
	const dialogBounds = await dialog.boundingBox();
	const linkBounds = await openLink.boundingBox();
	expect(linkBounds!.y + linkBounds!.height).toBeLessThanOrEqual(
		dialogBounds!.y + dialogBounds!.height
	);
	await page.keyboard.press('Tab');
	await expect(dialog.getByRole('button', { name: 'Close' })).toBeFocused();
	await page.keyboard.press('Shift+Tab');
	await expect(dialog.getByRole('link', { name: 'Open token explorer' })).toBeFocused();
	await page.keyboard.press('Tab');
	await expect(dialog.getByRole('button', { name: 'Close' })).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(dialog).toBeHidden();
	await expect(opener).toBeFocused();
	await opener.click();
	await dialog.getByRole('button', { name: 'Close' }).click();
	await expect(opener).toBeFocused();
	const other = page.getByRole('button', { name: 'Inspect identity', exact: true }).nth(1);
	await other.click();
	await expect(dialog.getByText(second.id, { exact: true })).toBeVisible();
	await expect(dialog.getByText(first.id, { exact: true })).toHaveCount(0);
});

test('Compare shows two deliberately chosen IDs and their separate mint provenance', async ({
	page
}) => {
	await mock(page);
	await page.goto('/tokens?q=Sigma');
	const opener = page.getByRole('button', { name: 'Compare', exact: true }).first();
	await opener.click();
	const dialog = page.getByRole('dialog', { name: 'Compare token identities' });
	await expect(dialog.getByRole('combobox')).toHaveValue('');
	await dialog.getByRole('combobox').selectOption(second.id);
	await expect(dialog.getByText(first.id, { exact: true })).toBeVisible();
	await expect(dialog.getByText(second.id, { exact: true })).toBeVisible();
	await expect(dialog.locator(`a[href="/tx/${first.mint_tx}"]`)).toBeVisible();
	await expect(dialog.locator(`a[href="/tx/${second.mint_tx}"]`)).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(opener).toBeFocused();
	await expect(page.locator('.token-result.current, .token-result.selected')).toHaveCount(0);
});

test('exact mode and pagination carry the query and strict snapshot', async ({ page }) => {
	const calls: URL[] = [];
	await page.route('**/v1/tokens/search?*', (route) => {
		const url = new URL(route.request().url());
		calls.push(url);
		return route.fulfill({
			json: url.searchParams.has('cursor')
				? response([second])
				: response([first], { next_cursor: 'next-token', next_snapshot: 'snapshot-one' })
		});
	});
	await page.goto('/tokens?q=Sigma&match=exact');
	await expect(page.getByRole('combobox', { name: 'Match', exact: true })).toHaveValue('exact');
	await page.getByRole('button', { name: 'Load more tokens' }).click();
	await expect(page.getByRole('article', { name: `Token ${second.id}` })).toBeVisible();
	expect(Object.fromEntries(calls.at(-1)!.searchParams)).toEqual({
		q: 'Sigma',
		match: 'exact',
		cursor: 'next-token',
		snapshot: 'snapshot-one',
		limit: '20',
		consistency: 'strict'
	});
});

test('a changed chain drops previous results and comparison details until explicit restart', async ({
	page
}) => {
	let restarted = false;
	await page.route('**/v1/tokens/search?*', (route) => {
		if (new URL(route.request().url()).searchParams.has('cursor'))
			return route.fulfill({ status: 409, json: { detail: 'Changed' } });
		return route.fulfill({
			json: restarted
				? response([second])
				: response([first], { next_cursor: 'next', next_snapshot: 'anchor' })
		});
	});
	await page.goto('/tokens?q=Sigma');
	await page.getByRole('button', { name: 'Load more tokens' }).click();
	await expect(page.getByRole('heading', { name: 'The chain changed' })).toBeVisible();
	await expect(page.getByRole('article', { name: `Token ${first.id}` })).toHaveCount(0);
	restarted = true;
	await page.getByRole('button', { name: 'Restart search' }).click();
	await expect(page.getByRole('article', { name: `Token ${second.id}` })).toBeVisible();
});

for (const scenario of [
	{ status: 503, code: 'token_search_preparing', title: 'Token search is preparing' },
	{ status: 404, code: undefined, title: 'Name search is unavailable on this server' },
	{ status: 503, code: undefined, title: 'Token search is unavailable' }
]) {
	test(`${scenario.title} is not displayed as an empty result`, async ({ page }) => {
		await page.route('**/v1/tokens/search?*', (route) =>
			route.fulfill({
				status: scenario.status,
				json: { code: scenario.code, detail: 'Unavailable' }
			})
		);
		await page.goto('/tokens?q=Sigma');
		await expect(page.getByRole('heading', { name: scenario.title, exact: true })).toBeVisible();
		await expect(page.getByRole('heading', { name: 'No indexed name matches' })).toHaveCount(0);
	});
}

test('coverage distinguishes unavailable names from missing early history', async ({ page }) => {
	const data = response();
	data.search.coverage = 'partial';
	data.search.unindexed_tokens = 5;
	await mock(page, data);
	await page.goto('/tokens?q=Sigma');
	await expect(
		page.getByText('5 token names could not be indexed.', { exact: false })
	).toBeVisible();
	await expect(page.getByText('earlier mints may be missing.', { exact: false })).toHaveCount(0);
});

test('out-of-order query responses do not replace the current token results', async ({ page }) => {
	let release!: () => void;
	const delay = new Promise<void>((resolve) => {
		release = resolve;
	});
	await page.route('**/v1/tokens/search?*', async (route) => {
		if (new URL(route.request().url()).searchParams.get('q') === 'Old') {
			await delay;
			return route.fulfill({ json: response([first]) });
		}
		return route.fulfill({ json: response([second]) });
	});
	await page.goto('/tokens?q=Old');
	await expect(page.getByText('Searching indexed token names…')).toBeVisible();
	await page.getByRole('textbox', { name: 'Token name', exact: true }).fill('New');
	await page.getByRole('button', { name: 'Find tokens', exact: true }).click();
	await expect(page.getByRole('article', { name: `Token ${second.id}` })).toBeVisible();
	release();
	await expect(page.getByRole('article', { name: `Token ${first.id}` })).toHaveCount(0);
});

for (const theme of ['light', 'dark']) {
	test(`token identities, dialog and comparisons fit 320px in ${theme} with reduced motion`, async ({
		page
	}) => {
		await page.setViewportSize({ width: 320, height: 900 });
		await page.emulateMedia({ reducedMotion: 'reduce' });
		await page.addInitScript((theme) => localStorage.setItem('xp-theme', theme), theme);
		await mock(page, response([{ ...first, name: 'שלום 🪙 ' + 'اسم'.repeat(30) }, second]));
		await page.goto('/tokens?q=Sigma');
		const matchBounds = await page
			.getByRole('combobox', { name: 'Match', exact: true })
			.boundingBox();
		expect(matchBounds!.width).toBeGreaterThan(150);
		await page.getByRole('button', { name: 'Compare', exact: true }).first().click();
		const dialog = page.getByRole('dialog');
		await dialog.getByRole('combobox').selectOption(second.id);
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(320);
		expect(await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
		await expect(dialog.getByRole('button', { name: 'Close' })).toBeVisible();
		await expect(dialog.locator('h3 bdi').first()).toHaveAttribute('dir', 'auto');
	});
}
