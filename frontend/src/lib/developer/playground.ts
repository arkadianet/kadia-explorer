/** A curated GET-only request catalog, kept against the current Rust handlers. */
export interface Field {
	name: string;
	label: string;
	kind: 'id' | 'block' | 'address' | 'height' | 'limit' | 'text' | 'asset' | 'choice';
	required?: boolean;
	initial?: string;
	options?: string[];
	max?: number;
	minimum?: number;
	maximum?: number;
}
export interface Endpoint {
	id: string;
	name: string;
	group: string;
	path: string;
	description: string;
	fields: Field[];
	strict?: boolean;
	note: string;
}
const id: Field = { name: 'id', label: 'ID', kind: 'id', required: true };
const block: Field = { name: 'block', label: 'Block height or ID', kind: 'block', required: true };
const address: Field = { name: 'address', label: 'Address', kind: 'address', required: true };
const limit: Field = { name: 'limit', label: 'Page size', kind: 'limit', initial: '20' };
const order: Field = {
	name: 'dir',
	label: 'Order',
	kind: 'choice',
	initial: 'desc',
	options: ['desc', 'asc']
};
const cursor: Field = { name: 'cursor', label: 'Cursor', kind: 'text', max: 4096 };
const snapshot: Field = { name: 'snapshot', label: 'Snapshot token', kind: 'text', max: 4096 };
const paging = [limit, cursor, snapshot];
const amounts =
	'Amounts are decimal strings in raw units. Keep them as strings or use BigInt; do not parse them as floating-point numbers.';
const strictNote =
	'Send next_cursor and next_snapshot together. A 409 means the anchor is no longer usable: restart from the first page. An empty page may still have a continuation.';
