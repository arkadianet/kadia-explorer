import type { PageDto, RentItemDto } from '$lib/api/types';

/**
 * `api.rentUpcoming` is typed as returning `RentItemDto[]` (per `$lib/api/types`/`endpoints.ts`),
 * but the live xp-api server was observed responding with a `PageDto<RentItemDto>` envelope
 * (`{ items, next_cursor }`) instead of a bare array — see task-9-report.md. Normalize either
 * shape defensively here so the Upcoming tab doesn't break if/when that mismatch is resolved
 * upstream in either direction.
 */
export function normalizeUpcoming(raw: RentItemDto[] | PageDto<RentItemDto>): RentItemDto[] {
	return Array.isArray(raw) ? raw : raw.items;
}
