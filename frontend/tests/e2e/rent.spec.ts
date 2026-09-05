import { expect, test } from '@playwright/test';
import { eligibleCount, upcomingCount } from './data.ts';
import { scrollUntilLoaded, useShortViewport } from './helpers.ts';

test('/rent upcoming tab lists boxes with their maturity and total due', async ({ page }) => {
	await page.goto('/rent');

	await expect(page.getByRole('heading', { name: 'Rent' })).toBeVisible();
	await expect(page.getByRole('tab', { name: 'Upcoming' })).toHaveAttribute(
		'aria-selected',
		'true'
	);
	await expect(page.getByText(`${upcomingCount} boxes, total due`)).toBeVisible();
	await expect(page.locator('table.table tbody tr')).toHaveCount(upcomingCount);
	await expect(page.getByRole('columnheader', { name: 'Matures at' })).toBeVisible();
});

test('/rent eligible tab loads claimable boxes', async ({ page }) => {
	await useShortViewport(page);
	await page.goto('/rent');
	await page.getByRole('tab', { name: 'Eligible' }).click();
	await expect(page).toHaveURL('/rent#eligible');

	const rows = page.locator('table.table tbody tr');
	// Paged 5 at a time by the mock; the sentinel pulls the rest in.
	await scrollUntilLoaded(page, eligibleCount);
	await expect(rows.first().getByText('claimable')).toBeVisible();
	await expect(page.getByText(`${eligibleCount} boxes, total due`)).toBeVisible();
});

test('the eligible tab survives a reload', async ({ page }) => {
	await page.goto('/rent#eligible');
	await expect(page.getByRole('tab', { name: 'Eligible' })).toHaveAttribute(
		'aria-selected',
		'true'
	);
	await expect(page.locator('table.table tbody tr').first()).toBeVisible();
});

test('the N-selector re-requests the upcoming window with the chosen horizon', async ({ page }) => {
	await page.goto('/rent');
	await expect(page.getByText(`${upcomingCount} boxes, total due`)).toBeVisible();

	const request = page.waitForRequest(
		(r) =>
			r.url().includes('/v1/rent/upcoming') && new URL(r.url()).searchParams.get('blocks') === '72'
	);
	await page.getByLabel('Horizon').selectOption('72');
	await request;

	await expect(page.getByLabel('Horizon')).toHaveValue('72');
	await expect(page.locator('table.table tbody tr').first()).toBeVisible();
});

test('the tablist and its panel are wired together for assistive tech', async ({ page }) => {
	await page.goto('/rent');
	await expect(page.getByRole('tablist')).toHaveAttribute('aria-label', 'Rent sections');
	await expect(page.getByRole('tab', { name: 'Upcoming' })).toHaveAttribute(
		'aria-controls',
		'panel-upcoming'
	);
	await expect(page.getByRole('tabpanel')).toHaveAttribute('id', 'panel-upcoming');
	await expect(page.getByRole('tabpanel')).toHaveAttribute('tabindex', '0');
});