export const ENDPOINTS: Endpoint[] = [
	{
		id: 'rent-schedule',
		name: 'Upcoming storage rent',
		group: 'Network',
		path: '/rent/schedule',
		strict: true,
		fields: [
			{
				name: 'window',
				label: 'Time window',
				kind: 'choice',
				initial: '24h',
				options: ['24h', '7d', '30d', '90d']
			},
			{
				name: 'mode',
				label: 'Claim mode',
				kind: 'choice',
				initial: 'all',
				options: ['all', 'collectible', 'full_claim']
			},
			{ name: 'token_id', label: 'Token ID (optional)', kind: 'id' },
			{ ...limit, maximum: 100 },
			cursor,
			snapshot
		],
		description:
			'Soonest-first unspent boxes with estimated maturity, exact rent and token details.',
		note:
			'First-page hourly totals scan independently of the item cap. Preserve batch_complete and history coverage; a partial overview is not the whole window. Full-claim candidates do not establish transaction validity. Dates use the indexed header and 120-second block target. ' +
			strictNote +
			' ' +
			amounts
	},
	{
		id: 'mempool',
		name: 'Pending transactions',
		group: 'Network',
		path: '/mempool',
		fields: [],
		description: 'Up to 100 pending summaries and connections within the returned snapshot.',
		note: 'One configured node, not network-wide coverage. A missing parent is not proof of a blocked transaction. Preserve observation time, limits and optional connection coverage.'
	},
	...(['network-history', 'mining'] as const).map((kind): Endpoint => ({
		id: kind,
		name: kind === 'mining' ? 'Mining and header signals' : 'Network history',
		group: 'Network',
		path: kind === 'mining' ? '/mining' : '/network/history',
		fields: [
			{ name: 'from_height', label: 'First height', kind: 'height', minimum: 1, required: true },
			{ name: 'to_height', label: 'Last height', kind: 'height', minimum: 1, required: true },
			kind === 'mining'
				? {
						name: 'top',
						label: 'Maximum keys and vote tuples',
						kind: 'height',
						minimum: 1,
						maximum: 50,
						initial: '20'
					}
				: {
						name: 'buckets',
						label: 'Maximum chart buckets',
						kind: 'height',
						minimum: 1,
						maximum: 120,
						initial: '24'
					},
			{ name: 'end_block_id', label: 'Pinned last block ID (optional)', kind: 'id' }
		],
		description:
			kind === 'mining'
				? 'Mining keys, versions and raw vote tuples across canonical headers.'
				: 'Exact activity and fee totals across canonical headers.',
		note:
			'One reader, at most 20,160 blocks per request. A changed pin returns 409. Missing or resource-limited coverage returns an error. ' +
			(kind === 'mining'
				? 'Keys do not establish pool ownership; raw votes are not approved protocol changes.'
				: amounts)
	})),
	{
		id: 'rent-exposure',
		name: 'Address rent exposure',
		group: 'Addresses',
		path: '/addresses/{address}/rent',
		fields: [
			address,
			{
				name: 'view',
				label: 'Response view',
				kind: 'choice',
				options: ['exposure'],
				initial: 'exposure',
				required: true
			}
		],
		description: 'Compact unspent box evidence with indexed height and scan coverage.',
		note: 'Preserve full_history, scan_complete, truncated and anchor. A partial scan is not the full address balance. Nominal due does not establish collectible value.'
	},
	{
		id: 'status',
		name: 'Index status',
		group: 'Network',
		path: '/status',
		fields: [],
		description: 'Indexed height, configured source observation and service health.',
		note: 'The source is one configured node. Its observation is not a network-wide health guarantee.'
	},
	{
		id: 'network',
		name: 'Network summary',
		group: 'Network',
		path: '/network/summary',
		fields: [],
		description: 'A compact overview of the latest 720 indexed blocks.',
		note: 'Totals describe sampled blocks. The timestamp buckets do not establish complete 24-hour coverage.'
	},
	{
		id: 'blocks',
		name: 'Block list',
		group: 'Network',
		path: '/blocks',
		fields: paging,
		strict: true,
		description: 'Newest indexed blocks with a strict snapshot.',
		note:
			'The legacy reward field is an emission estimate. Use the reward-evidence endpoint for a supported observed breakdown. ' +
			strictNote
	},
	{
		id: 'block',
		name: 'Block details',
		group: 'Network',
		path: '/blocks/{block}',
		fields: [block],
		description: 'Canonical block facts by height or header ID.',
		note: 'A height can identify a different block after a reorganization. Record the returned block ID.'
	},
	{
		id: 'rewards',
		name: 'Observed block rewards',
		group: 'Network',
		path: '/blocks/{block}/rewards',
		fields: [block],
		description: 'Observed EIP-27 gross reward, re-emission obligation and miner subsidy.',
		note: 'Unsupported contract shapes return null amounts. Fees are separate; this is not total miner income.'
	},
	{
		id: 'supply',
		name: 'ERG supply',
		group: 'Network',
		path: '/supply',
		fields: [],
		description: 'Supply definitions and indexed reserve evidence.',
		note: 'Gross reserves are not an estimate of liquid or circulating supply. Read the definitions in the response.'
	},
	{
		id: 'summaries',
		name: 'Transaction summaries',
		group: 'Transactions',
		path: '/tx-summaries',
		fields: [order, ...paging],
		strict: true,
		description: 'Transaction counts and fees without input/output expansion.',
		note: amounts + ' ' + strictNote
	},
	{
		id: 'block-transactions',
		name: 'Block transactions',
		group: 'Transactions',
		path: '/blocks/{block}/tx-summaries',
		fields: [block, order, ...paging],
		strict: true,
		description: 'Paged summaries within one block.',
		note: strictNote
	},
	{
		id: 'transaction',
		name: 'Transaction details',
		group: 'Transactions',
		path: '/txs/{id}',
		fields: [id],
		description: 'Expanded inputs and outputs for one indexed transaction.',
		note:
			'Large expansions can return 422. Missing input boxes in a partial index must not be treated as zero. ' +
			amounts
	},
	{
		id: 'transaction-status',
		name: 'Transaction status',
		group: 'Transactions',
		path: '/txs/{id}/status',
		fields: [id],
		description: 'Indexed inclusion and local node mempool observations.',
		note: 'Not observed does not mean rejected. Observation history is process-local and limited to requested transactions.'
	},
	{
		id: 'box',
		name: 'Box details',
		group: 'Transactions',
		path: '/boxes/{id}',
		fields: [id],
		description: 'Value, tokens, registers, producing transaction and indexed spending reference.',
		note:
			'creation_height is a box field, not necessarily its inclusion height. Unspent is an indexed observation. ' +
			amounts
	},
	{
		id: 'address',
		name: 'Address overview',
		group: 'Addresses',
		path: '/addresses/{address}',
		fields: [address],
		description: 'Current indexed balance and activity counts for a locking script.',
		note: 'An address does not establish beneficial ownership. ' + amounts
	},
	{
		id: 'activity',
		name: 'Address activity',
		group: 'Addresses',
		path: '/addresses/{address}/activity',
		fields: [
			address,
			{ name: 'asset', label: 'Asset (all, erg or token ID)', kind: 'asset', initial: 'all' },
			{
				name: 'direction',
				label: 'Direction',
				kind: 'choice',
				options: ['all', 'received', 'sent', 'mixed', 'neutral', 'unknown'],
				initial: 'all'
			},
			order,
			...paging
		],
		strict: true,
		description: 'Exact per-address ERG and token changes with input-coverage evidence.',
		note:
			'Unknown inputs withhold net amounts. Direction refers to the selected asset; fees are not deducted a second time. Any tip change requires a new walk. ' +
			strictNote
	},
	{
		id: 'balance-at',
		name: 'Historical address balance',
		group: 'Addresses',
		path: '/addresses/{address}/balance/at',
		fields: [
			address,
			{ name: 'height', label: 'End-of-block height', kind: 'height', required: true },
			{ name: 'block_id', label: 'Pinned block ID (optional)', kind: 'id' }
		],
		description: 'Exact unspent balance at the end of a specified block.',
		note: 'Requires complete retained history. A resource-limited scan returns 422 rather than a partial monetary answer. Genesis height 0 has no block ID.'
	},
	{
		id: 'token',
		name: 'Token identity',
		group: 'Tokens',
		path: '/tokens/{id}',
		fields: [id],
		description: 'Mint references, declared metadata and indexed supply.',
		note:
			'A matching name is not authentication. Supply is indexed emission minus burns, not circulating supply. ' +
			amounts
	},
	{
		id: 'token-search',
		name: 'Token name search',
		group: 'Tokens',
		path: '/tokens/search',
		fields: [
			{ name: 'q', label: 'Token name', kind: 'text', required: true, max: 128 },
			{
				name: 'match',
				label: 'Name match',
				kind: 'choice',
				options: ['prefix', 'exact'],
				initial: 'prefix'
			},
			...paging
		],
		strict: true,
		description: 'Discover token names with explicit index coverage.',
		note:
			'Read search.coverage; incomplete names or a preparing index must not become a claim that a token does not exist. ' +
			strictNote
	},
	{
		id: 'holders',
		name: 'Token holders',
		group: 'Tokens',
		path: '/tokens/{id}/holders',
		fields: [id, ...paging],
		strict: true,
		description: 'Locking-script balances and a same-snapshot supply denominator.',
		note:
			'Scripts are not necessarily individual owners. Read holder_context before calculating shares. ' +
			strictNote
	},
	{
		id: 'token-history',
		name: 'Token transactions',
		group: 'Tokens',
		path: '/tokens/{id}/txs',
		fields: [id, order, ...paging],
		strict: true,
		description: 'Indexed token involvement including mint, burn, change and contract movements.',
		note:
			'This is transaction membership, not a transfer amount. A rebuilding index returns 503 token_history_preparing. ' +
			strictNote
	},
	{
		id: 'rent',
		name: 'Upcoming storage rent',
		group: 'Protocol',
		path: '/rent/upcoming',
		fields: [{ name: 'blocks', label: 'Look-ahead blocks', kind: 'height', initial: '720' }, limit],
		description: 'A bounded preview of boxes approaching storage-rent maturity.',
		note: 'Read complete: false means a lower-bound preview even when next_cursor is null. Nominal due is not a collectible opportunity; inspect collectible and consensus_fee_nano.'
	}
];

