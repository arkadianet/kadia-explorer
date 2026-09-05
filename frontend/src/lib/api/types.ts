// Mirrors crates/xp-api/src/dto.rs. Amounts are decimal strings, never JSON numbers, since
// they can exceed the 2^53 range a JS `number` can represent exactly — convert with BigInt.

export interface PageDto<T> {
	items: T[];
	next_cursor: string | null;
}

export interface StatusDto {
	indexed: number | null;
	best: number;
	mode: 'bulk' | 'tip';
	source: string;
	halted: string | null;
	lag_blocks: number;
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

export interface BalanceDto {
	nano: string;
	tokens: TokenDto[];
}

export interface AddressDto {
	address: string;
	tree_hash: string;
	balance: BalanceDto;
	box_count: number;
	first_seen: number;
	last_seen: number;
}

export interface BoxRentDto {
	box_id: string;
	rent: RentDto;
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
	kind: 'block' | 'tx' | 'box' | 'address';
	id: string;
}
