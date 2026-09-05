import { expect, type Page } from '@playwright/test';

/**
 * A viewport short enough that an `InfiniteList` of five rows already overflows it, so the
 * sentinel starts below the fold and each `scrollIntoViewIfNeeded` is a real intersection
 * change — an always-visible sentinel would only ever fire its observer once.
 */
export async function useShortViewport(page: Page): Promise<void> {
	await page.setViewportSize({ width: 900, height: 360 });
}

/**
 * Scrolls `InfiniteList`'s sentinel into view until every page has been pulled in. The mock
 * pages by 5, so a full list needs several rounds; the loop is bounded so a genuinely stuck
 * list fails on the row-count assertion rather than hanging.
 */
export async function scrollUntilLoaded(page: Page, expected: number): Promise<void> {
	const rows = page.locator('table.table tbody tr');
	for (let i = 0; i < 40 && (await rows.count()) < expected; i++) {
		const sentinel = page.locator('.sentinel');
		if ((await sentinel.count()) === 0) break;
		// Back to the top first: the newly appended rows can leave the sentinel *partially*
		// visible, and then `scrollIntoViewIfNeeded` is a no-op and the observer never fires
		// again. Leaving and re-entering the viewport guarantees an intersection change.
		await page.evaluate(() => window.scrollTo(0, 0));
		await sentinel.first().scrollIntoViewIfNeeded();
		await page.waitForTimeout(120);
	}
	await expect(rows).toHaveCount(expected);
}
