<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager } from '$lib/pager/pager.svelte';
	import type { TxDto } from '$lib/api/types';

	const pager = createPager<TxDto>((cursor) => api.txs(cursor, 50));

	$effect(() => {
		if (pager.items.length === 0) void pager.loadMore();
	});
</script>

<svelte:head>
	<title>Transactions — Ergo Explorer</title>
</svelte:head>

<Panel title="Transactions">
	<InfiniteList
		table
		columns={6}
		dense
		{pager}
		empty="Transactions will appear here as the indexer catches up with the node."
	>
		{#snippet head()}
			<tr>
				<th>Id</th>
				<th>Block</th>
				<th>Age</th>
				<th class="num">Inputs</th>
				<th class="num">Outputs</th>
				<th class="num">Fee</th>
			</tr>
		{/snippet}
		{#snippet children(tx: TxDto)}
			<tr>
				<td><Hash value={tx.id} href={`/tx/${tx.id}`} copy={false} /></td>
				<td class="mono"><a href={`/blocks/${tx.height}`}>{tx.height}</a></td>
				<td><Age ms={tx.timestamp} /></td>
				<td class="num mono">{tx.inputs.length}</td>
				<td class="num mono">{tx.outputs.length}</td>
				<td class="num"><Amount nano={tx.fee} maxFrac={3} /></td>
			</tr>
		{/snippet}
	</InfiniteList>
</Panel>
