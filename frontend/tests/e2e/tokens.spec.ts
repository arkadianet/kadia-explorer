import { expect, test } from '@playwright/test';
import { createHash } from 'node:crypto';
import {
	MOCK_ADDRESS,
	SYNTHETIC_TOKEN,
	UNKNOWN_HEX,
	claimableBox,
	mostHeldToken,
	newestTx,
	newestToken,
	tokenBoxCount,
	tokenCount,
	tokenHolders,
	tokenUnspentBoxCount,
	tx
} from './data.ts';
import { scrollUntilCount, scrollUntilLoaded, useShortViewport } from './helpers.ts';

/** The first row's token link, which is what a change of sort reorders. */
const firstTokenLink = (page: import('@playwright/test').Page) =>
	page.locator('table.table tbody tr').first().locator('a[href^="/token/"]').first();

test('/tokens lists tokens and the sort toggle reorders them', async ({ page }) => {
	await useShortViewport(page);
	await page.goto('/tokens');

	// How many pages the observer pulled in before this ran is not the point — which token
	// heads the list is, and that is what the sort changes.
	await expect(page.locator('table.table tbody tr').first()).toBeVisible();
	await expect(page.getByRole('columnheader', { name: 'Holders' })).toBeVisible();

	// Default sort is newest, which the synthetic token heads: it is minted into the tip block.
	await expect(firstTokenLink(page)).toHaveAttribute('href', `/token/${newestToken.id}`);
	await expect(page.getByRole('button', { name: 'Newest' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);

	// The sort lives in the query string, and swapping it starts a fresh pager rather than
	// appending the other order's rows to the ones on screen.
	await page.getByRole('button', { name: 'Most held' }).click();
	await expect(page).toHaveURL('/tokens?sort=holders');
	await expect(firstTokenLink(page)).toHaveAttribute('href', `/token/${mostHeldToken.id}`);

	// A reload keeps the sort — that is the point of it being in the URL.
	await page.reload();
	await expect(page.getByRole('button', { name: 'Most held' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await expect(firstTokenLink(page)).toHaveAttribute('href', `/token/${mostHeldToken.id}`);
});

test('an unknown sort is normalised to newest rather than sent to the API', async ({ page }) => {
	// The mock answers `sort=bogus` with a 400 problem, as the real endpoint does — but the
	// page never asks it that: `/tokens/+page.ts` narrows the query parameter to
	// 'holders' | 'newest' before the pager is built, so a hand-edited URL falls back to the
	// default list instead of an ErrorState.
	const direct = await page.request.get('/v1/tokens?sort=bogus');
	expect(direct.status()).toBe(400);

	await useShortViewport(page);
	await page.goto('/tokens?sort=bogus');

	await expect(firstTokenLink(page)).toHaveAttribute('href', `/token/${newestToken.id}`);
	await expect(page.getByRole('button', { name: 'Newest' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await expect(page.locator('.error')).toHaveCount(0);
});

test('/tokens pulls in further pages as the sentinel scrolls into view', async ({ page }) => {
	await useShortViewport(page);
	await page.goto('/tokens');
	await scrollUntilLoaded(page, tokenCount);
});

test('a named token shows its facts, with the decimal point its mint declares', async ({
	page
}) => {
	await page.goto(`/token/${SYNTHETIC_TOKEN.id}`);

	await expect(page.getByRole('heading', { name: SYNTHETIC_TOKEN.name })).toBeVisible();
	await expect(page.getByText('Supply', { exact: true })).toBeVisible();
	// 123456 units at 2 decimals.
	await expect(page.getByText('1,234.56').first()).toBeVisible();
	await expect(page.getByText(SYNTHETIC_TOKEN.description)).toBeVisible();
	await expect(page.getByRole('link', { name: `${newestToken.mint_height}` })).toBeVisible();
});

test('a token page lists its holders and, on the other tab, its boxes', async ({ page }) => {
	// Short viewport, again so each list stops after the mock's first page — see above.
	await useShortViewport(page);
	await page.goto(`/token/${mostHeldToken.id}`);

	// Holders is the default tab and loads on mount; the mock pages by 5, so the full set only
	// arrives once the sentinel has been scrolled through.
	await scrollUntilCount(page, 'table.table tbody tr', tokenHolders.length);
	// The biggest holder, with the share the mock computed for it.
	await expect(page.locator('table.table tbody tr').first()).toContainText('99.94%');

	await page.getByRole('tab', { name: 'Boxes' }).click();
	await expect(page).toHaveURL(`/token/${mostHeldToken.id}#boxes`);

	// Unspent first — "who holds it now" is the question the page is asked.
	await expect(page.getByText('Boxes still holding the token')).toBeVisible();
	await scrollUntilCount(page, 'article.box', tokenUnspentBoxCount);

	// "All" is a different query, so it restarts the list rather than appending to it — and it
	// is a strictly larger set, spent boxes included.
	await page.getByRole('button', { name: 'All', exact: true }).click();
	await expect(page.getByText(`${tokenBoxCount.toLocaleString('en-US')} boxes ever`)).toBeVisible();
	await scrollUntilCount(page, 'article.box', tokenBoxCount);
});

test('an id with no mint row renders the token-not-found state', async ({ page }) => {
	await page.goto(`/token/${UNKNOWN_HEX}`);

	await expect(page.getByRole('heading', { name: 'Token' })).toBeVisible();
	await expect(page.getByText(/Token not found/)).toBeVisible();
});

test('a token id resolves through the API to the token page', async ({ page }) => {
	await page.goto('/');
	const input = page.getByRole('searchbox');
	await input.fill(mostHeldToken.id);
	await input.press('Enter');

	await expect(page).toHaveURL(`/token/${mostHeldToken.id}`);
	await expect(page.getByRole('tab', { name: 'Holders' })).toBeVisible();
});

test('an address page names the tokens it holds and scales their amounts', async ({ page }) => {
	await page.goto(`/address/${MOCK_ADDRESS}`);

	const holdings = page
		.locator('section.panel')
		.filter({ has: page.getByRole('heading', { name: /^Tokens \(/ }) });
	const named = holdings.getByRole('link', { name: SYNTHETIC_TOKEN.name });
	await expect(named).toBeVisible();
	await expect(named).toHaveAttribute('href', `/token/${SYNTHETIC_TOKEN.id}`);
	await expect(holdings.getByText('1,234.56', { exact: true })).toBeVisible();
});

test('the home page top-tokens card lists five tokens', async ({ page }) => {
	await page.goto('/');

	await expect(page.getByRole('heading', { name: 'Top tokens' })).toBeVisible();
	await expect(page.locator('.tokens li')).toHaveCount(5);
	// The card renders rows, not the failed-request state.
	await expect(page.locator('.error')).toHaveCount(0);
	await expect(page.getByRole('link', { name: 'All tokens' })).toBeVisible();
});

test('holder concentration uses the holder snapshot supply and exact amounts', async ({ page }) => {
	await page.route(`**/v1/tokens/${mostHeldToken.id}/holders?*`, (route) =>
		route.fulfill({
			json: {
				items: [
					{ ...tokenHolders[0], tree_hash: 'a'.repeat(64), amount: '80', share_pct: '0.01' },
					{ ...tokenHolders[0], tree_hash: 'b'.repeat(64), amount: '20', share_pct: '0.01' }
				],
				next_cursor: null,
				holder_context: {
					supply: '100',
					holder_count: 2,
					definition: 'indexed_emission_minus_burned'
				}
			}
		})
	);
	await page.goto(`/token/${mostHeldToken.id}`);
	const concentration = page.getByRole('region', { name: 'Holder concentration' });
	await expect(
		concentration.locator('dl > div').filter({ hasText: 'Largest script balance' })
	).toContainText('80.00%');
	await expect(
		concentration.locator('dl > div').filter({ hasText: 'Top 2 loaded scripts' })
	).toContainText('100.00%');
	await expect(concentration).toContainText('2 of 2 holder scripts are loaded');
	await expect(concentration).toContainText(
		'Supply means indexed minted amount minus indexed burns'
	);
	await expect(concentration).toContainText('not a full ownership distribution');
});

test('concentration stays unavailable without a matching holder snapshot denominator', async ({
	page
}) => {
	await page.route(`**/v1/tokens/${mostHeldToken.id}/holders?*`, (route) =>
		route.fulfill({
			json: {
				items: [tokenHolders[0]],
				next_cursor: null
			}
		})
	);
	await page.goto(`/token/${mostHeldToken.id}`);
	const concentration = page.getByRole('region', { name: 'Holder concentration' });
	await expect(concentration).toContainText('A matching supply denominator is required');
	await expect(concentration.getByText('Largest script balance')).toHaveCount(0);
	await expect(page.locator('table.table tbody tr')).toHaveCount(1);
});

const mediaUri = 'https://media.example.test/token.png';
const mediaPng = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=',
	'base64'
);
function mediaRegister(text: string) {
	const bytes = Buffer.from(text, 'utf8');
	if (bytes.length >= 128) throw new Error('Test fixture needs a short register');
	return '0e' + bytes.length.toString(16).padStart(2, '0') + bytes.toString('hex');
}
async function mockMediaMint(
	page: import('@playwright/test').Page,
	uri: string,
	hash: string,
	mint = newestToken
) {
	await page.route(`**/v1/tokens/${mint.id}`, (route) =>
		route.fulfill({
			json: {
				...mint,
				kind: 'nft-picture',
				token_type: '0101'
			}
		})
	);
	await page.route(`**/v1/boxes/${mint.mint_box}`, (route) =>
		route.fulfill({
			json: {
				...claimableBox,
				id: mint.mint_box,
				tx_id: mint.mint_tx,
				tokens: [{ id: mint.id, amount: '1' }],
				registers: { R9: mediaRegister(uri), R8: '0e20' + hash }
			}
		})
	);
}

test('NFT preview requires consent, omits cookies and referrer, and checks the downloaded hash', async ({
	page,
	context
}) => {
	await mockMediaMint(page, mediaUri, createHash('sha256').update(mediaPng).digest('hex'));
	await context.addCookies([
		{
			name: 'private-session',
			value: 'not-for-media',
			url: mediaUri,
			sameSite: 'None',
			secure: true
		}
	]);
	const requests: Record<string, string>[] = [];
	await page.route(mediaUri, async (route) => {
		requests.push(await route.request().allHeaders());
		await route.fulfill({
			status: 200,
			contentType: 'image/png',
			headers: { 'access-control-allow-origin': '*' },
			body: mediaPng
		});
	});
	await page.goto(`/token/${newestToken.id}`);
	const media = page.getByRole('region', { name: 'Token media' });
	await expect(media.getByRole('button', { name: 'Load external media' })).toBeVisible();
	expect(requests).toHaveLength(0);
	await expect(media.getByRole('img')).toHaveCount(0);
	await media.getByRole('button', { name: 'Load external media' }).click();
	await expect(media.getByRole('img')).toBeVisible();
	await expect(media).toContainText('Downloaded bytes match the SHA-256 recorded at mint');
	expect(requests).toHaveLength(1);
	expect(requests[0].cookie).toBeUndefined();
	expect(requests[0].referer).toBeUndefined();
	await media.getByRole('button', { name: 'Unload media' }).click();
	await expect(media.getByRole('img')).toHaveCount(0);
	await expect(media.getByRole('button', { name: 'Load external media' })).toBeVisible();
});

test('an NFT hash mismatch withholds the preview and offers a retry', async ({ page }) => {
	await mockMediaMint(page, mediaUri, '00'.repeat(32));
	await page.route(mediaUri, (route) =>
		route.fulfill({
			status: 200,
			contentType: 'image/png',
			headers: { 'access-control-allow-origin': '*' },
			body: mediaPng
		})
	);
	await page.goto(`/token/${newestToken.id}`);
	const media = page.getByRole('region', { name: 'Token media' });
	await media.getByRole('button', { name: 'Load external media' }).click();
	await expect(media).toContainText('do not match the declared SHA-256. Preview withheld');
	await expect(media.getByRole('img')).toHaveCount(0);
	await expect(media.getByRole('button', { name: 'Retry external media' })).toBeVisible();
});

test('client navigation between NFT pages unloads the previous preview and requires new consent', async ({
	page
}) => {
	const hash = createHash('sha256').update(mediaPng).digest('hex');
	const otherUri = 'https://media.example.test/other.png';
	await mockMediaMint(page, mediaUri, hash);
	await mockMediaMint(page, otherUri, hash, mostHeldToken);
	await page.route(mediaUri, (route) =>
		route.fulfill({
			status: 200,
			contentType: 'image/png',
			headers: { 'access-control-allow-origin': '*' },
			body: mediaPng
		})
	);
	let otherRequests = 0;
	await page.route(otherUri, (route) => {
		otherRequests++;
		return route.fulfill({
			status: 200,
			contentType: 'image/png',
			headers: { 'access-control-allow-origin': '*' },
			body: mediaPng
		});
	});
	await page.goto(`/token/${newestToken.id}`);
	const media = page.getByRole('region', { name: 'Token media' });
	await media.getByRole('button', { name: 'Load external media' }).click();
	await expect(media.getByRole('img')).toBeVisible();
	await page.getByRole('searchbox').fill(mostHeldToken.id);
	await page.getByRole('searchbox').press('Enter');
	await expect(page).toHaveURL(`/token/${mostHeldToken.id}`);
	await expect(media.getByRole('link', { name: 'Open declared media' })).toHaveAttribute(
		'href',
		otherUri
	);
	await expect(media.getByRole('img')).toHaveCount(0);
	await expect(media.getByRole('button', { name: 'Load external media' })).toBeVisible();
	expect(otherRequests).toBe(0);
});

test('unsupported NFT media stays readable without creating an active source link', async ({
	page
}) => {
	await mockMediaMint(page, 'javascript:alert(1)', '00'.repeat(32));
	await page.goto(`/token/${newestToken.id}`);
	const media = page.getByRole('region', { name: 'Token media' });
	await expect(media).toContainText('This media URI cannot be previewed');
	await expect(media.getByRole('button', { name: 'Load external media' })).toHaveCount(0);
	await expect(media.getByRole('link')).toHaveCount(0);
	await media.getByText('Declared URI', { exact: true }).click();
	await expect(media.getByText('javascript:alert(1)', { exact: true })).toBeVisible();
});

function historySummary(value: typeof tx) {
	return {
		id: value.id,
		height: value.height,
		index: value.index,
		timestamp: value.timestamp,
		size: value.size,
		fee: value.fee,
		input_count: value.inputs.length,
		data_input_count: value.data_inputs.length,
		output_count: value.outputs.length
	};
}
function historyPage(items = [historySummary(tx)], extra: Record<string, unknown> = {}) {
	return {
		items,
		next_cursor: null,
		next_snapshot: null,
		consistency: 'strict',
		anchor: { height: newestTx.height, block_id: 'a'.repeat(64) },
		history_context: { scope: 'indexed_token_touches', partial_from: null },
		...extra
	};
}

test('token transactions load on demand with bound cursor and snapshot and no detail fan-out', async ({
	page
}) => {
	const queries: URLSearchParams[] = [];
	let detailRequests = 0;
	page.on('request', (request) => {
		if (/\/v1\/txs\/[a-f0-9]{64}(?:\?|$)/.test(request.url())) detailRequests++;
	});
	await page.route(`**/v1/tokens/${mostHeldToken.id}/txs?*`, (route) => {
		const query = new URL(route.request().url()).searchParams;
		queries.push(query);
		return route.fulfill({
			json: query.has('cursor')
				? historyPage([historySummary(tx)])
				: historyPage([historySummary(newestTx)], {
						next_cursor: 'cursor-1',
						next_snapshot: 'snapshot-1'
					})
		});
	});
	await page.goto(`/token/${mostHeldToken.id}`);
	await expect(page.getByRole('tab', { name: 'Transactions', exact: true })).toBeVisible();
	expect(queries).toHaveLength(0);
	await page.getByRole('tab', { name: 'Transactions', exact: true }).click();
	await expect(page).toHaveURL(`/token/${mostHeldToken.id}#transactions`);
	const history = page.getByRole('region', { name: 'Token transactions' });
	await expect(
		history.getByRole('link', { name: new RegExp(newestTx.id.slice(0, 8)) })
	).toBeVisible();
	await scrollUntilCount(page, '.token-history tbody tr', 2);
	expect(queries).toHaveLength(2);
	expect(queries[0].get('consistency')).toBe('strict');
	expect(queries[0].get('dir')).toBe('desc');
	expect(queries[1].get('cursor')).toBe('cursor-1');
	expect(queries[1].get('snapshot')).toBe('snapshot-1');
	expect(detailRequests).toBe(0);
	await expect(history).toContainText('does not identify payments or a transfer amount');
	await expect(history).toContainText('Each transaction appears once');
	await page.reload();
	await expect(page.getByRole('tab', { name: 'Transactions', exact: true })).toHaveAttribute(
		'aria-selected',
		'true'
	);
});

test('preparing token history is retryable and never looks like an empty history', async ({
	page
}) => {
	let preparing = true;
	await page.route(`**/v1/tokens/${mostHeldToken.id}/txs?*`, (route) =>
		route.fulfill(
			preparing
				? {
						status: 503,
						json: { code: 'token_history_preparing', title: 'Preparing', detail: 'Preparing index' }
					}
				: { json: historyPage() }
		)
	);
	await page.goto(`/token/${mostHeldToken.id}#transactions`);
	const history = page.getByRole('region', { name: 'Token transactions' });
	await expect(history).toContainText('Token history is preparing');
	await expect(history.getByText(/No transaction touching/)).toHaveCount(0);
	preparing = false;
	await history.getByRole('button', { name: 'Retry', exact: true }).click();
	await expect(history.locator('tbody tr')).toHaveCount(1);
	await expect(history.getByRole('alert')).toHaveCount(0);
});

test('token history drops a conflicting snapshot and requires an explicit restart', async ({
	page
}) => {
	let restarting = false;
	await page.route(`**/v1/tokens/${mostHeldToken.id}/txs?*`, (route) => {
		const query = new URL(route.request().url()).searchParams;
		if (query.has('cursor'))
			return route.fulfill({
				status: 409,
				json: { title: 'Snapshot changed', detail: 'Fork changed' }
			});
		return route.fulfill({
			json: restarting
				? historyPage([historySummary(newestTx)])
				: historyPage([historySummary(tx)], {
						next_cursor: 'cursor-1',
						next_snapshot: 'snapshot-1'
					})
		});
	});
	await page.goto(`/token/${mostHeldToken.id}#transactions`);
	const history = page.getByRole('region', { name: 'Token transactions' });
	await history.scrollIntoViewIfNeeded();
	await expect(history.getByRole('button', { name: 'Restart', exact: true })).toBeVisible();
	await expect(history.locator('tbody tr')).toHaveCount(0);
	await expect(history.getByText(/Snapshot at indexed block/)).toHaveCount(0);
	restarting = true;
	await history.getByRole('button', { name: 'Restart', exact: true }).click();
	await expect(history.locator('tbody tr')).toHaveCount(1);
	await expect(history.locator(`a[href="/tx/${newestTx.id}"]`)).toBeVisible();
});

test('partial token history discloses unresolved-input gaps and an older server stays unavailable', async ({
	page
}) => {
	let supported = true;
	await page.route(`**/v1/tokens/${mostHeldToken.id}/txs?*`, (route) =>
		route.fulfill(
			supported
				? {
						json: historyPage([], {
							history_context: { scope: 'indexed_token_touches', partial_from: 100 }
						})
					}
				: { status: 404, json: { title: 'Not found' } }
		)
	);
	await page.goto(`/token/${mostHeldToken.id}#transactions`);
	const history = page.getByRole('region', { name: 'Token transactions' });
	await expect(history).toContainText('Partial index starting at block 100');
	await expect(history).toContainText(
		'only token involvement is in unresolved inputs may be absent'
	);
	supported = false;
	await page.reload();
	await expect(history).toContainText(
		'Token transaction history is not available on this server yet'
	);
	await expect(history.getByText(/No transaction touching/)).toHaveCount(0);
});

const exportHolderRows = Array.from({ length: 20 }, (_, index) => ({
	...tokenHolders[0],
	tree_hash: index.toString(16).padStart(64, '0'),
	amount: index === 0 ? '9007199254740993' : '1',
	share_pct: '0.00'
}));
function exportHolderPage(extra: Record<string, unknown> = {}) {
	return {
		items: exportHolderRows,
		next_cursor: 'cursor-1',
		next_snapshot: 'snapshot-1',
		consistency: 'strict',
		anchor: { height: newestTx.height, block_id: 'b'.repeat(64) },
		holder_context: {
			supply: '18446744073709551615',
			holder_count: 100,
			definition: 'indexed_emission_minus_burned'
		},
		...extra
	};
}

test('loaded holder CSV downloads exact amounts and the same snapshot without fetching remaining holders', async ({
	page
}) => {
	await useShortViewport(page);
	let requests = 0;
	await page.route(`**/v1/tokens/${mostHeldToken.id}/holders?*`, (route) => {
		requests++;
		return route.fulfill({ json: exportHolderPage() });
	});
	await page.goto(`/token/${mostHeldToken.id}`);
	const button = page.getByRole('button', { name: 'Export loaded holders CSV', exact: true });
	await expect(button).toBeEnabled();
	const [download] = await Promise.all([page.waitForEvent('download'), button.click()]);
	const stream = await download.createReadStream();
	let body = '';
	for await (const chunk of stream!) body += chunk.toString();
	const lines = body
		.trim()
		.split('\r\n')
		.map((line) => line.split(',').map((value) => value.slice(1, -1)));
	expect(lines).toHaveLength(21);
	const first = Object.fromEntries(lines[0].map((header, index) => [header, lines[1][index]]));
	expect(first).toMatchObject({
		token_id: mostHeldToken.id,
		ordinal: '1',
		tree_hash: '0'.repeat(64),
		amount_raw: '9007199254740993',
		decimals: mostHeldToken.decimals === null ? '' : String(mostHeldToken.decimals),
		indexed_supply_raw: '18446744073709551615',
		snapshot_height: String(newestTx.height),
		snapshot_block_id: 'b'.repeat(64),
		export_scope: 'loaded_holder_scripts_only',
		loaded_script_count: '20',
		indexed_holder_script_count: '100'
	});
	expect(requests).toBe(1);
	await expect(
		page.getByText('Scripts do not identify individual owners.', { exact: false })
	).toBeVisible();
});

test('holder CSV stays disabled when strict context is absent or changes across loaded pages', async ({
	page
}) => {
	await useShortViewport(page);
	let scenario = 'missing';
	await page.route(`**/v1/tokens/${mostHeldToken.id}/holders?*`, (route) => {
		const continued = new URL(route.request().url()).searchParams.has('cursor');
		return route.fulfill({
			json: exportHolderPage(
				scenario === 'missing'
					? { consistency: 'best_effort', anchor: null, next_cursor: null }
					: scenario === 'invalid'
						? {
								holder_context: {
									supply: '1',
									holder_count: 100,
									definition: 'indexed_emission_minus_burned'
								},
								next_cursor: null
							}
						: continued
							? {
									items: [{ ...exportHolderRows[1], tree_hash: 'c'.repeat(64) }],
									next_cursor: null,
									...(scenario === 'changed-anchor'
										? { anchor: { height: newestTx.height, block_id: 'c'.repeat(64) } }
										: {
												holder_context: {
													supply: '18446744073709551614',
													holder_count: 100,
													definition: 'indexed_emission_minus_burned'
												}
											})
								}
							: {}
			)
		});
	});
	await page.goto(`/token/${mostHeldToken.id}`);
	const button = page.getByRole('button', { name: 'Export loaded holders CSV', exact: true });
	await expect(page.locator('table.table tbody tr')).toHaveCount(20);
	await expect(button).toBeDisabled();
	scenario = 'invalid';
	await page.reload();
	await expect(page.locator('table.table tbody tr')).toHaveCount(20);
	await expect(button).toBeDisabled();
	for (scenario of ['changed-anchor', 'changed-supply']) {
		// The previous scenario intentionally reached the pagination sentinel. Do not
		// restore that scroll position and fetch page two before observing page one.
		await page.evaluate(() => window.scrollTo(0, 0));
		await page.reload();
		await expect(page.locator('table.table tbody tr')).toHaveCount(20);
		await expect(button).toBeEnabled();
		await scrollUntilCount(page, 'table.table tbody tr', 21);
		await expect(button).toBeDisabled();
		await expect(
			page.getByText(
				'A consistent holder snapshot and supply denominator are required for export.',
				{ exact: true }
			)
		).toBeVisible();
	}
});

test('holder CSV cannot download during continuation loading or after a strict snapshot conflict', async ({
	page
}) => {
	await useShortViewport(page);
	let restarted = false;
	let release!: () => void;
	const wait = new Promise<void>((resolve) => {
		release = resolve;
	});
	let started!: () => void;
	const requested = new Promise<void>((resolve) => {
		started = resolve;
	});
	await page.route(`**/v1/tokens/${mostHeldToken.id}/holders?*`, async (route) => {
		if (!new URL(route.request().url()).searchParams.has('cursor'))
			return route.fulfill({
				json: exportHolderPage(
					restarted
						? {
								items: [{ ...exportHolderRows[1], tree_hash: 'c'.repeat(64) }],
								next_cursor: null,
								next_snapshot: null,
								anchor: { height: newestTx.height, block_id: 'c'.repeat(64) }
							}
						: {}
				)
			});
		started();
		await wait;
		return route.fulfill({
			status: 409,
			json: { title: 'Snapshot changed', detail: 'Restart required' }
		});
	});
	await page.goto(`/token/${mostHeldToken.id}`);
	const button = page.getByRole('button', { name: 'Export loaded holders CSV', exact: true });
	await expect(button).toBeEnabled();
	await page.locator('.sentinel').scrollIntoViewIfNeeded();
	await requested;
	await expect(button).toBeDisabled();
	release();
	await expect(page.getByRole('button', { name: 'Restart', exact: true })).toBeVisible();
	await expect(button).toBeDisabled();
	await expect(page.locator('table.table tbody tr')).toHaveCount(0);
	restarted = true;
	await page.getByRole('button', { name: 'Restart', exact: true }).click();
	await expect(page.locator('table.table tbody tr')).toHaveCount(1);
	await expect(button).toBeEnabled();
});
