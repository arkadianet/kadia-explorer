/** Rich-list rows include protocol reserves, so use the entire genesis allocation. */
export function pctOfGenesis(nano: string, total: string | null): string {
	if (total === null || BigInt(total) === 0n) return '—';
	return `${(Number((BigInt(nano) * 10_000n) / BigInt(total)) / 100).toFixed(2)}%`;
}
