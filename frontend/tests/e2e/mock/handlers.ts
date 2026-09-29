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
import { readFileSync } from 'node:fs';
import type { GroupBalances, GroupMember } from '../../../src/lib/addresses/groups.ts';
import { range as networkRange } from '../../../src/lib/network/history.ts';
import type { NetworkBucket } from '../../../src/lib/network/history.ts';
import { range as miningRange } from '../../../src/lib/mining/overview.ts';
import type { MiningKey, MiningOverview } from '../../../src/lib/mining/overview.ts';
import type { PendingTransaction } from '../../../src/lib/mempool/observations.ts';
import type {
	AddressActivityDto,
	AddressActivityPageDto,
	BoxDto,
	PageDto,
	TokenHolderDto,
	TxDto,
	TxSummaryDto
} from '../../../src/lib/api/types.ts';
import { buildDataset, FIXTURE_HEIGHTS, registerKey, type Dataset } from './fixtures.ts';

/** Items per page, small enough that the app's 50-item requests still paginate. */
export const PAGE_SIZE = 5;

// Synthetic pending observations for the preview, separate from mined fixtures.
const pendingFixtures: PendingTransaction[] = [
	{
		id: 'a1'.repeat(32),
		input_count: 2,
		data_input_count: 0,
		output_count: 3,
		size: 512,
		fee: '1100000'
	},
	{
		id: 'b2'.repeat(32),
		input_count: 1,
		data_input_count: 1,
		output_count: 2,
		size: null,
		fee: '2200000'
	},
	{
		id: 'c3'.repeat(32),
		input_count: 4,
		data_input_count: 0,
		output_count: 5,
		size: 1024,
		fee: null
	}
];

// A captured mainnet workflow for the local demo. Display-only size/rent values are
// synthetic; workflow identities, scripts, registers and amounts come from the fixture.
function applicationFixture<T>(name: string): T {
	return JSON.parse(
		readFileSync(new URL(`../../../../tests/fixtures/apps/${name}.json`, import.meta.url), 'utf8')
	);
}
const appFixtures = [
	'spectrum-v3-swap',
	'spectrum-v3-swap-buy',
	'spectrum-v3-deposit',
	'spectrum-v3-redeem'
].map((name) => applicationFixture<{ order: BoxDto; settlement: TxDto }>(name));
const rosenFixture = applicationFixture<{
	deposit: BoxDto;
	sourceTx: TxDto;
	event: BoxDto;
	eventSpend: TxDto;
}>('rosen-erg-cardano');
function appBox(box: BoxDto): BoxDto {
	const knownSpend = [
		...appFixtures.map((fixture) => fixture.settlement),
		rosenFixture.sourceTx,
		rosenFixture.eventSpend
	].find((tx) => tx.id === box.spent_by);
	return {
		...box,
		spent_height: knownSpend?.height ?? box.spent_height,
		size: 0,
		kind: 'box',
		rent: {
			maturity_height: box.creation_height + 1_051_200,
			due_nano: '0',
			claimable_at_tip: false,
			consensus_fee_nano: '0',
			collectible: false
		}
	};
}
const appTransactions = new Map(
	[
		...appFixtures.map((fixture) => fixture.settlement),
		rosenFixture.sourceTx,
		rosenFixture.eventSpend
	].map((tx): [string, TxDto] => [
		tx.id,
		{
			...tx,
			inputs: tx.inputs.map((input) => ({ ...input, box: input.box ? appBox(input.box) : null })),
			outputs: tx.outputs.map(appBox)
		}
	])
);
const appBoxes = new Map(
	[
		...appFixtures.map((fixture) => appBox(fixture.order)),
		appBox(rosenFixture.deposit),
		appBox(rosenFixture.event),
		...[...appTransactions.values()].flatMap((tx) => [
			...tx.inputs.flatMap((input) => (input.box ? [input.box] : [])),
			...tx.outputs
		])
	].map((box) => [box.id, box])
);

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

/** `/v1/addresses/{addr}/txs` — mirrors `xp-api`'s switch to cheap summaries (Task 1). */
function txSummariesOfTree(d: Dataset, tree: string): TxSummaryDto[] {
	return txsOfTree(d, tree).map(txSummary);
}

