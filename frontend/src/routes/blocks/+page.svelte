<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager } from '$lib/pager/pager.svelte';
	import type { BlockDto } from '$lib/api/types';

	const pager = createPager<BlockDto>((cursor) => api.blocks(cursor, 50));

	$effect(() => {
		if (pager.items.length === 0) void pager.loadMore();
	});

	function kb(size: number): string {
		return `${(size / 1024).toFixed(1)} KB`;
	}
</script>

<svelte:head>
	<title>Blocks — Ergo Explorer</title>
</svelte:head>

<Panel title="Blocks">
	<InfiniteList table columns={7} dense {pager} empty="No blocks yet.">
		{#snippet head()}
			<tr>
				<th>Height</th>
				<th>Age</th>
				<th>Txs</th>
				<th>Size</th>
				<th>Fees</th>
				<th>Reward</th>
				<th>Miner</th>
			</tr>
		{/snippet}
		{#snippet children(block: BlockDto)}
			<tr>
				<td><a href={`/blocks/${block.height}`}>{block.height}</a></td>
				<td><Age ms={block.timestamp} /></td>
				<td>{block.tx_count}</td>
				<td>{kb(block.size)}</td>
				<td><Amount nano={block.fees} maxFrac={3} /></td>
				<td><Amount nano={block.reward} maxFrac={3} /></td>
				<td><Hash value={block.miner_pk} copy={false} /></td>
			</tr>
		{/snippet}
	</InfiniteList>
</Panel>
