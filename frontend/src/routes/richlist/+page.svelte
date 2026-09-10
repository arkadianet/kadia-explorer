<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager } from '$lib/pager/pager.svelte';
	import type { RichlistItemDto, SupplyDto } from '$lib/api/types';

	const pager = createPager<RichlistItemDto>((cursor) => api.richlist(cursor, 50));

	$effect(() => {
		if (pager.items.length === 0) void pager.loadMore();
	});

	// Supply comes from the chain (emission contract balance), not an emission schedule: a
	// hardcoded schedule was wrong about both the early foundation share and EIP-27.
	let supplyDto = $state<SupplyDto | null>(null);
	$effect(() => {
		void api
			.supply()
			.then((s) => (supplyDto = s))
			.catch(() => (supplyDto = null));
	});

	const supply = $derived(supplyDto?.emitted_nano == null ? null : BigInt(supplyDto.emitted_nano));

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
	<InfiniteList
		table
		columns={4}
		dense
		{pager}
		empty="The rich list fills in as the indexer works through the chain."
	>
		{#snippet head()}
			<tr>
				<th class="num">Rank</th>
				<th>Address</th>
				<th class="num">Balance</th>
				<th class="num">≈ % of supply</th>
			</tr>
		{/snippet}
		{#snippet children(item: RichlistItemDto, i: number)}
			<tr>
				<td class="num mono">{i + 1}</td>
				<td>
					{#if item.address}
						<Hash value={item.address} href={`/address/${item.address}`} copy={false} />
					{:else}
						<span class="muted" title="script address unavailable">
							<Hash value={item.tree_hash} copy={false} />
						</span>
					{/if}
				</td>
				<td class="num"><Amount nano={item.nano} maxFrac={9} /></td>
				<td class="num mono">{pctOfSupply(item.nano)}</td>
			</tr>
		{/snippet}
	</InfiniteList>
</Panel>

<style>
	.note {
		color: var(--fg-muted);
		padding-bottom: var(--space-3);
		font-size: var(--fs-data);
	}
	.muted {
		color: var(--fg-muted);
	}
</style>
