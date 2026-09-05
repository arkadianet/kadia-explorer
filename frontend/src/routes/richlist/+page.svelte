<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager } from '$lib/pager/pager.svelte';
	import { status } from '$lib/status/status.svelte';
	import { circulatingAt } from '$lib/format/supply';
	import type { RichlistItemDto } from '$lib/api/types';

	const pager = createPager<RichlistItemDto>((cursor) => api.richlist(cursor, 50));

	$effect(() => {
		if (pager.items.length === 0) void pager.loadMore();
	});

	const tip = $derived(status.current?.indexed ?? null);
	const supply = $derived(tip === null ? null : circulatingAt(tip));

	function pctOfSupply(nano: string): string {
		if (supply === null || supply === 0n) return '—';
		const pct = Number((BigInt(nano) * 10_000n) / supply) / 100;
		return `${pct.toFixed(2)}%`;
	}
</script>

<svelte:head>
	<title>Rich list — Ergo Explorer</title>
</svelte:head>

<Panel title="Rich list">
	<p class="note">Supply estimate ignores EIP‑27 re-emission</p>
	<InfiniteList table columns={4} dense {pager} empty="No addresses yet.">
		{#snippet head()}
			<tr>
				<th>Rank</th>
				<th>Address</th>
				<th>Balance</th>
				<th>≈ % of supply</th>
			</tr>
		{/snippet}
		{#snippet children(item: RichlistItemDto, i: number)}
			<tr>
				<td>{i + 1}</td>
				<td>
					{#if item.address}
						<Hash value={item.address} href={`/address/${item.address}`} copy={false} />
					{:else}
						<span class="muted" title="script address unavailable">
							<Hash value={item.tree_hash} copy={false} />
						</span>
					{/if}
				</td>
				<td><Amount nano={item.nano} maxFrac={9} /></td>
				<td>{pctOfSupply(item.nano)}</td>
			</tr>
		{/snippet}
	</InfiniteList>
</Panel>

<style>
	.note {
		color: var(--fg-muted);
		padding: 0 var(--space-4) var(--space-2);
		font-size: 12px;
	}
	.muted {
		color: var(--fg-muted);
	}
</style>