/** Explicit group fixture read: the sample index is partial and has no address decoder. */
async function addressGroupBalances(d: Dataset, req: IncomingMessage, res: ServerResponse) {
	const reject = (status: number, code: string, detail: string) =>
		sendJson(res, status, { status, title: 'Group balances unavailable', code, detail });
	if (header(req, 'content-type')?.split(';')[0].trim().toLowerCase() !== 'application/json')
		return reject(
			400,
			'invalid_group_request',
			'Send an application/json object containing only addresses.'
		);
	const raw = await new Promise<string | null>((resolve, rejectRead) => {
		const chunks: Buffer[] = [];
		let bytes = 0;
		let finished = false;
		req.on('data', (chunk: Buffer) => {
			if (finished) return;
			bytes += chunk.length;
			if (bytes > 512_000) {
				finished = true;
				resolve(null);
				return;
			}
			chunks.push(chunk);
		});
		req.once('end', () => {
			if (!finished) resolve(Buffer.concat(chunks).toString('utf8'));
		});
		req.once('error', rejectRead);
	});
	if (raw === null)
		return reject(413, 'group_request_limit', 'Group request exceeds its byte limit.');
	let body: unknown;
	try {
		body = JSON.parse(raw);
	} catch {
		return reject(
			400,
			'invalid_group_request',
			'Send a valid JSON object containing only addresses.'
		);
	}
	if (
		!body ||
		typeof body !== 'object' ||
		Array.isArray(body) ||
		Object.keys(body).length !== 1 ||
		!('addresses' in body) ||
		!Array.isArray(body.addresses) ||
		!body.addresses.length ||
		body.addresses.length > 100 ||
		body.addresses.some((address: unknown) => typeof address !== 'string')
	)
		return reject(
			400,
			'invalid_group_request',
			'Select 1–100 address strings; labels and group names are not accepted.'
		);
	const addresses = body.addresses as string[];
	if (
		addresses.some((address) => Buffer.byteLength(address, 'utf8') > 4096) ||
		addresses.reduce((bytes, address) => bytes + Buffer.byteLength(address, 'utf8'), 0) > 128_000
	)
		return reject(413, 'group_request_limit', 'Group addresses exceed their byte limit.');
	// Only fixture mappings can establish canonical scripts here. A plausible unknown
	// address is unsupported, not a made-up hash, a zero balance, or a verified invalidity.
	if (
		addresses.some(
			(address) => !d.treeByAddress.has(address) && /^[1-9A-HJ-NP-Za-km-z]{7,4096}$/.test(address)
		)
	)
		return reject(
			501,
			'mock_address_unavailable',
			'This preview only resolves recorded sample addresses. A requested address is not mapped in the fixture; its validity and balance are unknown.'
		);
	const members: GroupMember[] = [];
	const seen = new Map<string, number>();
	const tokens = new Map<string, bigint>();
	let nano = 0n;
	let missing = false;
	let tokenEntries = 0;
	for (const address of addresses) {
		const tree = d.treeByAddress.get(address);
		if (tree === undefined) {
			missing = true;
			members.push({
				address,
				tree_hash: null,
				status: 'invalid',
				duplicate_of: null,
				balance: null
			});
			continue;
		}
		const first = seen.get(tree);
		if (first !== undefined) {
			members.push({
				address,
				tree_hash: tree,
				status: 'duplicate',
				duplicate_of: first,
				balance: null
			});
			continue;
		}
		const info = d.addresses.get(address);
		if (!info)
			return reject(
				500,
				'mock_fixture_integrity',
				'A known sample address has no retained balance.'
			);
		seen.set(tree, members.length);
		const balance = {
			nano: info.balance.nano,
			tokens: info.balance.tokens
				.map(({ id, amount }) => ({ id, amount }))
				.sort((a, b) => a.id.localeCompare(b.id))
		};
		tokenEntries += balance.tokens.length;
		if (tokenEntries > 5000)
			return reject(
				422,
				'group_work_limit',
				'The selected fixture balances exceed the bounded token budget.'
			);
		nano += BigInt(balance.nano);
		for (const token of balance.tokens)
			tokens.set(token.id, (tokens.get(token.id) ?? 0n) + BigInt(token.amount));
		members.push({ address, tree_hash: tree, status: 'resolved', duplicate_of: null, balance });
	}
	const block = d.status.indexed === null ? undefined : d.blockByHeight.get(d.status.indexed);
	const result: GroupBalances = {
		scope: 'selected_address_scripts',
		consistency: 'single_reader',
		indexed_height: d.status.indexed,
		anchor: block ? { height: block.height, block_id: block.id } : null,
		full_history: false,
		partial_from: Math.min(...FIXTURE_HEIGHTS),
		complete: false,
		requested_count: addresses.length,
		resolved_script_count: seen.size,
		members,
		observed_totals: missing
			? null
			: {
					nano: nano.toString(),
					tokens: [...tokens]
						.sort(([a], [b]) => a.localeCompare(b))
						.map(([id, amount]) => ({ id, amount: amount.toString() }))
				}
	};
	if (Buffer.byteLength(JSON.stringify(result), 'utf8') > 2 * 1024 * 1024)
		return reject(
			422,
			'group_response_limit',
			'The selected fixture balances exceed the bounded response budget.'
		);
	sendJson(res, 200, result);
}

function txSummary(t: TxDto): TxSummaryDto {
	return {
		id: t.id,
		height: t.height,
		index: t.index,
		timestamp: t.timestamp,
		size: t.size,
		fee: t.fee,
		input_count: t.inputs.length,
		data_input_count: t.data_inputs.length,
		output_count: t.outputs.length
	};
}

