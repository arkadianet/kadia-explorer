/**
 * Request router for the e2e mock API — one function per `/v1` route of `xp-api`, over the
 * in-memory dataset built by `fixtures.ts`.
 *
 * Why a hand-written router instead of MSW: MSW's `setupServer` patches the *in-process*
 * fetch/http clients, but Playwright needs the API to be a real listening socket that
 * `vite preview` can proxy `/v1` to. Wrapping MSW handlers in an HTTP server buys nothing
 * over ~150 lines of `node:http` routing, and this way the mock has no runtime dependency
 * at all and can be curl'd by hand.
 *
 * Test toggles (either an HTTP header or a query parameter, so both the browser — via
 * `page.setExtraHTTPHeaders` — and curl can drive them):
 *
 *  - `x-mock-fail: 500` / `?__fail=500` — every route answers with that status as RFC 7807
 *    problem JSON, for the ErrorState/retry test.
 *  - `x-mock-lag: 500` / `?__lag=500` / `MOCK_LAG=500` in the environment — `/v1/status`
 *    reports that many blocks of lag, for the lag-banner test.
 *  - `x-mock-stall: 1` / `?__stall=1` — `/v1/status` reports a non-null `stalled`, for the
 *    status page's stall row.
 *
 * Pagination is a fixed page of `PAGE_SIZE` items with `next_cursor` = the string offset of
 * the next item, so a `?limit=50` from the app still yields several pages.
 */

import type { IncomingMessage, ServerResponse } from 'node:http';
import type { BoxDto, PageDto, TokenHolderDto, TxDto } from '../../../src/lib/api/types.ts';
import { buildDataset, registerKey, type Dataset } from './fixtures.ts';

/** Items per page, small enough that the app's 50-item requests still paginate. */
export const PAGE_SIZE = 5;

/** Titles for the statuses the fail toggle may produce, mirroring `xp-api`'s `ApiError`. */
const FAIL_TITLES: Record<number, string> = {
	400: 'Bad Request',
	404: 'Not Found',
	408: 'Request Timeout',
	500: 'Internal Server Error',
	503: 'Service Unavailable'
};

const HEX64 = /^[0-9a-fA-F]{64}$/;
const DIGITS = /^\d+$/;

let cached: Dataset | null = null;

export function dataset(): Dataset {
	cached ??= buildDataset();
	return cached;
}

// ------------------------------------------------------------------------------ replies

function sendJson(res: ServerResponse, status: number, body: unknown): void {
	const payload = JSON.stringify(body);
	res.writeHead(status, {
		'content-type': 'application/json',
		'access-control-allow-origin': '*',
		'cache-control': 'no-store'
	});
	res.end(payload);
}

function problem(res: ServerResponse, status: number, title: string, detail: string): void {
	res.writeHead(status, {
		'content-type': 'application/problem+json',
		'access-control-allow-origin': '*',
		'cache-control': 'no-store'
	});
	res.end(JSON.stringify({ type: 'about:blank', title, status, detail }));
}

const notFound = (res: ServerResponse) =>
	problem(res, 404, 'Not Found', 'the requested resource does not exist');
const badRequest = (res: ServerResponse, detail: string) =>
	problem(res, 400, 'Bad Request', detail);

function page<T>(items: T[], cursor: string | null, limit: number): PageDto<T> {
	const from = cursor ? Number(cursor) : 0;
	const size = Math.min(limit, PAGE_SIZE);
	const slice = items.slice(from, from + size);
	const next = from + size;
	return { items: slice, next_cursor: next < items.length ? String(next) : null };
}

/**
 * Holders page. Unlike every other list here the real endpoint's cursor is the last row's
 * `"<amount>:<treehex>"` rather than an offset — a keyset cursor over the descending amount
 * order — so the mock reproduces that shape, and a cursor for a row that no longer exists
 * yields an empty last page rather than restarting from the top.
 */
