import { expect, test, type Page } from '@playwright/test';
import type { RentSchedulePageDto } from '../../src/lib/api/types.ts';
import { formatErg, formatTokenAmount } from '../../src/lib/format/amount.ts';
import { eligibleCount } from './data.ts';
import { RENT_CALENDAR_LARGE_AMOUNT, RENT_CALENDAR_TOKEN } from './mock/rent-schedule.ts';
import { scrollUntilLoaded, useShortViewport } from './helpers.ts';

const routePattern = '**/v1/rent/schedule?*';
const view = (page: Page) => page.getByRole('region', { name: 'Upcoming rent calendar' });
async function openSchedule(page: Page, url = '/rent'): Promise<RentSchedulePageDto> {
	const response = page.waitForResponse(
		(res) => res.url().includes('/v1/rent/schedule?') && res.ok()
	);
	await page.goto(url);
	const result = (await (await response).json()) as RentSchedulePageDto;
	await expect(view(page).locator('.rent-record').first()).toBeVisible();
	return result;
}

test('/rent shows earliest records and a larger independent hourly peak beyond page1', async ({
	page
}) => {
	const result = await openSchedule(page);
	await expect(page.getByRole('heading', { name: 'Rent', exact: true })).toBeVisible();
	await expect(page.getByRole('tab', { name: 'Upcoming' })).toHaveAttribute(
		'aria-selected',
		'true'
	);
	await expect(view(page).locator('.rent-record')).toHaveCount(result.items.length);
	const peak = [...result.batches!].sort((a, b) =>
		BigInt(a.collectible_due_nano) > BigInt(b.collectible_due_nano) ? -1 : 1
	)[0];
	const loadedDue = result.items.reduce((sum, item) => sum + BigInt(item.collectible_due_nano), 0n);
	expect(BigInt(peak.collectible_due_nano)).toBeGreaterThan(loadedDue);
	await expect(view(page).locator('.batch').first()).toContainText(
		formatErg(peak.collectible_due_nano, { maxFrac: 9 })
	);
	await expect(view(page).getByText(/Independent of the 5 loaded records/)).toBeVisible();
	await expect(view(page).getByText(/Partial index/)).toBeVisible();
	await expect(view(page).getByRole('heading', { name: 'Largest observed hours' })).toBeVisible();
	const overviewAmounts = await view(page)
		.getByLabel('Window overview')
		.locator('strong')
		.allTextContents();
	const continuation = page.waitForRequest(
		(request) =>
			request.url().includes('/v1/rent/schedule?') &&
			new URL(request.url()).searchParams.has('snapshot')
	);
	await view(page).getByRole('button', { name: 'Load next boxes' }).click();
	const request = await continuation;
	expect(new URL(request.url()).searchParams.get('snapshot')).toBe(result.next_snapshot);
	await expect(view(page).locator('.rent-record')).toHaveCount(10);
	await expect(view(page).getByLabel('Window overview').locator('strong')).toHaveText(
		overviewAmounts
	);
	const heights = await view(page).locator('.record-time > span').allTextContents();
	expect(heights).toEqual([...heights].sort());
});

test('/rent eligible tab loads claimable boxes and survives reload', async ({ page }) => {
	await useShortViewport(page);
	await page.goto('/rent');
	await page.getByRole('tab', { name: 'Eligible' }).click();
	await expect(page).toHaveURL('/rent#eligible');
	await scrollUntilLoaded(page, eligibleCount);
	await expect(page.locator('table.table tbody tr').first().getByText('claimable')).toBeVisible();
	await expect(page.getByText(`${eligibleCount} boxes, total due`)).toBeVisible();
	await page.reload();
	await expect(page.getByRole('tab', { name: 'Eligible' })).toHaveAttribute(
		'aria-selected',
		'true'
	);
	await expect(page.locator('table.table tbody tr').first()).toBeVisible();
});

