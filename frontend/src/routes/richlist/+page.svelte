<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import { api } from '$lib/api/endpoints';
	import { pctOfGenesis } from '$lib/format/supply';
	import { createPager } from '$lib/pager/pager.svelte';
	import type { RichlistItemDto, SupplyDto } from '$lib/api/types';

	const pager = createPager<RichlistItemDto>((cursor) => api.richlist(cursor, 50));

	$effect(() => {
		if (pager.items.length === 0) void pager.loadMore();
	});

	// The list includes protocol reserves; its denominator includes them too.
	let supplyDto = $state<SupplyDto | null>(null);
	$effect(() => {
		void api
			.supply()
			.then((s) => (supplyDto = s))
			.catch(() => (supplyDto = null));
	});

	const supply = $derived(supplyDto?.complete ? supplyDto.genesis_total_nano : null);
</script>

<svelte:head>
	<title>Rich list — Ergo Explorer</title>
</svelte:head>

<Panel title="Rich list">
	<p class="note">
		Share of total genesis allocation, including emission, re-emission and treasury reserves. This
		is not circulating supply.
	</p>
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
				<th class="num">% of genesis allocation</th>
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
				<td class="num mono">{pctOfGenesis(item.nano, supply)}</td>
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
