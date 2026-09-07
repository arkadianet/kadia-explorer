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
	await scrollUntilCount(page, 'table.table tbody tr', expected);
}

/**
 * The same, for a list whose items are not table rows — the `BoxCard` lists on the token,
 * template and register-lookup pages. Asserting the full count rather than one page's worth
 * is what makes these specs independent of how many pages the intersection observer happened
 * to pull in before the first assertion ran.
 */
export async function scrollUntilCount(
	page: Page,
	selector: string,
	expected: number
): Promise<void> {
	const rows = page.locator(selector);
	for (let i = 0; i < 40 && (await rows.count()) < expected; i++) {
		const sentinel = page.locator('.sentinel');
		if ((await sentinel.count()) === 0) break;
		// Back to the top first: the newly appended rows can leave the sentinel *partially*
		// visible, and then `scrollIntoViewIfNeeded` is a no-op and the observer never fires
		// again. Leaving and re-entering the viewport guarantees an intersection change.
		await page.evaluate(() => window.scrollTo(0, 0));
		try {
			await sentinel.first().scrollIntoViewIfNeeded({ timeout: 2000 });
		} catch {
			// The sentinel detaches whenever a filter toggle restarts the pager, and the count
			// above can be a round out of date. Let the next round find its replacement rather
			// than waiting out the whole test timeout on an element that is already gone.
		}
		await page.waitForTimeout(120);
	}
	await expect(rows).toHaveCount(expected);
}
