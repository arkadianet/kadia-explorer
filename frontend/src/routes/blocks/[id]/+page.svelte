<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import PageHead from '$lib/components/PageHead.svelte';
	import Facts from '$lib/components/Facts.svelte';
	import Fact from '$lib/components/Fact.svelte';
	import Table from '$lib/components/Table.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import MinerChip from '$lib/components/MinerChip.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import { formatNano, sumNano } from '$lib/format/amount';
	import { formatKb } from '$lib/format/size';
	import { absTime } from '$lib/format/time';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const block = $derived(data.block);
	const txs = $derived(data.txs);
</script>

<svelte:head>
	<title>Block {block.height} — Ergo Explorer</title>
</svelte:head>

<div class="head">
	<PageHead title={`Block ${block.height}`} id={block.id} />

	<Facts>
		<Fact label="Height"><span class="mono">{block.height}</span></Fact>
		<Fact label="Parent">
			<Hash value={block.parent_id} href={`/blocks/${block.parent_id}`} copy={false} />
		</Fact>
		<Fact label="Mined"><span>{absTime(block.timestamp)}, <Age ms={block.timestamp} /></span></Fact>
		<Fact label="Difficulty"><span class="mono">{formatNano(block.difficulty)}</span></Fact>
		<Fact label="Size"><span class="mono">{formatKb(block.size)}</span></Fact>
		<Fact label="Version"><span class="mono">{block.version}</span></Fact>
		<Fact label="Txs"><span class="mono">{block.tx_count}</span></Fact>
		<Fact label="Fees"><Amount nano={block.fees} maxFrac={3} /></Fact>
		<Fact label="Reward"><Amount nano={block.reward} maxFrac={3} /></Fact>
		<Fact label="Miner"><MinerChip minerPk={block.miner_pk} /></Fact>
	</Facts>

	<nav class="pager" aria-label="Adjacent blocks">
		{#if block.height > 1}
			<a href={`/blocks/${block.height - 1}`}>&larr; Prev</a>
		{:else}
			<span></span>
		{/if}
		<a href={`/blocks/${block.height + 1}`}>Next &rarr;</a>
	</nav>
</div>

<Panel title="Transactions">
	{#if txs.length === 0}
		<EmptyState message="This block carries no transactions." />
	{:else}
		<Table>
			{#snippet head()}
				<tr>
					<th>Id</th>
					<th class="num">Inputs</th>
					<th class="num">Outputs</th>
					<th class="num">Total output</th>
					<th class="num">Fee</th>
				</tr>
			{/snippet}
			{#each txs as tx (tx.id)}
				<tr>
					<td><Hash value={tx.id} href={`/tx/${tx.id}`} copy={false} /></td>
					<td class="num mono">{tx.inputs.length}</td>
					<td class="num mono">{tx.outputs.length}</td>
					<td class="num">
						<Amount nano={sumNano(tx.outputs.map((o) => o.value)).toString()} maxFrac={3} />
					</td>
					<td class="num"><Amount nano={tx.fee} maxFrac={3} /></td>
				</tr>
			{/each}
		</Table>
	{/if}
</Panel>

<style>
	.head {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.pager {
		display: flex;
		justify-content: space-between;
		font-size: var(--fs-data);
		color: var(--fg-muted);
	}
	.pager a:hover,
	.pager a:focus-visible {
		color: var(--accent-ink);
	}
</style>
