import type { KadiaOperations, PagedOperation } from './api-types.js';
export type * from './api-types.js';
export interface ClientOptions {
	/** API root, including /v1; e.g. https://explorer.kadia.io/v1 */
	baseUrl: string;
	fetch?: typeof fetch;
	timeoutMs?: number;
	maxResponseBytes?: number;
}
export interface RequestOptions {
	signal?: AbortSignal;
}
export interface WalkOptions extends RequestOptions {
	maxPages?: number;
	maxItems?: number;
}
export class KadiaApiError extends Error {
	status: number;
	code: string | null;
	retryAfter: string | null;
	data: unknown;
}
export class KadiaLimitError extends Error {}
export interface KadiaClient {
	/** Retain store coverage; unknown means the server did not expose the header. */
	observe<K extends keyof KadiaOperations>(
		operation: K,
		parameters: KadiaOperations[K]['parameters'],
		options?: RequestOptions
	): Promise<{
		data: KadiaOperations[K]['response'];
		completeness: 'complete' | 'incomplete' | 'unknown';
		status: number;
	}>;
	request<K extends keyof KadiaOperations>(
		operation: K,
		parameters: KadiaOperations[K]['parameters'],
		options?: RequestOptions
	): Promise<KadiaOperations[K]['response']>;
	/** Bounded strict walk. If a later page fails, earlier pages are incomplete. No auto retry. */
	pages<K extends PagedOperation>(
		operation: K,
		parameters: KadiaOperations[K]['parameters'],
		options?: WalkOptions
	): AsyncGenerator<KadiaOperations[K]['response'], void, unknown>;
}
export function createKadiaClient(options: ClientOptions): KadiaClient;