function holderPage(
	items: TokenHolderDto[],
	cursor: string | null,
	limit: number
): PageDto<TokenHolderDto> {
	let from = 0;
	if (cursor !== null) {
		const at = items.findIndex((h) => `${h.amount}:${h.tree_hash}` === cursor);
		from = at < 0 ? items.length : at + 1;
	}
	const slice = items.slice(from, from + Math.min(limit, PAGE_SIZE));
	const last = slice[slice.length - 1];
	const more = last !== undefined && from + slice.length < items.length;
	return { items: slice, next_cursor: more ? `${last.amount}:${last.tree_hash}` : null };
}

function limitOf(url: URL, fallback = PAGE_SIZE): number {
	const raw = url.searchParams.get('limit');
	if (raw === null) return fallback;
	const n = Number(raw);
	return Number.isFinite(n) && n > 0 ? n : fallback;
}

// ------------------------------------------------------------------------------ toggles

function header(req: IncomingMessage, name: string): string | null {
	const v = req.headers[name];
	return typeof v === 'string' ? v : null;
}

/** The status the fail toggle asks for, restricted to the ones that have a title above. */
function failStatus(req: IncomingMessage, url: URL): number | null {
	const raw = header(req, 'x-mock-fail') ?? url.searchParams.get('__fail');
	if (raw === null) return null;
	const n = Number(raw);
	return Number.isInteger(n) && n in FAIL_TITLES ? n : 500;
}

function lagBlocks(req: IncomingMessage, url: URL): number {
	const raw = header(req, 'x-mock-lag') ?? url.searchParams.get('__lag') ?? process.env.MOCK_LAG;
	const n = Number(raw);
	return Number.isFinite(n) && n > 0 ? n : 0;
}

function stallRequested(req: IncomingMessage, url: URL): boolean {
	const raw = header(req, 'x-mock-stall') ?? url.searchParams.get('__stall');
	return raw !== null && raw !== '' && raw !== '0';
}

// ------------------------------------------------------------------------------- routing

function boxesOfTree(d: Dataset, tree: string, unspentOnly: boolean): BoxDto[] {
	const ids = d.boxIdsByTree.get(tree) ?? [];
	const boxes = ids.flatMap((id) => {
		const b = d.boxById.get(id);
		return b ? [b] : [];
	});
	return unspentOnly ? boxes.filter((b) => b.spent_by === null) : boxes;
}

/**
 * Resolves an index's box-id list to boxes, applying the `unspent` and `dir` parameters the
 * token, template and register box endpoints all share.
 */
function boxesOfIds(d: Dataset, ids: string[], url: URL): BoxDto[] {
	const boxes = ids.flatMap((id) => {
		const b = d.boxById.get(id);
		return b ? [b] : [];
	});
	const filtered =
		url.searchParams.get('unspent') === 'true' ? boxes.filter((b) => b.spent_by === null) : boxes;
	// The indexes are newest first, so `dir=asc` is that order reversed.
	return url.searchParams.get('dir') === 'asc' ? filtered.reverse() : filtered;
}

function txsOfTree(d: Dataset, tree: string): TxDto[] {
	return (d.txIdsByTree.get(tree) ?? []).flatMap((id) => {
		const t = d.txById.get(id);
		return t ? [t] : [];
	});
}

/** Resolves `/v1/blocks/{height_or_id}` the way the real handler does. */
function resolveBlock(d: Dataset, raw: string) {
	if (DIGITS.test(raw)) return d.blockByHeight.get(Number(raw));
	if (HEX64.test(raw)) return d.blockById.get(raw.toLowerCase());
	return undefined;
}

