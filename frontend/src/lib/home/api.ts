import { apiGet } from '$lib/api/client';
import type { BlockDto } from '$lib/api/types';

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

export const networkSummary = (f?: typeof fetch) =>
	apiGet<NetworkSummary>('/network/summary', undefined, f);
