<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import Table from '$lib/components/Table.svelte';
	import Hash from '$lib/components/Hash.svelte';
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

<Panel title={`Block ${block.height}`}>
	<div class="facts">
		<div class="fact">
			<span class="label">Height</span>
			<span>{block.height}</span>
		</div>
		<div class="fact">
			<span class="label">Id</span>
			<Hash value={block.id} />
		</div>
		<div class="fact">
			<span class="label">Parent</span>
			<a href={`/blocks/${block.parent_id}`}><Hash value={block.parent_id} copy={false} /></a>
		</div>
		<div class="fact">
			<span class="label">Timestamp</span>
			<span>{absTime(block.timestamp)} (<Age ms={block.timestamp} />)</span>
		</div>
		<div class="fact">
			<span class="label">Difficulty</span>
			<span class="mono">{formatNano(block.difficulty)}</span>
		</div>
		<div class="fact">
			<span class="label">Size</span>
			<span>{formatKb(block.size)}</span>
		</div>
		<div class="fact">
			<span class="label">Version</span>
			<span>{block.version}</span>
		</div>
		<div class="fact">
			<span class="label">Txs</span>
			<span>{block.tx_count}</span>
		</div>
		<div class="fact">
			<span class="label">Fees</span>
			<Amount nano={block.fees} maxFrac={3} />
		</div>
		<div class="fact">
			<span class="label">Reward</span>
			<Amount nano={block.reward} maxFrac={3} />
		</div>
		<div class="fact">
			<span class="label">Miner</span>
			<Hash value={block.miner_pk} copy={false} />
		</div>
	</div>

	<div class="nav">
		{#if block.height > 1}
			<a href={`/blocks/${block.height - 1}`}>&larr; Prev</a>
		{:else}
			<span class="disabled" aria-disabled="true">&larr; Prev</span>
		{/if}
		<a href={`/blocks/${block.height + 1}`}>Next &rarr;</a>
	</div>
</Panel>

<Panel title="Transactions">
	{#if txs.length === 0}
		<EmptyState message="No transactions." />
	{:else}
		<Table>
			{#snippet head()}
				<tr>
					<th>Id</th>
					<th>Inputs</th>
					<th>Outputs</th>
					<th>Total output</th>
					<th>Fee</th>
				</tr>
			{/snippet}
			{#each txs as tx (tx.id)}
				<tr>
					<td><Hash value={tx.id} href={`/tx/${tx.id}`} copy={false} /></td>
					<td>{tx.inputs.length}</td>
					<td>{tx.outputs.length}</td>
					<td><Amount nano={sumNano(tx.outputs.map((o) => o.value)).toString()} maxFrac={3} /></td>
					<td><Amount nano={tx.fee} maxFrac={3} /></td>
				</tr>
			{/each}
		</Table>
	{/if}
</Panel>

<style>
	.facts {
		display: grid;
		grid-template-columns: 1fr;
		gap: var(--space-2) var(--space-4);
		padding: var(--space-2) var(--space-4);
	}
	@media (min-width: 768px) {
		.facts {
			grid-template-columns: repeat(2, 1fr);
		}
	}
	.fact {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
		border-bottom: 1px solid var(--border);
		padding: var(--space-1) 0;
	}
	.label {
		color: var(--fg-muted);
	}
	.nav {
		display: flex;
		justify-content: space-between;
		padding: var(--space-3) var(--space-4);
		border-top: 1px solid var(--border);
	}
	.disabled {
		color: var(--fg-muted);
	}
</style>
