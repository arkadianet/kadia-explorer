import { apiGet } from './client';
import type {
	AddressDto,
	AddressRentDto,
	BlockDto,
	BoxDto,
	PageDto,
	RentItemDto,
	RichlistItemDto,
	SearchDto,
	StatusDto,
	TxDto
} from './types';

type Fetch = typeof fetch;

export const api = {
	status: (f?: Fetch) => apiGet<StatusDto>('/status', undefined, f),

	blocks: (cursor?: string, limit = 50, dir?: 'asc' | 'desc', f?: Fetch) =>
		apiGet<PageDto<BlockDto>>('/blocks', { cursor, limit, dir }, f),

	block: (heightOrId: string | number, f?: Fetch) =>
		apiGet<BlockDto>(`/blocks/${heightOrId}`, undefined, f),

	blockTxs: (id: string, f?: Fetch) => apiGet<TxDto[]>(`/blocks/${id}/txs`, undefined, f),

	txs: (cursor?: string, limit = 50, dir?: 'asc' | 'desc', f?: Fetch) =>
		apiGet<PageDto<TxDto>>('/txs', { cursor, limit, dir }, f),

	tx: (id: string, f?: Fetch) => apiGet<TxDto>(`/txs/${id}`, undefined, f),

	box: (id: string, f?: Fetch) => apiGet<BoxDto>(`/boxes/${id}`, undefined, f),

	boxRent: (id: string, f?: Fetch) =>
		apiGet<{
			box_id: string;
			maturity_height: number;
			due_nano: string;
			claimable_at_tip: boolean;
		}>(`/boxes/${id}/rent`, undefined, f),

	address: (addr: string, f?: Fetch) => apiGet<AddressDto>(`/addresses/${addr}`, undefined, f),

	addressBoxes: (
		addr: string,
		unspent?: boolean,
		cursor?: string,
		limit = 50,
		dir?: 'asc' | 'desc',
		f?: Fetch
	) => apiGet<PageDto<BoxDto>>(`/addresses/${addr}/boxes`, { unspent, cursor, limit, dir }, f),

	addressTxs: (addr: string, cursor?: string, limit = 50, dir?: 'asc' | 'desc', f?: Fetch) =>
		apiGet<PageDto<TxDto>>(`/addresses/${addr}/txs`, { cursor, limit, dir }, f),

	addressRent: (addr: string, f?: Fetch) =>
		apiGet<AddressRentDto>(`/addresses/${addr}/rent`, undefined, f),

	richlist: (cursor?: string, limit = 50, f?: Fetch) =>
		apiGet<PageDto<RichlistItemDto>>('/richlist', { cursor, limit }, f),

	rentUpcoming: (blocks = 720, limit = 50, f?: Fetch) =>
		apiGet<RentItemDto[]>('/rent/upcoming', { blocks, limit }, f),

	rentEligible: (cursor?: string, limit = 50, f?: Fetch) =>
		apiGet<PageDto<RentItemDto>>('/rent/eligible', { cursor, limit }, f),

	search: (q: string, f?: Fetch) => apiGet<SearchDto>('/search', { q }, f)
};