export function handle(req: IncomingMessage, res: ServerResponse): void {
	const url = new URL(req.url ?? '/', 'http://mock.local');
	const d = dataset();
	const path = url.pathname;
	const cursor = url.searchParams.get('cursor');
	const limit = limitOf(url);

	if (req.method === 'OPTIONS') {
		res.writeHead(204, {
			'access-control-allow-origin': '*',
			'access-control-allow-headers': '*'
		});
		res.end();
		return;
	}

	if (req.method !== 'GET') {
		problem(res, 405, 'Method Not Allowed', `${req.method} is not supported`);
		return;
	}

	const fail = failStatus(req, url);
	if (fail !== null && path.startsWith('/v1/')) {
		problem(res, fail, FAIL_TITLES[fail], 'the mock was asked to fail this request');
		return;
	}

	// --- status --------------------------------------------------------------------
	if (path === '/v1/status') {
		const lag = lagBlocks(req, url);
		const indexed = d.status.indexed ?? d.status.best;
		sendJson(res, 200, {
			...d.status,
			lag_blocks: lag,
			best: d.status.best + lag,
			stalled: stallRequested(req, url)
				? {
						height: indexed + 1,
						since_secs: 137,
						reason: 'node has not served this block yet'
					}
				: null
		});
		return;
	}

	// --- blocks --------------------------------------------------------------------
	if (path === '/v1/blocks') {
		const dir = url.searchParams.get('dir');
		const items = dir === 'asc' ? [...d.blocks].reverse() : d.blocks;
		sendJson(res, 200, page(items, cursor, limit));
		return;
	}

	const blockTxs = /^\/v1\/blocks\/([^/]+)\/txs$/.exec(path);
	if (blockTxs) {
		const block = resolveBlock(d, decodeURIComponent(blockTxs[1]));
		if (!block) return notFound(res);
		const ids = d.txIdsByHeight.get(block.height) ?? [];
		sendJson(
			res,
			200,
			ids.flatMap((id) => {
				const t = d.txById.get(id);
				return t ? [t] : [];
			})
		);
		return;
	}

	const oneBlock = /^\/v1\/blocks\/([^/]+)$/.exec(path);
	if (oneBlock) {
		const block = resolveBlock(d, decodeURIComponent(oneBlock[1]));
		return block ? sendJson(res, 200, block) : notFound(res);
	}

	// --- transactions --------------------------------------------------------------
	if (path === '/v1/txs') {
		sendJson(res, 200, page(d.txs, cursor, limit));
		return;
	}

	const oneTx = /^\/v1\/txs\/([^/]+)$/.exec(path);
	if (oneTx) {
		const tx = d.txById.get(decodeURIComponent(oneTx[1]).toLowerCase());
		return tx ? sendJson(res, 200, tx) : notFound(res);
	}

	// --- boxes ---------------------------------------------------------------------
	const boxRent = /^\/v1\/boxes\/([^/]+)\/rent$/.exec(path);
	if (boxRent) {
		const b = d.boxById.get(decodeURIComponent(boxRent[1]).toLowerCase());
		return b ? sendJson(res, 200, { box_id: b.id, ...b.rent }) : notFound(res);
	}

	const oneBox = /^\/v1\/boxes\/([^/]+)$/.exec(path);
	if (oneBox) {
		const b = d.boxById.get(decodeURIComponent(oneBox[1]).toLowerCase());
		return b ? sendJson(res, 200, b) : notFound(res);
	}

	// --- addresses -----------------------------------------------------------------
	const address = /^\/v1\/addresses\/([^/]+)(\/boxes|\/txs|\/rent)?$/.exec(path);
	if (address) {
		const addr = decodeURIComponent(address[1]);
		const tree = d.treeByAddress.get(addr);
		if (tree === undefined) return notFound(res);
		switch (address[2]) {
			case '/boxes': {
				const unspent = url.searchParams.get('unspent') === 'true';
				sendJson(res, 200, page(boxesOfTree(d, tree, unspent), cursor, limit));
				return;
			}
			case '/txs':
				sendJson(res, 200, page(txsOfTree(d, tree), cursor, limit));
				return;
			case '/rent':
				sendJson(res, 200, { items: boxesOfTree(d, tree, true), truncated: false });
				return;
			default: {
				const info = d.addresses.get(addr);
				return info ? sendJson(res, 200, info) : notFound(res);
			}
		}
	}

	// --- tokens ------------------------------------------------------------------------
	if (path === '/v1/tokens') {
		const sort = url.searchParams.get('sort');
		if (sort !== null && sort !== 'newest' && sort !== 'holders') {
			return badRequest(res, `sort must be "newest" or "holders", not ${JSON.stringify(sort)}`);
		}
		sendJson(res, 200, page(sort === 'holders' ? d.tokensByHolders : d.tokens, cursor, limit));
		return;
	}

	const token = /^\/v1\/tokens\/([^/]+)(\/holders|\/boxes)?$/.exec(path);
	if (token) {
		const id = decodeURIComponent(token[1]).toLowerCase();
		const info = d.tokenById.get(id);
		if (info === undefined) return notFound(res);
		switch (token[2]) {
			case '/holders':
				sendJson(res, 200, holderPage(d.tokenHolders.get(id) ?? [], cursor, limit));
				return;
			case '/boxes':
				sendJson(res, 200, page(boxesOfIds(d, d.boxIdsByToken.get(id) ?? [], url), cursor, limit));
				return;
			default:
				sendJson(res, 200, info);
				return;
		}
	}

	// --- templates ---------------------------------------------------------------------
	const template = /^\/v1\/templates\/([^/]+)(\/boxes)?$/.exec(path);
	if (template) {
		const hash = decodeURIComponent(template[1]).toLowerCase();
		const info = d.templates.get(hash);
		if (info === undefined) return notFound(res);
		if (template[2] === '/boxes') {
			sendJson(
				res,
				200,
				page(boxesOfIds(d, d.boxIdsByTemplate.get(hash) ?? [], url), cursor, limit)
			);
			return;
		}
		sendJson(res, 200, info);
		return;
	}

	// --- register lookup -----------------------------------------------------------------
	// A register that is not R4–R9 and a value that is not whole bytes of hex are both 400s,
	// as on the real endpoint; a *well-formed* value nothing carries is an empty 200, since
	// "no box has this" is an answer, not an error.
	const register = /^\/v1\/registers\/([^/]+)\/([^/]+)\/boxes$/.exec(path);
	if (register) {
		const reg = decodeURIComponent(register[1]).toUpperCase();
		if (!/^R[4-9]$/.test(reg)) return badRequest(res, `${reg} is not one of R4..R9`);
		const value = decodeURIComponent(register[2]);
		if (!/^[0-9a-fA-F]+$/.test(value)) return badRequest(res, 'the register value must be hex');
		if (value.length % 2 !== 0) {
			return badRequest(res, 'the register value must be a whole number of bytes');
		}
		const ids = d.boxIdsByRegister.get(registerKey(reg, value)) ?? [];
		sendJson(res, 200, page(boxesOfIds(d, ids, url), cursor, limit));
		return;
	}

	// --- richlist / rent -------------------------------------------------------------
	if (path === '/v1/richlist') {
		sendJson(res, 200, page(d.richlist, cursor, limit));
		return;
	}

	if (path === '/v1/rent/upcoming') {
		// The `blocks` window is ignored on purpose — see the rent note in fixtures.ts.
		sendJson(res, 200, { items: d.rentUpcoming.slice(0, limit), next_cursor: null });
		return;
	}

	if (path === '/v1/rent/eligible') {
		sendJson(res, 200, page(d.rentEligible, cursor, limit));
		return;
	}

	// --- search ----------------------------------------------------------------------
	if (path === '/v1/search') {
		const q = (url.searchParams.get('q') ?? '').trim();
		if (!q) return badRequest(res, 'q must not be empty');
		if (DIGITS.test(q)) {
			const block = d.blockByHeight.get(Number(q));
			return block ? sendJson(res, 200, { kind: 'block', id: q }) : notFound(res);
		}
		if (HEX64.test(q)) {
			const id = q.toLowerCase();
			if (d.blockById.has(id)) return sendJson(res, 200, { kind: 'block', id });
			if (d.txById.has(id)) return sendJson(res, 200, { kind: 'tx', id });
			if (d.boxById.has(id)) return sendJson(res, 200, { kind: 'box', id });
			// Token and template come last: an id that is also a box id is the box, which is
			// the order the real resolver tries them in.
			if (d.tokenById.has(id)) return sendJson(res, 200, { kind: 'token', id });
			if (d.templates.has(id)) return sendJson(res, 200, { kind: 'template', id });
			return notFound(res);
		}
		if (d.treeByAddress.has(q)) return sendJson(res, 200, { kind: 'address', id: q });
		if (/^[9382]/.test(q) && q.length >= 40) return notFound(res);
		return badRequest(res, `q is not a height, a 64-hex id, or an address: ${JSON.stringify(q)}`);
	}

	notFound(res);
}
