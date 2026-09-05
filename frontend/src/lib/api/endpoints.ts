import { apiGet } from './client';
import type {
	AddressDto,
	AddressRentDto,
	BlockDto,
	BoxDto,
	BoxRentDto,
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
		apiGet<BlockDto>(`/blocks/${encodeURIComponent(heightOrId)}`, undefined, f),

	blockTxs: (id: string, f?: Fetch) =>
		apiGet<TxDto[]>(`/blocks/${encodeURIComponent(id)}/txs`, undefined, f),

	txs: (cursor?: string, limit = 50, dir?: 'asc' | 'desc', f?: Fetch) =>
		apiGet<PageDto<TxDto>>('/txs', { cursor, limit, dir }, f),

	tx: (id: string, f?: Fetch) => apiGet<TxDto>(`/txs/${encodeURIComponent(id)}`, undefined, f),

	box: (id: string, f?: Fetch) => apiGet<BoxDto>(`/boxes/${encodeURIComponent(id)}`, undefined, f),

	boxRent: (id: string, f?: Fetch) =>
		apiGet<BoxRentDto>(`/boxes/${encodeURIComponent(id)}/rent`, undefined, f),

	address: (addr: string, f?: Fetch) =>
		apiGet<AddressDto>(`/addresses/${encodeURIComponent(addr)}`, undefined, f),

	addressBoxes: (
		addr: string,
		unspent?: boolean,
		cursor?: string,
		limit = 50,
		dir?: 'asc' | 'desc',
		f?: Fetch
	) =>
		apiGet<PageDto<BoxDto>>(
			`/addresses/${encodeURIComponent(addr)}/boxes`,
			{ unspent, cursor, limit, dir },
			f
		),

	addressTxs: (addr: string, cursor?: string, limit = 50, dir?: 'asc' | 'desc', f?: Fetch) =>
		apiGet<PageDto<TxDto>>(`/addresses/${encodeURIComponent(addr)}/txs`, { cursor, limit, dir }, f),

	addressRent: (addr: string, f?: Fetch) =>
		apiGet<AddressRentDto>(`/addresses/${encodeURIComponent(addr)}/rent`, undefined, f),

	richlist: (cursor?: string, limit = 50, f?: Fetch) =>
		apiGet<PageDto<RichlistItemDto>>('/richlist', { cursor, limit }, f),

	rentUpcoming: (blocks = 720, limit = 50, f?: Fetch) =>
		apiGet<RentItemDto[]>('/rent/upcoming', { blocks, limit }, f),

	rentEligible: (cursor?: string, limit = 50, f?: Fetch) =>
		apiGet<PageDto<RentItemDto>>('/rent/eligible', { cursor, limit }, f),

	search: (q: string, f?: Fetch) => apiGet<SearchDto>('/search', { q }, f)
};