/** Token touches use only retained fixture inputs/outputs, not guessed transfer intent. */
function tokenTransactions(d: Dataset, id: string, url: URL, res: ServerResponse): void {
	const dir = url.searchParams.get('dir') ?? 'desc';
	const limit = Number(url.searchParams.get('limit') ?? 20);
	const cursor = url.searchParams.get('cursor');
	const offset = Number(cursor ?? 0);
	if (
		!['asc', 'desc'].includes(dir) ||
		![null, 'strict'].includes(url.searchParams.get('consistency')) ||
		!Number.isInteger(limit) ||
		limit < 1 ||
		limit > 100 ||
		!Number.isSafeInteger(offset) ||
		offset < 0 ||
		(cursor !== null && !DIGITS.test(cursor))
	)
		return badRequest(res, 'invalid token history pagination');
	const block = d.blockByHeight.get(d.status.indexed ?? d.status.best)!;
	const anchor = { height: block.height, block_id: block.id };
	const snapshot = `mock-token-history-${Buffer.from(JSON.stringify([anchor, id, dir])).toString('base64url')}`;
	const provided = url.searchParams.get('snapshot');
	if ((cursor !== null && provided !== snapshot) || (provided !== null && provided !== snapshot))
		return sendJson(res, 409, {
			status: 409,
			title: 'Snapshot changed',
			code: 'snapshot_changed',
			detail: 'Restart token history with the current snapshot and token.'
		});
	const seen = new Set<string>();
	const touched = d.txs
		.filter((tx) => {
			if (seen.has(tx.id)) return false;
			const touches =
				tx.outputs.some((box) => box.tokens.some((token) => token.id === id)) ||
				tx.inputs.some((input) => input.box?.tokens.some((token) => token.id === id));
			if (touches) seen.add(tx.id);
			return touches;
		})
		.sort((a, b) =>
			dir === 'asc'
				? a.height - b.height || a.index - b.index
				: b.height - a.height || b.index - a.index
		);
	const result = page(touched.map(txSummary), cursor, limit);
	sendJson(res, 200, {
		...result,
		next_snapshot: result.next_cursor ? snapshot : null,
		consistency: 'strict',
		anchor,
		observed_anchor: anchor,
		history_context: { scope: 'indexed_token_touches', partial_from: FIXTURE_HEIGHTS[0] }
	});
}

/** Recorded fixture activity, with the same incomplete-input rules as the real endpoint.
 * The mock's numeric cursor tracks scanned candidates, including filtered-out rows. */
