// Generated from explorer wire types. Amounts stay decimal strings.
export interface StatusDto {
	/** Last successful /info response; does not certify source health. */
	source_observed_at_ms?: number | null;
	source_error?: string | null;
	indexed: number | null;
	best: number;
	mode: 'bulk' | 'tip';
	source: string;
	halted: string | null;
	lag_blocks: number;
	stalled: StatusStalledDto | null;
	inflight_reads: number;
	rate_limited_total: number;
}
export interface StatusStalledDto {
	height: number;
	since_secs: number;
	reason: string;
}
export interface MempoolSnapshot {
	scope: 'configured_node_mempool';
	source: 'configured_primary_node';
	checked_at_ms: number;
	expires_at_ms: number;
	cached: boolean;
	limit: 100;
	limit_reached: boolean;
	observed_count: number;
	items: PendingTransaction[];
	/** Absent on older API versions; absence is not evidence of zero connections. */
	connections?: PendingConnections;
}
export interface PendingTransaction {
	id: string;
	input_count: number;
	data_input_count: number;
	output_count: number;
	size: number | null;
	fee: string | null;
}
export interface PendingConnections {
	scope: 'returned_snapshot_only';
	output_count: number;
	identified_output_count: number;
	edge_count: number;
	edges_truncated: boolean;
	edges: PendingConnection[];
	shared_input_count: number;
	shared_inputs_truncated: boolean;
	shared_inputs: SharedPendingInput[];
}
export interface PendingConnection {
	producer_id: string;
	consumer_id: string;
	box_id: string;
	kind: 'spend' | 'read';
}
export interface SharedPendingInput {
	box_id: string;
	transaction_count: number;
	transaction_ids: string[];
	truncated: boolean;
}
export interface MiningOverview {
	scope: 'canonical_block_headers';
	consistency: 'single_reader';
	complete: true;
	from_height: number;
	to_height: number;
	block_count: number;
	indexed_height: number;
	full_history: boolean;
	partial_from: number | null;
	anchor: {
		height: number;
		block_id: string;
	};
	top: number;
	totals: {
		fees: string;
		transaction_count: string;
	};
	miner_keys: {
		distinct_count: number;
		items: MiningKey[];
		other_key_count: number;
		other_block_count: number;
		other_fees: string;
	};
	versions: {
		version: number;
		block_count: number;
	}[];
	votes: {
		known_blocks: number;
		unknown_blocks: number;
		zero_vote_blocks: number;
		distinct_tuples: number;
		items: {
			votes: string;
			block_count: number;
		}[];
		other_tuple_count: number;
		other_block_count: number;
	};
}
export interface MiningKey {
	public_key: string;
	block_count: number;
	first_height: number;
	last_height: number;
	first_block_id: string;
	last_block_id: string;
	fees: string;
}
export interface NetworkSummary {
	scope: 'latest_indexed_blocks';
	requested_blocks: number;
	block_count: number;
	transaction_count: number;
	fees: string;
	from_height: number | null;
	to_height: number | null;
	anchor_id: string | null;
	earliest_timestamp: number | null;
	latest_timestamp: number | null;
	partial_from: number | null;
	recent_blocks: BlockDto[];
	blocks_per_hour: number[];
	transactions_per_hour: number[];
	fees_per_hour: string[];
	blocks_per_ten_minutes: number[];
}
export interface BlockDto {
	id: string;
	height: number;
	parent_id: string;
	timestamp: number;
	difficulty: string;
	miner_pk: string;
	tx_count: number;
	size: number;
	fees: string;
	reward: string;
	version: number;
}
export interface NetworkHistory {
	scope: 'canonical_block_headers';
	consistency: 'single_reader';
	complete: true;
	full_history: boolean;
	partial_from: number | null;
	indexed_height: number;
	anchor: {
		height: number;
		block_id: string;
	};
	requested_buckets: number;
	bucket_width: number;
	totals: NetworkBucket;
	buckets: NetworkBucket[];
}
export interface NetworkBucket {
	from_height: number;
	to_height: number;
	block_count: number;
	transaction_count: string;
	fees: string;
	size_bytes: string;
	difficulty_min: string;
	difficulty_max: string;
	difficulty_end: string;
	first_timestamp: number;
	last_timestamp: number;
	earliest_timestamp: number;
	latest_timestamp: number;
}
export interface SupplyDto {
	indexed_height: number | null;
	emission_remaining_nano: string | null;
	/** Deprecated alias for outside_emission_nano; not circulating supply. */
	emitted_nano: string | null;
	outside_emission_nano: string | null;
	circulating_nano: null;
	definition: 'genesis_allocation_minus_original_emission_reserve';
	genesis_total_nano: string;
	complete: boolean;
}
export interface PageDto<T> {
	items: T[];
	next_cursor: string | null;
	next_snapshot?: string | null;
	consistency?: 'strict' | 'best_effort';
	anchor?: {
		height: number;
		block_id: string;
	} | null;
	observed_anchor?: {
		height: number;
		block_id: string;
	} | null;
}
export interface RewardBreakdown {
	block_id: string;
	height: number;
	basis: 'observed_eip27_reward_box' | 'unsupported';
	gross_reward: string | null;
	reemission_obligation: string | null;
	miner_subsidy: string | null;
	transaction_fees: string;
	reward_box_id: string | null;
	transaction_id: string | null;
	note: string;
}
export interface TxDto {
	/** Snapshot fields on the detail route; older servers and list routes omit these. */
	block_id?: string;
	indexed_height?: number | null;
	confirmations?: number | null;
	id: string;
	height: number;
	index: number;
	timestamp: number;
	size: number;
	fee: string;
	inputs: InputDto[];
	data_inputs: string[];
	outputs: BoxDto[];
}
export interface InputDto {
	id: string;
	box: BoxDto | null;
}
export interface BoxDto {
	kind?: 'box' | 'fee' | 'emission';
	id: string;
	tx_id: string;
	index: number;
	value: string;
	creation_height: number;
	ergo_tree: string | null;
	address: string | null;
	template_hash: string | null;
	tree_hash: string;
	tokens: TokenDto[];
	registers: Record<string, unknown> | null;
	size: number;
	spent_by: string | null;
	spent_height: number | null;
	rent: RentDto;
}
export interface TokenDto {
	id: string;
	amount: string;
	name: string | null;
	decimals: number | null;
}
export interface RentDto {
	maturity_height: number;
	/** Nominal rent. Not a collectible opportunity on its own — read with `collectible`. */
	due_nano: string;
	claimable_at_tip: boolean;
	/** Storage fee as consensus computes it (wrapping i32), so negative above 1,717 bytes. */
	consensus_fee_nano: string;
	/** False when the consensus fee is not positive: the box cannot be rent-claimed at all. */
	collectible: boolean;
}
export interface TxStatusDto {
	id: string;
	state:
		'confirmed' | 'pending' | 'not_observed' | 'no_longer_observed' | 'unavailable' | 'conflicted';
	checked_at_ms: number;
	indexed_height: number | null;
	inclusion: {
		block_id: string;
		height: number;
		confirmations: number;
	} | null;
	previous_inclusion: {
		block_id: string;
		height: number;
	} | null;
	mempool: {
		observation: 'present' | 'absent' | 'unavailable' | 'not_checked';
		checked_at_ms: number | null;
		first_seen_at_ms: number | null;
		last_seen_at_ms: number | null;
		error: 'unsupported' | 'unavailable' | 'busy' | null;
	};
	pending: {
		input_count: number;
		output_count: number;
		data_input_count: number;
		size: number | null;
		fee: string | null;
		/** Older servers return summary counts only. */
		details?: PendingTxDetails;
	} | null;
	conflicts: {
		input_id: string;
		tx_id: string;
		height: number;
	}[];
	history_scope: 'process_local_requested_transactions';
	retention_seconds: number;
}
export interface PendingTxDetails {
	inputs: string[];
	data_inputs: string[];
	outputs: {
		index: number;
		id: string | null;
		value: string;
		ergo_tree: string | null;
		/** Derived with the same mainnet encoder as confirmed boxes when the script is supported. */
		address?: string | null;
		tokens: {
			id: string;
			amount: string;
		}[];
		token_count: number | null;
		tokens_truncated: boolean;
		ergo_tree_truncated: boolean;
	}[];
	inputs_truncated: boolean;
	data_inputs_truncated: boolean;
	outputs_truncated: boolean;
	/** Completeness of this projection, never proof of network acceptance. */
	complete: boolean;
}
export interface TxEvidence {
	tx_id: string;
	block_id: string;
	height: number;
	assurance: 'trusted_node_response';
	inputs: {
		id: string;
		proof: 'empty' | 'nonempty';
		extension_127: string | null;
	}[];
}
export interface TxSummaryDto {
	id: string;
	height: number;
	index: number;
	timestamp: number;
	size: number;
	fee: string;
	input_count: number;
	data_input_count: number;
	output_count: number;
}
export interface AddressDto {
	address: string;
	tree_hash: string;
	balance: BalanceDto;
	box_count: number;
	tx_count: number;
	first_seen: number;
	last_seen: number;
}
export interface BalanceDto {
	nano: string;
	tokens: TokenDto[];
}
export interface RentExposure {
	items: RentExposureBox[];
	truncated: boolean;
	context: {
		scope: 'indexed_unspent_boxes';
		address: string;
		tree_hash: string;
		indexed_height: number | null;
		anchor: {
			height: number;
			block_id: string;
		} | null;
		full_history: boolean;
		partial_from: number | null;
		scanned_count: number;
		scan_limit: number;
		scan_complete: boolean;
	};
}
export interface RentExposureBox {
	id: string;
	value: string;
	creation_height: number;
	size: number;
	token_count: number;
	rent: {
		maturity_height: number;
		due_nano: string;
		claimable_at_tip: boolean;
		consensus_fee_nano: string;
		collectible: boolean;
	};
}
export interface AddressActivityPageDto extends PageDto<AddressActivityDto> {
	scanned: number;
	scan_limit_reached: boolean;
	partial_from: number | null;
}
export interface AddressActivityDto {
	id: string;
	height: number;
	block_id: string;
	timestamp: number;
	index: number;
	fee: string;
	input_count: number;
	output_count: number;
	coverage: {
		complete: boolean;
		resolved_inputs: number;
		total_inputs: number;
	};
	erg_delta: string | null;
	tokens: {
		id: string;
		name: string | null;
		decimals: number | null;
		delta: string | null;
	}[];
	direction: ActivityDirection;
	asset_match: 'definite' | 'uncertain';
}
export type ActivityDirection = 'received' | 'sent' | 'mixed' | 'neutral' | 'unknown';
export interface GroupBalances {
	scope: 'selected_address_scripts';
	consistency: 'single_reader';
	indexed_height: number | null;
	anchor: {
		height: number;
		block_id: string;
	} | null;
	full_history: boolean;
	partial_from: number | null;
	complete: boolean;
	requested_count: number;
	resolved_script_count: number;
	members: GroupMember[];
	observed_totals: GroupBalance | null;
}
export interface GroupMember {
	address: string;
	tree_hash: string | null;
	status: 'resolved' | 'unseen' | 'invalid' | 'duplicate';
	duplicate_of: number | null;
	balance: GroupBalance | null;
}
export interface GroupBalance {
	nano: string;
	tokens: GroupToken[];
}
export interface GroupToken {
	id: string;
	amount: string;
}
export interface HistoryBalance {
	address: string;
	at: HistoryAnchor;
	indexed_height: number | null;
	balance: {
		nano: string;
		box_count: number;
		tokens: HistoryToken[];
	};
	complete: boolean;
}
export interface HistoryAnchor {
	height: number;
	block_id: string | null;
}
export interface HistoryToken {
	token_id: string;
	amount: string;
}
export interface HistoryBoxes {
	at: HistoryAnchor;
	items: HistoryBox[];
	next_cursor: string | null;
}
export interface HistoryBox {
	box_id: string;
	inclusion_height: number;
	nano: string;
	tokens: HistoryToken[];
}
export interface HistoryComparison {
	address: string;
	indexed_height: number | null;
	complete: boolean;
	from: {
		at: HistoryAnchor;
		balance: HistoryBalance['balance'];
	};
	to: {
		at: HistoryAnchor;
		balance: HistoryBalance['balance'];
	};
}
export interface TokenInfoDto {
	id: string;
	name: string;
	description: string;
	decimals: number | null;
	token_type: string | null;
	kind: TokenKind;
	emission: string;
	burned: string;
	supply: string;
	holder_count: number;
	box_count: number;
	mint_tx: string;
	mint_box: string;
	mint_height: number;
}
export type TokenKind = 'nft-picture' | 'nft-audio' | 'nft-video' | 'membership' | 'token';
export interface TokenHoldersPageDto extends PageDto<TokenHolderDto> {
	holder_context?: TokenHolderContext;
}
export interface TokenHolderContext {
	supply: string;
	holder_count: number;
	definition: 'indexed_emission_minus_burned';
}
export interface TokenHolderDto {
	address: string | null;
	tree_hash: string;
	amount: string;
	share_pct: string;
}
export interface TokenSearchDto extends PageDto<TokenInfoDto> {
	search: {
		query: string;
		normalized_query: string;
		match: 'prefix' | 'exact';
		index_version: 1;
		coverage: 'complete' | 'partial';
		partial_from: number | null;
		indexed_names: number;
		total_tokens: number;
		unindexed_tokens: number;
	};
}

