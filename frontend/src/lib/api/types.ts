// Mirrors crates/xp-api/src/dto.rs. Amounts are decimal strings, never JSON numbers, since
// they can exceed the 2^53 range a JS `number` can represent exactly — convert with BigInt.

export interface PageDto<T> {
	items: T[];
	next_cursor: string | null;
	next_snapshot?: string | null;
	consistency?: 'strict' | 'best_effort';
	anchor?: { height: number; block_id: string } | null;
	observed_anchor?: { height: number; block_id: string } | null;
}

export interface StatusStalledDto {
	height: number;
	since_secs: number;
	reason: string;
}

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

export interface InputDto {
	id: string;
	box: BoxDto | null;
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

export interface TxEvidence {
	tx_id: string;
	block_id: string;
	height: number;
	assurance: 'trusted_node_response';
	inputs: { id: string; proof: 'empty' | 'nonempty'; extension_127: string | null }[];
}

/** Bounded node projection; input balances, registers and signatures are not resolved. */
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
		tokens: { id: string; amount: string }[];
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

/** A single local-index and configured-node observation, not network-wide acceptance. */
export interface TxStatusDto {
	id: string;
	state:
		'confirmed' | 'pending' | 'not_observed' | 'no_longer_observed' | 'unavailable' | 'conflicted';
	checked_at_ms: number;
	indexed_height: number | null;
	inclusion: { block_id: string; height: number; confirmations: number } | null;
	previous_inclusion: { block_id: string; height: number } | null;
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
	conflicts: { input_id: string; tx_id: string; height: number }[];
	history_scope: 'process_local_requested_transactions';
	retention_seconds: number;
}

/** Transaction summary routes and address history: counts without expanded inputs/outputs. */
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

export interface BalanceDto {
	nano: string;
	tokens: TokenDto[];
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

/** `/v1/boxes/{id}/rent` — the Rust DTO `#[serde(flatten)]`s `RentDto`, so the wire shape is
 * flat: `{ box_id, maturity_height, due_nano, claimable_at_tip }`. */
export interface BoxRentDto extends RentDto {
	box_id: string;
}

export interface AddressRentDto {
	items: BoxDto[];
	truncated: boolean;
}

export interface RentItemDto {
	maturity_height: number;
	box: BoxDto;
}

export interface RichlistItemDto {
	address: string | null;
	tree_hash: string;
	nano: string;
}

export interface SearchDto {
	matches?: { kind: SearchDto['kind']; id: string }[];
	kind: 'block' | 'tx' | 'box' | 'address' | 'token' | 'template';
	id: string;
}

/** EIP-4's R7 type-tag classification, as interpreted by `token_kind` on the backend. */
export type TokenKind = 'nft-picture' | 'nft-audio' | 'nft-video' | 'membership' | 'token';

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

export interface TokenHolderDto {
	address: string | null;
	tree_hash: string;
	amount: string;
	share_pct: string;
}

export interface TokenHolderContext {
	supply: string;
	holder_count: number;
	definition: 'indexed_emission_minus_burned';
}
export interface TokenHoldersPageDto extends PageDto<TokenHolderDto> {
	holder_context?: TokenHolderContext;
}

export type ActivityDirection = 'received' | 'sent' | 'mixed' | 'neutral' | 'unknown';
export interface AddressActivityFilters {
	asset?: string;
	direction?: ActivityDirection | 'all';
	from_ms?: number;
	to_ms?: number;
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
	coverage: { complete: boolean; resolved_inputs: number; total_inputs: number };
	erg_delta: string | null;
	tokens: { id: string; name: string | null; decimals: number | null; delta: string | null }[];
	direction: ActivityDirection;
	asset_match: 'definite' | 'uncertain';
}
export interface AddressActivityPageDto extends PageDto<AddressActivityDto> {
	scanned: number;
	scan_limit_reached: boolean;
	partial_from: number | null;
}

export interface TemplateDto {
	hash: string;
	box_count: number;
	unspent_count: number;
	first_seen: number;
	example_address: string | null;
}

/** Chain-derived supply figures (`/v1/supply`). Amounts are decimal strings of nanoERG. */
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