export function endpointById(key: string): Endpoint {
	const endpoint = ENDPOINTS.find((entry) => entry.id === key);
	if (!endpoint) throw new Error('Choose a supported endpoint.');
	return endpoint;
}
export function defaults(endpoint: Endpoint): Record<string, string> {
	return Object.fromEntries(endpoint.fields.map((field) => [field.name, field.initial ?? '']));
}
function validate(field: Field, raw: string): string {
	const value = raw.trim();
	if (!value) {
		if (field.required) throw new Error(`${field.label} is required.`);
		return '';
	}
	let valid =
		value.length <= (field.max ?? 4096) &&
		![...value].some((char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127);
	const height = /^\d{1,10}$/.test(value) && Number(value) <= 4_294_967_295;
	const hash = /^[a-fA-F0-9]{64}$/.test(value);
	if (field.kind === 'id') valid &&= hash;
	if (field.kind === 'block') valid &&= height || hash;
	if (field.kind === 'height')
		valid &&=
			height &&
			Number(value) >= (field.minimum ?? 0) &&
			Number(value) <= (field.maximum ?? 4_294_967_295);
	if (field.kind === 'limit')
		valid &&= /^\d{1,3}$/.test(value) && Number(value) >= 1 && Number(value) <= 100;
	if (field.kind === 'address') valid &&= /^[1-9A-HJ-NP-Za-km-z]{7,4096}$/.test(value);
	if (field.kind === 'asset') valid &&= value === 'all' || value === 'erg' || hash;
	if (field.kind === 'choice') valid &&= field.options?.includes(value) ?? false;
	if (!valid)
		throw new Error(
			`${field.label} has an invalid value${field.kind === 'limit' ? '; use 1 through 100' : ''}.`
		);
	return value;
}
export function buildRequest(key: string, values: Record<string, string>): string {
	const endpoint = endpointById(key);
	let path = endpoint.path;
	const params = new URLSearchParams();
	for (const field of endpoint.fields) {
		const value = validate(field, values[field.name] ?? field.initial ?? '');
		if (path.includes(`{${field.name}}`))
			path = path.replace(`{${field.name}}`, encodeURIComponent(value));
		else if (value) params.set(field.name, value);
	}
	if (endpoint.strict) {
		if (params.has('cursor') !== params.has('snapshot'))
			throw new Error(
				'Send the cursor and snapshot token together, or leave both empty for the first page.'
			);
		params.set('consistency', 'strict');
	}
	if (key === 'network-history' || key === 'mining') {
		const from = Number(params.get('from_height'));
		const to = Number(params.get('to_height'));
		if (from > to) throw new Error('First height must not exceed last height.');
		if (to - from >= 20160) throw new Error('Choose at most 20,160 blocks per request.');
	}
	return `/v1${path}${params.size ? `?${params}` : ''}`;
}
export function sharePath(key: string, values: Record<string, string>): string {
	buildRequest(key, values);
	const params = new URLSearchParams({ endpoint: key });
	for (const field of endpointById(key).fields) {
		const value = values[field.name]?.trim();
		if (value) params.set(`p.${field.name}`, value);
	}
	return `/developers?${params}`;
}
export function readShare(params: URLSearchParams) {
	const key = params.get('endpoint') ?? 'status';
	const endpoint = endpointById(key);
	const values = defaults(endpoint);
	for (const field of endpoint.fields) {
		const value = params.get(`p.${field.name}`);
		if (value !== null) values[field.name] = validate(field, value);
	}
	return { key, values };
}
export function curlCommand(origin: string, path: string): string {
	const url = new URL(origin);
	if (
		!['http:', 'https:'].includes(url.protocol) ||
		!path.startsWith('/v1/') ||
		/[\r\n]/.test(path)
	)
		throw new Error('Invalid API URL.');
	const target = new URL(path, url.origin).href;
	// POSIX-shell quoting, including token names containing apostrophes.
	return `curl --fail-with-body --header 'Accept: application/json' '${target.replaceAll("'", "'\\''")}'`;
}
export const RESPONSE_LIMIT = 2 * 1024 * 1024;
export interface PlaygroundResult {
	status: number;
	ok: boolean;
	text: string;
	data: unknown;
	bytes: number;
	elapsed: number;
}
export async function executeRequest(
	key: string,
	values: Record<string, string>,
	signal: AbortSignal,
	fetcher: typeof fetch = fetch
): Promise<PlaygroundResult> {
	const path = buildRequest(key, values);
	const start = performance.now();
	const response = await fetcher(path, {
		method: 'GET',
		signal,
		credentials: 'omit',
		redirect: 'error',
		cache: 'no-store',
		headers: { Accept: 'application/json' }
	});
	const reader = response.body?.getReader();
	if (!reader) throw new Error('The server returned an empty response body.');
	const decoder = new TextDecoder();
	let bytes = 0;
	let text = '';
	try {
		if (Number(response.headers.get('content-length')) > RESPONSE_LIMIT)
			throw new Error('Response exceeds the 2 MiB playground limit. Reduce the page size.');
		for (;;) {
			const chunk = await reader.read();
			if (chunk.done) break;
			bytes += chunk.value.byteLength;
			if (bytes > RESPONSE_LIMIT)
				throw new Error('Response exceeds the 2 MiB playground limit. Reduce the page size.');
			text += decoder.decode(chunk.value, { stream: true });
		}
		text += decoder.decode();
		let data: unknown;
		try {
			data = JSON.parse(text);
		} catch {
			throw new Error(`HTTP ${response.status}: the server did not return valid JSON.`);
		}
		return {
			status: response.status,
			ok: response.ok,
			text,
			data,
			bytes,
			elapsed: Math.round(performance.now() - start)
		};
	} finally {
		await reader.cancel().catch(() => {});
		reader.releaseLock();
	}
}
export function continuation(data: unknown): { cursor: string; snapshot: string } | null {
	if (
		!data ||
		typeof data !== 'object' ||
		!('next_cursor' in data) ||
		!('next_snapshot' in data) ||
		!('consistency' in data) ||
		data.consistency !== 'strict'
	)
		return null;
	if (
		typeof data.next_cursor !== 'string' ||
		!data.next_cursor ||
		data.next_cursor.length > 4096 ||
		typeof data.next_snapshot !== 'string' ||
		!data.next_snapshot ||
		data.next_snapshot.length > 4096
	)
		return null;
	return { cursor: data.next_cursor, snapshot: data.next_snapshot };
}
