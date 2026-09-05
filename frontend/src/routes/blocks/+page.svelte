<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import MinerChip from '$lib/components/MinerChip.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager } from '$lib/pager/pager.svelte';
	import { formatKb } from '$lib/format/size';
	import type { BlockDto } from '$lib/api/types';

	const pager = createPager<BlockDto>((cursor) => api.blocks(cursor, 50));

	$effect(() => {
		if (pager.items.length === 0) void pager.loadMore();
	});
</script>

<svelte:head>
	<title>Blocks — Ergo Explorer</title>
</svelte:head>

<Panel title="Blocks">
	<InfiniteList
		table
		columns={7}
		dense
		{pager}
		empty="Blocks will appear here as the indexer catches up with the node."
	>
		{#snippet head()}
			<tr>
				<th>Height</th>
				<th>Age</th>
				<th class="num">Txs</th>
				<th class="num">Size</th>
				<th class="num">Fees</th>
				<th class="num">Reward</th>
				<th>Miner</th>
			</tr>
		{/snippet}
		{#snippet children(block: BlockDto)}
			<tr>
				<td class="mono"><a href={`/blocks/${block.height}`}>{block.height}</a></td>
				<td><Age ms={block.timestamp} /></td>
				<td class="num mono">{block.tx_count}</td>
				<td class="num mono">{formatKb(block.size)}</td>
				<td class="num"><Amount nano={block.fees} maxFrac={3} /></td>
				<td class="num"><Amount nano={block.reward} maxFrac={3} /></td>
				<td><MinerChip minerPk={block.miner_pk} /></td>
			</tr>
		{/snippet}
	</InfiniteList>
</Panel>
