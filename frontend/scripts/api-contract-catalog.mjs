// Public read API subset supported by the portable SDK. Paths match the Rust router.
const id = { type: 'string', pattern: '^[a-fA-F0-9]{64}$' };
const text = { type: 'string', maxLength: 4096 };
const height = { type: 'integer', minimum: 0, maximum: 4294967295 };
const address = { type: 'string', minLength: 1, maxLength: 4096 };
const page = {
	limit: { type: 'integer', minimum: 1, maximum: 500 },
	cursor: text,
	snapshot: text,
	dir: { type: 'string', enum: ['asc', 'desc'] }
};
const pathId = { id };
const pathAddress = { addr: address };
const block = { height_or_id: { anyOf: [height, id] } };
const at = { height, block_id: id };
const op = (path, response, parameters = {}, required = [], extra = {}) => ({
	path,
	response,
	parameters,
	required,
	...extra
});
export const operations = {
	status: op('/status', 'StatusDto'),
	mempool: op('/mempool', 'MempoolSnapshot'),
	mining: op(
		'/mining',
		'MiningOverview',
		{
			from_height: { ...height, minimum: 1 },
			to_height: { ...height, minimum: 1 },
			top: { type: 'integer', minimum: 1, maximum: 50 },
			end_block_id: id
		},
		['from_height', 'to_height']
	),
	network: op('/network/summary', 'NetworkSummary'),
	networkHistory: op(
		'/network/history',
		'NetworkHistory',
		{
			from_height: { ...height, minimum: 1 },
			to_height: { ...height, minimum: 1 },
			buckets: { type: 'integer', minimum: 1, maximum: 120 },
			end_block_id: id
		},
		['from_height', 'to_height']
	),
	supply: op('/supply', 'SupplyDto'),
	blocks: op(
		'/blocks',
		'PageDto<BlockDto>',
		{ ...page, dir: { type: 'string', enum: ['desc'] } },
		[],
		{ paged: true }
	),
	block: op('/blocks/{height_or_id}', 'BlockDto', block, ['height_or_id']),
	rewards: op('/blocks/{height_or_id}/rewards', 'RewardBreakdown', block, ['height_or_id']),
	transaction: op('/txs/{id}', 'TxDto', pathId, ['id']),
	transactionStatus: op('/txs/{id}/status', 'TxStatusDto', pathId, ['id']),
	transactionEvidence: op('/txs/{id}/evidence', 'TxEvidence', pathId, ['id']),
	transactions: op('/tx-summaries', 'PageDto<TxSummaryDto>', page, [], { paged: true }),
	blockTransactions: op(
		'/blocks/{height_or_id}/tx-summaries',
		'PageDto<TxSummaryDto>',
		{ ...block, ...page },
		['height_or_id'],
		{ paged: true }
	),
	box: op('/boxes/{id}', 'BoxDto', pathId, ['id']),
	address: op('/addresses/{addr}', 'AddressDto', pathAddress, ['addr']),
	addressRentExposure: op(
		'/addresses/{addr}/rent',
		'RentExposure',
		{
			...pathAddress,
			view: { type: 'string', enum: ['exposure'] }
		},
		['addr', 'view']
	),
	rentSchedule: op(
		'/rent/schedule',
		'RentSchedulePageDto',
		{
			window: { type: 'string', enum: ['24h', '7d', '30d', '90d'] },
			mode: { type: 'string', enum: ['all', 'collectible', 'full_claim'] },
			token_id: id,
			limit: { type: 'integer', minimum: 1, maximum: 100 },
			cursor: text,
			snapshot: text
		},
		[],
		{ paged: true, strictOnly: true }
	),
	addressActivity: op(
		'/addresses/{addr}/activity',
		'AddressActivityPageDto',
		{
			...pathAddress,
			...page,
			limit: { type: 'integer', minimum: 1, maximum: 100 },
			asset: { anyOf: [id, { type: 'string', enum: ['all', 'erg'] }] },
			direction: {
				type: 'string',
				enum: ['all', 'received', 'sent', 'mixed', 'neutral', 'unknown']
			},
			from_ms: { type: 'integer', minimum: 0, maximum: Number.MAX_SAFE_INTEGER },
			to_ms: { type: 'integer', minimum: 0, maximum: Number.MAX_SAFE_INTEGER }
		},
		['addr'],
		{ paged: true, strictOnly: true }
	),
	groupBalances: op(
		'/addresses/balances',
		'GroupBalances',
		{
			addresses: { type: 'array', minItems: 1, maxItems: 100, items: address }
		},
		['addresses'],
		{ method: 'POST' }
	),
	historicalBalance: op(
		'/addresses/{addr}/balance/at',
		'HistoryBalance',
		{ ...pathAddress, ...at },
		['addr', 'height']
	),
	historicalBoxes: op(
		'/addresses/{addr}/boxes/at',
		'HistoryBoxes',
		{ ...pathAddress, ...at, cursor: text, limit: page.limit },
		['addr', 'height']
	),
	compareBalances: op(
		'/addresses/{addr}/balance/compare',
		'HistoryComparison',
		{
			...pathAddress,
			from_height: height,
			to_height: height,
			from_block_id: id,
			to_block_id: id
		},
		['addr', 'from_height', 'to_height']
	),
	token: op('/tokens/{id}', 'TokenInfoDto', pathId, ['id']),
	tokenHolders: op(
		'/tokens/{id}/holders',
		'TokenHoldersPageDto',
		{ ...pathId, limit: page.limit, cursor: text, snapshot: text },
		['id'],
		{ paged: true }
	),
	searchTokens: op(
		'/tokens/search',
		'TokenSearchDto',
		{
			q: {
				type: 'string',
				minLength: 1,
				maxLength: 512,
				'x-max-utf8-bytes': 512,
				description:
					'At most 512 UTF-8 bytes. ASCII whitespace is trimmed and collapsed; ASCII letters are lowercased. Non-ASCII text is matched exactly.'
			},
			match: { type: 'string', enum: ['prefix', 'exact'] },
			limit: page.limit,
			cursor: text,
			snapshot: text
		},
		['q'],
		{ paged: true }
	)
};

export const sourceFiles = [
	'src/lib/api/types.ts',
	'src/lib/home/api.ts',
	'src/lib/blocks/rewards.ts',
	'src/lib/addresses/groups.ts',
	'src/lib/addresses/history.ts',
	'src/lib/network/history.ts',
	'src/lib/mempool/observations.ts',
	'src/lib/mining/overview.ts',
	'src/lib/rent/exposure.ts'
];
