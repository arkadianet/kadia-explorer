<script lang="ts">
	import { api } from '$lib/api/endpoints';
	import type { TxSummaryDto } from '$lib/api/types';
	import { createPager } from '$lib/pager/pager.svelte';
	import InfiniteList from './InfiniteList.svelte';
	import Hash from './Hash.svelte';
	import Amount from './Amount.svelte';
	let { blockId }: { blockId: string } = $props();
	const pager = createPager((cursor, snapshot) =>
		api.blockTxSummaries(blockId, cursor, 50, undefined, snapshot)
	);
	$effect(() => {
		if (pager.items.length === 0) void pager.loadMore();
	});
</script>

<InfiniteList table columns={4} {pager} empty="This block carries no transactions.">
	{#snippet head()}
		<tr
			><th>Transaction</th><th class="num">Inputs</th><th class="num">Outputs</th><th class="num"
				>Fee</th
			></tr
		>
	{/snippet}
	{#snippet children(tx: TxSummaryDto)}
		<tr>
			<td><Hash value={tx.id} href={`/tx/${tx.id}`} copy={false} /></td>
			<td class="num mono">{tx.input_count}</td>
			<td class="num mono">{tx.output_count}</td>
			<td class="num"><Amount nano={tx.fee} maxFrac={3} /></td>
		</tr>
	{/snippet}
</InfiniteList>
