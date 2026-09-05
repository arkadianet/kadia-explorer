<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import BoxCard from '$lib/components/BoxCard.svelte';
	import { status } from '$lib/status/status.svelte';
	import { sumNano } from '$lib/format/amount';
	import { formatKb } from '$lib/format/size';
	import { absTime } from '$lib/format/time';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const tx = $derived(data.tx);
	const tip = $derived(status.current?.indexed ?? null);

	const knownInputValues = $derived(tx.inputs.flatMap((i) => (i.box ? [i.box.value] : [])));
	// Inputs whose box predates the indexed range have no value we can add up, so the "in"
	// total is explicitly marked partial rather than silently under-reporting.
	const partialIn = $derived(knownInputValues.length !== tx.inputs.length);

	const totalIn = $derived(sumNano(knownInputValues).toString());
	const totalOut = $derived(sumNano(tx.outputs.map((o) => o.value)).toString());
</script>

<svelte:head>
	<title>Transaction {tx.id} — Ergo Explorer</title>
</svelte:head>

<Panel title="Transaction">
	<div class="facts">
		<div class="fact">
			<span class="label">Id</span>
			<Hash value={tx.id} head={12} tail={10} />
		</div>
		<div class="fact">
			<span class="label">Block</span>
			<a href={`/blocks/${tx.height}`}>{tx.height}</a>
		</div>
		<div class="fact">
			<span class="label">Timestamp</span>
			<span>{absTime(tx.timestamp)} (<Age ms={tx.timestamp} />)</span>
		</div>
		<div class="fact">
			<span class="label">Index in block</span>
			<span>{tx.index}</span>
		</div>
		<div class="fact">
			<span class="label">Size</span>
			<span>{formatKb(tx.size)}</span>
		</div>
		<div class="fact">
			<span class="label">Fee</span>
			<Amount nano={tx.fee} maxFrac={9} />
		</div>
	</div>

	{#if tx.data_inputs.length > 0}
		<div class="data-inputs">
			<span class="label">Data inputs</span>
			<ul>
				{#each tx.data_inputs as id, i (i)}
					<li><Hash value={id} href={`/box/${id}`} copy={false} head={12} /></li>
				{/each}
			</ul>
		</div>
	{/if}

	<div class="totals">
		<div>
			<span class="label">Total in{partialIn ? ' (known boxes)' : ''}</span>
			<Amount nano={totalIn} maxFrac={9} />
		</div>
		<div>
			<span class="label">Total out</span>
			<Amount nano={totalOut} maxFrac={9} />
		</div>
		<div>
			<span class="label">Fee</span>
			<Amount nano={tx.fee} maxFrac={9} />
		</div>
	</div>
</Panel>

<div class="flow">
	<Panel title={`Inputs (${tx.inputs.length})`}>
		<div class="cards">
			{#each tx.inputs as input (input.id)}
				{#if input.box}
					<BoxCard box={input.box} {tip} role="input" />
				{:else}
					<article class="box unknown">
						<span class="label">Unknown box</span>
						<Hash value={input.id} href={`/box/${input.id}`} copy={false} head={10} />
					</article>
				{/if}
			{/each}
		</div>
	</Panel>

	<Panel title={`Outputs (${tx.outputs.length})`}>
		<div class="cards">
			{#each tx.outputs as output (output.id)}
				<BoxCard box={output} {tip} role="output" />
			{/each}
		</div>
	</Panel>
</div>

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
	.data-inputs {
		padding: var(--space-2) var(--space-4);
		border-top: 1px solid var(--border);
	}
	.data-inputs ul {
		list-style: none;
		margin: var(--space-1) 0 0;
		padding: 0;
	}
	.totals {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-6);
		padding: var(--space-3) var(--space-4);
		border-top: 1px solid var(--border);
	}
	.totals div {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.flow {
		display: grid;
		grid-template-columns: 1fr;
		gap: var(--space-4);
		margin-top: var(--space-4);
	}
	@media (min-width: 900px) {
		.flow {
			grid-template-columns: 1fr 1fr;
			align-items: start;
		}
	}
	.cards {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: 0 var(--space-3);
	}
	.box.unknown {
		border: 1px dashed var(--border);
		border-radius: var(--radius);
		padding: var(--space-2) var(--space-3);
		display: flex;
		flex-direction: column;
		gap: 2px;
		color: var(--fg-muted);
	}
</style>
