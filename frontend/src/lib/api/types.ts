// Mirrors crates/xp-api/src/dto.rs. Amounts are decimal strings, never JSON numbers, since
// they can exceed the 2^53 range a JS `number` can represent exactly — convert with BigInt.

export interface PageDto<T> {
	items: T[];
	next_cursor: string | null;
}

export interface StatusStalledDto {
	height: number;
	since_secs: number;
	reason: string;
}

export interface StatusDto {
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
	due_nano: string;
	claimable_at_tip: boolean;
}

export interface BoxDto {
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

/** `/v1/addresses/{addr}/txs` — a cheap summary of a transaction, without inputs/outputs. */
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

export interface TokenHolderDto {
	address: string | null;
	tree_hash: string;
	amount: string;
	share_pct: string;
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
	emitted_nano: string | null;
	genesis_total_nano: string;
	complete: boolean;
}
