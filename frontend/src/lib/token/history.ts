import { ApiError, apiGet } from '$lib/api/client';
import type { PageDto, TxSummaryDto } from '$lib/api/types';

export interface TokenHistoryPage extends PageDto<TxSummaryDto> {
	history_context: {
		scope: 'indexed_token_touches';
		partial_from: number | null;
	};
}

export async function tokenHistory(id: string, cursor?: string, snapshot?: string) {
	try {
		const result = await apiGet<TokenHistoryPage>(`/tokens/${encodeURIComponent(id)}/txs`, {
			limit: 20,
			dir: 'desc',
			consistency: 'strict',
			cursor,
			snapshot
		});
		if (
			result.consistency !== 'strict' ||
			!result.anchor ||
			result.history_context?.scope !== 'indexed_token_touches' ||
			(result.history_context.partial_from !== null &&
				(!Number.isSafeInteger(result.history_context.partial_from) ||
					result.history_context.partial_from < 0)) ||
			(result.next_cursor !== null && !result.next_snapshot)
		)
			throw new Error(
				'The server did not provide a consistent token history snapshot. Retry after the server is updated.'
			);
		return result;
	} catch (error) {
		if (
			error instanceof ApiError &&
			error.status === 503 &&
			error.code === 'token_history_preparing'
		)
			throw new ApiError(
				503,
				'Token history is preparing',
				'The token transaction index is being prepared. Retry shortly; holder balances and boxes remain available.',
				error.code
			);
		if (error instanceof ApiError && error.status === 404)
			throw new ApiError(
				404,
				'Token history unavailable',
				'Token transaction history is not available on this server yet. Holder balances and boxes remain available.',
				error.code
			);
		throw error;
	}
}