export interface KadiaOperations {
	status: { parameters: Record<string, never>; response: StatusDto };
	mempool: { parameters: Record<string, never>; response: MempoolSnapshot };
	mining: {
		parameters: { from_height: number; to_height: number; top?: number; end_block_id?: string };
		response: MiningOverview;
	};
	network: { parameters: Record<string, never>; response: NetworkSummary };
	networkHistory: {
		parameters: { from_height: number; to_height: number; buckets?: number; end_block_id?: string };
		response: NetworkHistory;
	};
	supply: { parameters: Record<string, never>; response: SupplyDto };
	blocks: {
		parameters: { limit?: number; cursor?: string; snapshot?: string; dir?: 'desc' };
		response: PageDto<BlockDto>;
	};
	block: { parameters: { height_or_id: number | string }; response: BlockDto };
	rewards: { parameters: { height_or_id: number | string }; response: RewardBreakdown };
	transaction: { parameters: { id: string }; response: TxDto };
	transactionStatus: { parameters: { id: string }; response: TxStatusDto };
	transactionEvidence: { parameters: { id: string }; response: TxEvidence };
	transactions: {
		parameters: { limit?: number; cursor?: string; snapshot?: string; dir?: 'asc' | 'desc' };
		response: PageDto<TxSummaryDto>;
	};
	blockTransactions: {
		parameters: {
			height_or_id: number | string;
			limit?: number;
			cursor?: string;
			snapshot?: string;
			dir?: 'asc' | 'desc';
		};
		response: PageDto<TxSummaryDto>;
	};
	box: { parameters: { id: string }; response: BoxDto };
	address: { parameters: { addr: string }; response: AddressDto };
	addressRentExposure: { parameters: { addr: string; view: 'exposure' }; response: RentExposure };
	addressActivity: {
		parameters: {
			addr: string;
			limit?: number;
			cursor?: string;
			snapshot?: string;
			dir?: 'asc' | 'desc';
			asset?: string | 'all' | 'erg';
			direction?: 'all' | 'received' | 'sent' | 'mixed' | 'neutral' | 'unknown';
			from_ms?: number;
			to_ms?: number;
		};
		response: AddressActivityPageDto;
	};
	groupBalances: { parameters: { addresses: string[] }; response: GroupBalances };
	historicalBalance: {
		parameters: { addr: string; height: number; block_id?: string };
		response: HistoryBalance;
	};
	historicalBoxes: {
		parameters: {
			addr: string;
			height: number;
			block_id?: string;
			cursor?: string;
			limit?: number;
		};
		response: HistoryBoxes;
	};
	compareBalances: {
		parameters: {
			addr: string;
			from_height: number;
			to_height: number;
			from_block_id?: string;
			to_block_id?: string;
		};
		response: HistoryComparison;
	};
	token: { parameters: { id: string }; response: TokenInfoDto };
	tokenHolders: {
		parameters: { id: string; limit?: number; cursor?: string; snapshot?: string };
		response: TokenHoldersPageDto;
	};
	searchTokens: {
		parameters: {
			q: string;
			match?: 'prefix' | 'exact';
			limit?: number;
			cursor?: string;
			snapshot?: string;
		};
		response: TokenSearchDto;
	};
}
export type PagedOperation =
	| 'blocks'
	| 'transactions'
	| 'blockTransactions'
	| 'addressActivity'
	| 'tokenHolders'
	| 'searchTokens';