function addressActivity(d: Dataset, tree: string, url: URL, res: ServerResponse): void {
	const asset = (url.searchParams.get('asset') ?? 'all').toLowerCase();
	const direction = url.searchParams.get('direction') ?? 'all';
	const dir = url.searchParams.get('dir') ?? 'desc';
	const limit = Number(url.searchParams.get('limit') ?? 20);
	const offset = Number(url.searchParams.get('cursor') ?? 0);
	const time = (key: string) =>
		url.searchParams.has(key) ? Number(url.searchParams.get(key)) : null;
	const from = time('from_ms');
	const to = time('to_ms');
	if (
		(asset !== 'all' && asset !== 'erg' && !HEX64.test(asset)) ||
		!['all', 'received', 'sent', 'mixed', 'neutral', 'unknown'].includes(direction) ||
		!['asc', 'desc'].includes(dir) ||
		!Number.isInteger(limit) ||
		limit < 1 ||
		limit > 100 ||
		!Number.isSafeInteger(offset) ||
		offset < 0 ||
		[from, to].some(
			(value) =>
				value !== null &&
				(!Number.isSafeInteger(value) || value < 0 || value > 8_640_000_000_000_000)
		) ||
		(from !== null && to !== null && from >= to) ||
		![null, 'strict'].includes(url.searchParams.get('consistency'))
	) {
		return badRequest(res, 'invalid address activity filters or pagination');
	}
	const block = d.blockByHeight.get(d.status.indexed ?? d.status.best)!;
	const anchor = { height: block.height, block_id: block.id };
	const snapshot = `mock-activity-${Buffer.from(JSON.stringify([anchor, tree, asset, direction, dir, from, to])).toString('base64url')}`;
	const providedSnapshot = url.searchParams.get('snapshot');
	if (
		(offset > 0 && providedSnapshot !== snapshot) ||
		(providedSnapshot !== null && providedSnapshot !== snapshot)
	) {
		return sendJson(res, 409, {
			status: 409,
			title: 'Snapshot changed',
			code: 'snapshot_changed',
			detail: 'Restart fixture activity with the current filters and snapshot.'
		});
	}
	const candidates = txsOfTree(d, tree);
	if (dir === 'asc') candidates.reverse();
	const items: AddressActivityDto[] = [];
	let scanned = 0;
	let next = offset;
	while (next < candidates.length && scanned < 200 && items.length < Math.min(PAGE_SIZE, limit)) {
		const tx = candidates[next++];
		scanned++;
		if ((from !== null && tx.timestamp < from) || (to !== null && tx.timestamp >= to)) continue;
		const resolved = tx.inputs.filter((input) => input.box !== null).length;
		const complete = resolved === tx.inputs.length;
		let erg = 0n;
		const tokens = new Map<string, bigint>();
		function accumulate(box: BoxDto, sign: bigint) {
			if (box.tree_hash !== tree) return;
			erg += sign * BigInt(box.value);
			for (const token of box.tokens)
				tokens.set(token.id, (tokens.get(token.id) ?? 0n) + sign * BigInt(token.amount));
		}
		for (const input of tx.inputs) if (input.box) accumulate(input.box, -1n);
		for (const output of tx.outputs) accumulate(output, 1n);
		const tokenMissing = asset !== 'all' && asset !== 'erg' && !tokens.has(asset);
		if (tokenMissing && complete) continue;
		const values =
			asset === 'all'
				? [erg, ...tokens.values()]
				: asset === 'erg'
					? [erg]
					: [tokens.get(asset) ?? 0n];
		const positive = values.some((value) => value > 0n);
		const negative = values.some((value) => value < 0n);
		const actualDirection = !complete
			? 'unknown'
			: positive && negative
				? 'mixed'
				: positive
					? 'received'
					: negative
						? 'sent'
						: 'neutral';
		if (direction !== 'all' && direction !== actualDirection) continue;
		items.push({
			id: tx.id,
			height: tx.height,
			block_id: tx.block_id ?? d.blockByHeight.get(tx.height)!.id,
			timestamp: tx.timestamp,
			index: tx.index,
			fee: tx.fee,
			input_count: tx.inputs.length,
			output_count: tx.outputs.length,
			coverage: { complete, resolved_inputs: resolved, total_inputs: tx.inputs.length },
			erg_delta: complete ? erg.toString() : null,
			tokens: [...tokens]
				.sort(([a], [b]) => a.localeCompare(b))
				.map(([id, delta]) => ({
					id,
					name: d.tokenById.get(id)?.name || null,
					decimals: d.tokenById.get(id)?.decimals ?? null,
					delta: complete ? delta.toString() : null
				})),
			direction: actualDirection,
			asset_match: tokenMissing ? 'uncertain' : 'definite'
		});
	}
	const more = next < candidates.length;
	const result: AddressActivityPageDto = {
		items,
		next_cursor: more ? String(next) : null,
		next_snapshot: more ? snapshot : null,
		consistency: 'strict',
		anchor,
		observed_anchor: anchor,
		scanned,
		scan_limit_reached: scanned === 200 && more,
		partial_from: FIXTURE_HEIGHTS[0]
	};
	sendJson(res, 200, result);
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

	if (req.method !== 'GET' && !(req.method === 'POST' && path === '/v1/addresses/balances')) {
		problem(res, 405, 'Method Not Allowed', `${req.method} is not supported`);
		return;
	}
	const fail = failStatus(req, url);
	if (fail !== null && path.startsWith('/v1/')) {
		problem(res, fail, FAIL_TITLES[fail], 'the mock was asked to fail this request');
		return;
	}
	if (req.method === 'POST' && path === '/v1/addresses/balances') {
		void addressGroupBalances(d, req, res).catch((error: unknown) => {
			if (!res.headersSent && !res.destroyed)
				problem(
					res,
					500,
					'Fixture read failed',
					error instanceof Error ? error.message : 'Unable to read the request.'
				);
		});
		return;
	}
	// --- status --------------------------------------------------------------------
	if (path === '/v1/mempool') {
		const checked = Date.now();
		return sendJson(res, 200, {
			scope: 'configured_node_mempool',
			source: 'configured_primary_node',
			checked_at_ms: checked,
			expires_at_ms: checked + 5000,
			cached: false,
			limit: 100,
			limit_reached: false,
			observed_count: pendingFixtures.length,
			items: pendingFixtures,
			connections: {
				scope: 'returned_snapshot_only',
				output_count: 10,
				identified_output_count: 10,
				edge_count: 2,
				edges_truncated: false,
				edges: [
					{
						producer_id: pendingFixtures[0].id,
						consumer_id: pendingFixtures[1].id,
						box_id: 'd4'.repeat(32),
						kind: 'spend'
					},
					{
						producer_id: pendingFixtures[2].id,
						consumer_id: pendingFixtures[1].id,
						box_id: 'e5'.repeat(32),
						kind: 'read'
					}
				],
				shared_input_count: 0,
				shared_inputs_truncated: false,
				shared_inputs: []
			}
		});
	}
	const pendingFixture = pendingFixtures.find((tx) => path === `/v1/txs/${tx.id}/status`);
	if (pendingFixture) {
		const checked = Date.now();
		return sendJson(res, 200, {
			id: pendingFixture.id,
			state: 'pending',
			checked_at_ms: checked,
			indexed_height: d.status.indexed,
			inclusion: null,
			previous_inclusion: null,
			pending: {
				...pendingFixture,
				details: {
					inputs:
						pendingFixture.id === pendingFixtures[1].id
							? ['bb'.repeat(32)]
							: [...d.boxById.keys()].slice(0, pendingFixture.input_count),
					data_inputs: pendingFixture.data_input_count ? ['cc'.repeat(32)] : [],
					outputs: Array.from({ length: pendingFixture.output_count }, (_, index) => {
						// Synthetic output identities preserve the connections shown by this demo.
						const sample = appFixtures
							.flatMap((fixture) => fixture.settlement.outputs)
							.find((box) => box.address && box.ergo_tree && box.ergo_tree.length <= 1024)!;
						const token = d.tokens[0];
						const isFee = pendingFixture.fee !== null && index === pendingFixture.output_count - 1;
						const feeTree =
							'1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304';
						return {
							index,
							id:
								index === 0 && pendingFixture.id !== pendingFixtures[1].id
									? (pendingFixture.id === pendingFixtures[0].id ? 'bb' : 'cc').repeat(32)
									: pendingFixture.id.slice(0, 62) + index.toString(16).padStart(2, '0'),
							value: isFee ? pendingFixture.fee! : String(1_000_000_000 + index * 100_000_000),
							ergo_tree: isFee ? feeTree : sample.ergo_tree,
							address: isFee ? null : sample.address,
							tokens: index === 0 ? [{ id: token.id, amount: '250000000' }] : [],
							token_count: index === 0 ? 1 : 0,
							tokens_truncated: false,
							ergo_tree_truncated: false
						};
					}),
					inputs_truncated: false,
					data_inputs_truncated: false,
					outputs_truncated: false,
					complete: true
				}
			},
			conflicts: [],
			mempool: {
				observation: 'present',
				checked_at_ms: checked,
				first_seen_at_ms: checked,
				last_seen_at_ms: checked,
				error: null
			},
			history_scope: 'process_local_requested_transactions',
			retention_seconds: 3600
		});
	}
	const appStatusId = /^\/v1\/txs\/([a-f0-9]{64})\/status$/.exec(path)?.[1];
	const appTransaction = appStatusId ? appTransactions.get(appStatusId) : undefined;
	if (appTransaction) {
		return sendJson(res, 200, {
			id: appTransaction.id,
			state: 'confirmed',
			checked_at_ms: Date.now(),
			indexed_height: appTransaction.height,
			inclusion: {
				block_id: appTransaction.block_id,
				height: appTransaction.height,
				confirmations: 1
			},
			previous_inclusion: null,
			mempool: {
				observation: 'not_checked',
				checked_at_ms: null,
				first_seen_at_ms: null,
				last_seen_at_ms: null,
				error: null
			},
			pending: null,
			conflicts: [],
			history_scope: 'process_local_requested_transactions',
			retention_seconds: 3600
		});
	}
	if (path === '/v1/status') {
		const lag = lagBlocks(req, url);
		const indexed = d.status.indexed ?? d.status.best;
		sendJson(res, 200, {
			...d.status,
			source_observed_at_ms: Date.now(),
			source_error: null,
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

	if (path === '/v1/network/summary') {
		const rows = d.blocks.slice(0, 720);
		const anchor = rows[0]?.timestamp ?? 0;
		const bucket = (step: number, count: number, value: (b: (typeof rows)[number]) => number) =>
			Array.from({ length: count }, (_, i) =>
				rows
					.filter((b) => {
						const age = anchor - b.timestamp;
						return age >= 0 && age < step * count && count - 1 - Math.floor(age / step) === i;
					})
					.reduce((n, b) => n + value(b), 0)
			);
		return sendJson(res, 200, {
			scope: 'latest_indexed_blocks',
			requested_blocks: 720,
			block_count: rows.length,
			transaction_count: rows.reduce((n, b) => n + b.tx_count, 0),
			fees: rows.reduce((n, b) => n + BigInt(b.fees), 0n).toString(),
			from_height: rows.at(-1)?.height ?? null,
			to_height: rows[0]?.height ?? null,
			anchor_id: rows[0]?.id ?? null,
			earliest_timestamp: rows.length ? Math.min(...rows.map((b) => b.timestamp)) : null,
			latest_timestamp: rows.length ? Math.max(...rows.map((b) => b.timestamp)) : null,
			partial_from: null,
			recent_blocks: rows.slice(0, 10),
			blocks_per_hour: bucket(3600000, 24, () => 1),
			transactions_per_hour: bucket(3600000, 24, (b) => b.tx_count),
			fees_per_hour: bucket(3600000, 24, (b) => Number(b.fees)).map(String),
			blocks_per_ten_minutes: bucket(600000, 36, () => 1)
		});
	}
	if (path === '/v1/network/history') {
		let requested;
		try {
			requested = networkRange(
				url.searchParams.get('from_height') ?? '',
				url.searchParams.get('to_height') ?? '',
				url.searchParams.get('buckets') ?? '60',
				url.searchParams.get('end_block_id') ?? undefined
			);
		} catch {
			return badRequest(res, 'Choose a positive range of at most 20,160 retained fixture blocks.');
		}
		const rows = d.blocks
			.filter((b) => b.height >= requested.from_height && b.height <= requested.to_height)
			.sort((a, b) => a.height - b.height);
		if (rows.length !== requested.to_height - requested.from_height + 1)
			return problem(
				res,
				422,
				'Fixture history unavailable',
				`This preview retains heights ${Math.min(...d.blocks.map((b) => b.height))}–${Math.max(...d.blocks.map((b) => b.height))}. Choose a range inside those heights.`
			);
		const last = rows.at(-1)!;
		if (requested.end_block_id && requested.end_block_id !== last.id)
			return problem(res, 409, 'Anchor changed', 'The pinned fixture end block differs.');
		const aggregate = (blocks: typeof rows): NetworkBucket => {
			const difficulties = blocks.map((b) => BigInt(b.difficulty));
			return {
				from_height: blocks[0].height,
				to_height: blocks.at(-1)!.height,
				block_count: blocks.length,
				transaction_count: blocks.reduce((n, b) => n + BigInt(b.tx_count), 0n).toString(),
				fees: blocks.reduce((n, b) => n + BigInt(b.fees), 0n).toString(),
				size_bytes: blocks.reduce((n, b) => n + BigInt(b.size), 0n).toString(),
				difficulty_min: difficulties.reduce((a, b) => (a < b ? a : b)).toString(),
				difficulty_max: difficulties.reduce((a, b) => (a > b ? a : b)).toString(),
				difficulty_end: blocks.at(-1)!.difficulty,
				first_timestamp: blocks[0].timestamp,
				last_timestamp: blocks.at(-1)!.timestamp,
				earliest_timestamp: Math.min(...blocks.map((b) => b.timestamp)),
				latest_timestamp: Math.max(...blocks.map((b) => b.timestamp))
			};
		};
		const width = Math.ceil(rows.length / requested.buckets);
		return sendJson(res, 200, {
			scope: 'canonical_block_headers',
			consistency: 'single_reader',
			complete: true,
			full_history: false,
			partial_from: Math.min(...d.blocks.map((b) => b.height)),
			indexed_height: d.status.indexed,
			anchor: { height: last.height, block_id: last.id },
			requested_buckets: requested.buckets,
			bucket_width: width,
			totals: aggregate(rows),
			buckets: Array.from({ length: Math.ceil(rows.length / width) }, (_, i) =>
				aggregate(rows.slice(i * width, (i + 1) * width))
			)
		});
	}
	if (path === '/v1/mining') {
		let requested;
		try {
			requested = miningRange(
				url.searchParams.get('from_height') ?? '',
				url.searchParams.get('to_height') ?? '',
				url.searchParams.get('top') ?? '20',
				url.searchParams.get('end_block_id') ?? undefined
			);
		} catch {
			return badRequest(res, 'Choose 1–20,160 retained fixture blocks and 1–50 keys.');
		}
		const rows = d.blocks
			.filter((b) => b.height >= requested.from_height && b.height <= requested.to_height)
			.sort((a, b) => a.height - b.height);
		if (rows.length !== requested.to_height - requested.from_height + 1)
			return problem(
				res,
				422,
				'Fixture history unavailable',
				`This preview retains heights ${Math.min(...d.blocks.map((b) => b.height))}–${Math.max(...d.blocks.map((b) => b.height))}. Choose a range inside those heights.`
			);
		const last = rows.at(-1)!;
		if (requested.end_block_id && requested.end_block_id !== last.id)
			return problem(res, 409, 'Anchor changed', 'The pinned fixture end block differs.');
		const keys = new Map<string, MiningKey>();
		const versions = new Map<number, number>();
		for (const block of rows) {
			const key = keys.get(block.miner_pk) ?? {
				public_key: block.miner_pk,
				block_count: 0,
				first_height: block.height,
				first_block_id: block.id,
				last_height: block.height,
				last_block_id: block.id,
				fees: '0'
			};
			key.block_count++;
			key.last_height = block.height;
			key.last_block_id = block.id;
			key.fees = (BigInt(key.fees) + BigInt(block.fees)).toString();
			keys.set(block.miner_pk, key);
			versions.set(block.version, (versions.get(block.version) ?? 0) + 1);
		}
		const all = [...keys.values()].sort(
			(a, b) => b.block_count - a.block_count || a.public_key.localeCompare(b.public_key)
		);
		const other = all.slice(requested.top);
		const overview: MiningOverview = {
			scope: 'canonical_block_headers',
			consistency: 'single_reader',
			complete: true,
			from_height: requested.from_height,
			to_height: requested.to_height,
			top: requested.top,
			block_count: rows.length,
			indexed_height: d.status.indexed!,
			full_history: false,
			partial_from: Math.min(...d.blocks.map((b) => b.height)),
			anchor: { height: last.height, block_id: last.id },
			totals: {
				fees: rows.reduce((sum, b) => sum + BigInt(b.fees), 0n).toString(),
				transaction_count: rows.reduce((sum, b) => sum + BigInt(b.tx_count), 0n).toString()
			},
			miner_keys: {
				distinct_count: keys.size,
				items: all.slice(0, requested.top),
				other_key_count: other.length,
				other_block_count: other.reduce((sum, k) => sum + k.block_count, 0),
				other_fees: other.reduce((sum, k) => sum + BigInt(k.fees), 0n).toString()
			},
			versions: [...versions]
				.sort(([a], [b]) => a - b)
				.map(([version, block_count]) => ({ version, block_count })),
			// Compact fixture BlockDto records do not carry raw header votes.
			votes: {
				known_blocks: 0,
				unknown_blocks: rows.length,
				zero_vote_blocks: 0,
				distinct_tuples: 0,
				items: [],
				other_tuple_count: 0,
				other_block_count: 0
			}
		};
		return sendJson(res, 200, overview);
	}
	const rewards = /^\/v1\/blocks\/([^/]+)\/rewards$/.exec(path);
	if (rewards) {
		const block = resolveBlock(d, decodeURIComponent(rewards[1]));
		if (!block) return notFound(res);
		return sendJson(res, 200, {
			block_id: block.id,
			height: block.height,
			basis: 'unsupported',
			gross_reward: null,
			reemission_obligation: null,
			miner_subsidy: null,
			transaction_fees: block.fees,
			reward_box_id: null,
			transaction_id: null,
			note: 'No supported EIP-27 emission reward was identified. Fees are separate; storage-rent income is not included.'
		});
	}

	// --- blocks --------------------------------------------------------------------
	if (path === '/v1/blocks') {
		const dir = url.searchParams.get('dir');
		const items = dir === 'asc' ? [...d.blocks].reverse() : d.blocks;
		sendJson(res, 200, page(items, cursor, limit));
		return;
	}

	const blockSummaries = /^\/v1\/blocks\/([^/]+)\/tx-summaries$/.exec(path);
	if (blockSummaries) {
		const block = resolveBlock(d, decodeURIComponent(blockSummaries[1]));
		if (!block) return notFound(res);
		const ids = d.txIdsByHeight.get(block.height) ?? [];
		const summaries = ids.flatMap((id) => {
			const tx = d.txById.get(id);
			return tx ? [txSummary(tx)] : [];
		});
		sendJson(res, 200, page(summaries, cursor, limit));
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

	if (path === '/v1/tx-summaries') {
		sendJson(res, 200, page(d.txs.map(txSummary), cursor, limit));
		return;
	}

	const oneTx = /^\/v1\/txs\/([^/]+)$/.exec(path);
	if (oneTx) {
		const id = decodeURIComponent(oneTx[1]).toLowerCase();
		const tx = appTransactions.get(id) ?? d.txById.get(id);
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
		const id = decodeURIComponent(oneBox[1]).toLowerCase();
		const b = appBoxes.get(id) ?? d.boxById.get(id);
		return b ? sendJson(res, 200, b) : notFound(res);
	}

	// --- addresses -----------------------------------------------------------------
	if (/^\/v1\/addresses\/[^/]+\/(?:(balance|boxes)\/at|balance\/compare)$/.test(path)) {
		return sendJson(res, 503, {
			type: 'about:blank',
			title: 'Historical state unavailable',
			status: 503,
			code: 'history_unavailable',
			detail:
				'This fixture index starts above genesis; exact historical state requires complete retained history.'
		});
	}
	const address = /^\/v1\/addresses\/([^/]+)(\/boxes|\/txs|\/rent|\/activity)?$/.exec(path);
	if (address) {
		const addr = decodeURIComponent(address[1]);
		const tree = d.treeByAddress.get(addr);
		if (tree === undefined) return notFound(res);
		switch (address[2]) {
			case '/activity':
				return addressActivity(d, tree, url, res);
			case '/boxes': {
				const unspent = url.searchParams.get('unspent') === 'true';
				sendJson(res, 200, page(boxesOfTree(d, tree, unspent), cursor, limit));
				return;
			}
			case '/txs':
				sendJson(res, 200, page(txSummariesOfTree(d, tree), cursor, limit));
				return;
			case '/rent': {
				const view = url.searchParams.get('view');
				if (view !== null && view !== 'exposure') return badRequest(res, 'Unknown rent view.');
				if (view === 'exposure') {
					const items = boxesOfTree(d, tree, true)
						.slice(0, 5000)
						.map((box) => ({
							id: box.id,
							value: box.value,
							creation_height: box.creation_height,
							size: box.size,
							token_count: box.tokens.length,
							rent: {
								...box.rent,
								claimable_at_tip:
									d.status.indexed !== null && box.rent.maturity_height <= d.status.indexed
							}
						}));
					const tip = d.blocks.find((b) => b.height === d.status.indexed);
					return sendJson(res, 200, {
						items,
						truncated: false,
						context: {
							scope: 'indexed_unspent_boxes',
							address: addr,
							tree_hash: tree,
							indexed_height: d.status.indexed,
							anchor: tip ? { height: tip.height, block_id: tip.id } : null,
							full_history: false,
							partial_from: Math.min(...d.blocks.map((b) => b.height)),
							scanned_count: items.length,
							scan_limit: 5000,
							scan_complete: true
						}
					});
				}
				sendJson(res, 200, { items: boxesOfTree(d, tree, true), truncated: false });
				return;
			}
			default: {
				const info = d.addresses.get(addr);
				return info ? sendJson(res, 200, info) : notFound(res);
			}
		}
	}

	// --- tokens ------------------------------------------------------------------------
	if (path === '/v1/tokens/search') {
		const q = url.searchParams.get('q') ?? '';
		const normalize = (value: string) =>
			value
				.trim()
				.replace(/[\t\n\v\f\r ]+/g, ' ')
				.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
		const normalized = normalize(q);
		if (!normalized) return badRequest(res, 'q must not be empty');
		const match = url.searchParams.get('match') ?? 'prefix';
		const matches = d.tokens
			.filter(
				(token) =>
					token.name &&
					(match === 'exact'
						? normalize(token.name) === normalized
						: normalize(token.name).startsWith(normalized))
			)
			.sort(
				(a, b) => normalize(a.name).localeCompare(normalize(b.name)) || a.id.localeCompare(b.id)
			);
		const result = page(matches, cursor, limit);
		return sendJson(res, 200, {
			...result,
			next_snapshot: result.next_cursor ? 'mock-name-snapshot' : null,
			search: {
				query: q,
				normalized_query: normalized,
				match,
				index_version: 1,
				coverage: 'complete',
				partial_from: null,
				indexed_names: d.tokens.filter((t) => t.name).length,
				total_tokens: d.tokens.length,
				unindexed_tokens: 0
			}
		});
	}
	if (path === '/v1/tokens') {
		const sort = url.searchParams.get('sort');
		if (sort !== null && sort !== 'newest' && sort !== 'holders') {
			return badRequest(res, `sort must be "newest" or "holders", not ${JSON.stringify(sort)}`);
		}
		sendJson(res, 200, page(sort === 'holders' ? d.tokensByHolders : d.tokens, cursor, limit));
		return;
	}

	const token = /^\/v1\/tokens\/([^/]+)(\/holders|\/boxes|\/txs)?$/.exec(path);
	if (token) {
		const id = decodeURIComponent(token[1]).toLowerCase();
		const info = d.tokenById.get(id);
		if (info === undefined) return notFound(res);
		switch (token[2]) {
			case '/txs':
				return tokenTransactions(d, id, url, res);
			case '/holders': {
				const result = holderPage(d.tokenHolders.get(id) ?? [], cursor, limit);
				const strict = url.searchParams.get('consistency') === 'strict';
				const block = d.blockByHeight.get(d.status.indexed ?? d.status.best)!;
				const anchor = { height: block.height, block_id: block.id };
				const snapshotFor = (next: string) =>
					`mock-holders-${Buffer.from(JSON.stringify([anchor, id, next])).toString('base64url')}`;
				const provided = url.searchParams.get('snapshot');
				if (
					strict &&
					((cursor === null) !== (provided === null) ||
						(cursor !== null && provided !== snapshotFor(cursor)))
				)
					return sendJson(res, 409, {
						status: 409,
						title: 'Snapshot changed',
						code: 'snapshot_changed',
						detail: 'Restart holders with the current snapshot, token and cursor.'
					});
				sendJson(res, 200, {
					...result,
					...(strict
						? {
								consistency: 'strict',
								anchor,
								observed_anchor: anchor,
								next_snapshot: result.next_cursor ? snapshotFor(result.next_cursor) : null
							}
						: {}),
					holder_context: {
						supply: info.supply,
						holder_count: info.holder_count,
						definition: 'indexed_emission_minus_burned'
					}
				});
				return;
			}
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
		if (reg === 'R5' && value.toLowerCase() === rosenFixture.event.registers?.R5) {
			const anchor = { height: 1880000, block_id: 'a'.repeat(64) };
			return sendJson(res, 200, {
				items: [appBox(rosenFixture.event)],
				next_cursor: null,
				next_snapshot: null,
				consistency: 'strict',
				anchor,
				observed_anchor: anchor
			});
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
		sendJson(res, 200, {
			items: d.rentUpcoming.slice(0, limit),
			next_cursor: null,
			complete: d.rentUpcoming.length <= limit
		});
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