test('all horizons persist in URL and daily overview exposes hourly evidence', async ({ page }) => {
	await openSchedule(page);
	for (const window of ['7d', '30d', '90d']) {
		const request = page.waitForResponse(
			(res) =>
				res.url().includes('/v1/rent/schedule?') &&
				new URL(res.url()).searchParams.get('window') === window
		);
		await view(page).getByRole('button', { name: window, exact: true }).click();
		await request;
		await expect(page).toHaveURL(new RegExp(`window=${window}`));
		await expect(view(page).getByRole('button', { name: window, exact: true })).toHaveAttribute(
			'aria-pressed',
			'true'
		);
	}
	await page.reload();
	await expect(view(page).getByRole('button', { name: '90d', exact: true })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await expect(view(page).getByRole('heading', { name: 'Largest observed days' })).toBeVisible();
	const firstBatch = view(page).locator('.batch').first();
	await firstBatch.locator('summary').click();
	await expect(firstBatch.locator('.hour-breakdown li').first()).toContainText('full-claim value');
	await view(page).getByLabel('Rank batches by').selectOption('boxes');
	await expect(view(page).locator('.rent-record')).toHaveCount(5);
});

test('token-ID and full-claim filters preserve exact token quantities through reload', async ({
	page
}) => {
	await openSchedule(page);
	await view(page).getByLabel('Token ID optional').fill(RENT_CALENDAR_TOKEN);
	await view(page)
		.getByRole('combobox', { name: 'Opportunity', exact: true })
		.selectOption('full_claim');
	await view(page).getByRole('button', { name: 'Apply', exact: true }).click();
	await expect(page).toHaveURL(new RegExp(`token_id=${RENT_CALENDAR_TOKEN}`));
	await expect(page).toHaveURL(/mode=full_claim/);
	await expect(
		view(page).locator('.rent-records').getByTitle(`${RENT_CALENDAR_LARGE_AMOUNT} raw units`)
	).toHaveText(formatTokenAmount(RENT_CALENDAR_LARGE_AMOUNT, 2));
	await expect(view(page).locator('.rent-records').getByText('Protocol-constrained')).toHaveCount(
		0
	);
	await view(page).getByLabel('Rank batches by').selectOption('token');
	await page.reload();
	await expect(view(page).getByLabel('Token ID optional')).toHaveValue(RENT_CALENDAR_TOKEN);
	await expect(view(page).getByRole('combobox', { name: 'Opportunity', exact: true })).toHaveValue(
		'full_claim'
	);
	await expect(
		view(page).locator('.rent-records').getByTitle(`${RENT_CALENDAR_LARGE_AMOUNT} raw units`)
	).toBeVisible();
});

test('invalid token ID stays local without fetching a misleading empty schedule', async ({
	page
}) => {
	await openSchedule(page);
	let requests = 0;
	page.on('request', (request) => {
		if (request.url().includes('/v1/rent/schedule?')) requests++;
	});
	await view(page).getByLabel('Token ID optional').fill('not-a-token');
	await view(page).getByRole('button', { name: 'Apply', exact: true }).click();
	await expect(view(page).getByRole('alert')).toContainText('64-character');
	expect(requests).toBe(0);
	await expect(view(page).locator('.rent-record')).toHaveCount(5);
});

test('empty scan-limited pages continue instead of claiming no matching boxes', async ({
	page
}) => {
	await page.route(routePattern, async (route) => {
		const response = await route.fetch();
		const data = (await response.json()) as RentSchedulePageDto;
		if (!new URL(route.request().url()).searchParams.has('cursor')) {
			data.items = [];
			data.schedule_context.scanned = 2000;
			data.schedule_context.scan_limit_reached = true;
		}
		await route.fulfill({ json: data });
	});
	await page.goto('/rent');
	await expect(view(page).getByText(/No matches in this scanned segment/)).toBeVisible();
	await view(page).getByRole('button', { name: 'Continue scanning' }).click();
	await expect(view(page).locator('.rent-record')).toHaveCount(5);
	await expect(view(page).getByLabel('Window overview')).toContainText('Observed rent');
});

test('incomplete batch scan keeps warnings and does not label a complete ranking', async ({
	page
}) => {
	await page.route(routePattern, async (route) => {
		const data = (await (await route.fetch()).json()) as RentSchedulePageDto;
		data.schedule_context.batch_complete = false;
		data.schedule_context.batch_stop_reason = 'deadline';
		await route.fulfill({ json: data });
	});
	await page.goto('/rent');
	await expect(view(page).getByText(/Batch scan incomplete/)).toBeVisible();
	await expect(view(page).getByText(/larger batches may be missing/)).toBeVisible();
	await expect(view(page).getByRole('heading', { name: 'Largest observed hours' })).toBeVisible();
	await expect(view(page).locator('.rent-record').first()).toBeVisible();
});

test('refresh failure marks retained schedule stale and snapshot conflict clears evidence', async ({
	page
}) => {
	await openSchedule(page);
	await page.route(routePattern, (route) =>
		route.fulfill({
			status: 503,
			json: { title: 'Unavailable', detail: 'Fixture source unavailable.' }
		})
	);
	await view(page).getByRole('button', { name: 'Refresh rent schedule' }).click();
	await expect(view(page).getByRole('alert')).toContainText('records and totals are stale');
	await expect(view(page).locator('.rent-record')).toHaveCount(5);
	await page.unroute(routePattern);
	await page.route(routePattern, (route) =>
		route.fulfill({ status: 409, json: { code: 'snapshot_changed', detail: 'Chain changed.' } })
	);
	await view(page).getByRole('button', { name: 'Load next boxes' }).click();
	await expect(view(page).getByRole('alert')).toContainText('indexed chain changed');
	await expect(view(page).locator('.rent-record')).toHaveCount(0);
	await expect(view(page).getByLabel('Window overview')).toHaveCount(0);
});

test('old tip estimates disclose their anchor and future maturity heights are not block links', async ({
	page
}) => {
	await page.route(routePattern, async (route) => {
		const data = (await (await route.fetch()).json()) as RentSchedulePageDto;
		const delta = 7 * 86_400_000;
		data.schedule_context.anchor_timestamp_ms -= delta;
		for (const item of data.items) item.estimated_maturity_ms -= delta;
		for (const batch of data.batches ?? []) batch.estimated_hour_start_ms -= delta;
		await route.fulfill({ json: data });
	});
	const data = await openSchedule(page);
	await expect(view(page).getByText(/The indexed block is more than an hour old/)).toContainText(
		'not from now'
	);
	await expect(view(page).locator('.schedule-context time')).toHaveAttribute(
		'datetime',
		new Date(data.schedule_context.anchor_timestamp_ms).toISOString()
	);
	await expect(view(page).locator('.schedule-context a')).toHaveAttribute(
		'href',
		`/blocks/${data.anchor!.block_id}`
	);
	await expect(view(page).locator('.record-time a')).toHaveCount(0);
});

test('calendar errors do not prevent using Eligible and tab semantics remain intact', async ({
	page
}) => {
	await page.route(routePattern, (route) =>
		route.fulfill({ status: 404, json: { title: 'Not Found', detail: 'Old server.' } })
	);
	await page.goto('/rent');
	await expect(view(page).getByRole('alert')).toContainText(
		'does not provide the rent calendar yet'
	);
	await expect(page.getByRole('tablist')).toHaveAttribute('aria-label', 'Rent sections');
	await expect(page.getByRole('tab', { name: 'Upcoming' })).toHaveAttribute(
		'aria-controls',
		'panel-upcoming'
	);
	await expect(page.getByRole('tabpanel')).toHaveAttribute('tabindex', '0');
	await page.getByRole('tab', { name: 'Eligible' }).click();
	await expect(page.locator('table.table tbody tr').first()).toBeVisible();
});

for (const appearance of ['original', 'prism', 'atelier', 'aurora']) {
	for (const width of [390, 1440]) {
		test(`${appearance} calendar keeps first record prominent at ${width}px Standard`, async ({
			page
		}) => {
			await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
			await page.addInitScript((appearance) => {
				localStorage.setItem('xp-appearance', appearance);
				localStorage.setItem('xp-density', 'standard');
				localStorage.setItem('xp-theme', 'light');
			}, appearance);
			await openSchedule(page);
			await page.evaluate(async () => {
				await document.fonts.ready;
				window.scrollTo(0, 0);
			});
			const record = await view(page).locator('.rent-record').first().boundingBox();
			expect(record).not.toBeNull();
			expect(record!.y).toBeLessThan(650);
			if (width === 390) expect(record!.y + record!.height).toBeLessThanOrEqual(844 - 80);
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
				width
			);
			await page.screenshot({ path: `../artifacts/rent-calendar-${appearance}-${width}.png` });
			if (width === 390) {
				await view(page).getByRole('link', { name: 'Peak hours' }).click();
				await expect(page).toHaveURL(/#rent-calendar-peaks$/);
				const peak = await view(page)
					.getByRole('heading', { name: 'Largest observed hours' })
					.boundingBox();
				expect(peak).not.toBeNull();
				expect(peak!.y).toBeGreaterThanOrEqual(0);
				expect(peak!.y + peak!.height).toBeLessThan(764);
				await page.reload();
				await expect(page.locator('#rent-calendar-peaks')).toBeAttached();
				await expect(page).toHaveURL(/#rent-calendar-peaks$/);
				await expect(view(page).getByRole('link', { name: 'Peak hours' })).toHaveAttribute(
					'href',
					'#rent-calendar-peaks'
				);
			}
		});
	}
	for (const theme of ['light', 'dark']) {
		test(`${appearance} ${theme} calendar has44px primary controls and no overflow at320px`, async ({
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
			await openSchedule(page);
			await view(page).getByRole('button', { name: 'Filter boxes' }).click();
			for (const selector of [
				'.filter-toggle',
				'.windows button',
				'.schedule-filter select',
				'.schedule-filter input',
				'.schedule-filter button'
			]) {
				for (const control of await view(page).locator(selector).all())
					expect((await control.boundingBox())!.height).toBeGreaterThanOrEqual(44);
			}
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
				320
			);
			await expect(view(page).getByText(/Partial index/)).toBeVisible();
		});
	}
}
