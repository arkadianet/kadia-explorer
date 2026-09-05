<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import Table from '$lib/components/Table.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import RegistersTable from '$lib/components/RegistersTable.svelte';
	import { hasDisplayableRegisters } from '$lib/registers/decode';
	import { status } from '$lib/status/status.svelte';
	import { formatNano } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import type { PageData } from './$types';

	/** Nominal Ergo block time, used to give the "matures in N blocks" rent estimate a rough
	 * day count. */
	const MINUTES_PER_BLOCK = 2;
	const MINUTES_PER_DAY = 24 * 60;

	let { data }: { data: PageData } = $props();

	const box = $derived(data.box);
	const tip = $derived(status.current?.indexed ?? null);

	const hasRegisters = $derived(hasDisplayableRegisters(box.registers));

	const maturityReached = $derived(tip !== null && box.rent.maturity_height <= tip);
	const blocksLeft = $derived(tip === null ? null : box.rent.maturity_height - tip);
	const daysLeft = $derived(
		blocksLeft === null ? null : Math.ceil((blocksLeft * MINUTES_PER_BLOCK) / MINUTES_PER_DAY)
	);
</script>

<svelte:head>
	<title>Box {box.id} — Ergo Explorer</title>
</svelte:head>

<Panel title="Box">
	<div class="facts">
		<div class="fact">
			<span class="label">Id</span>
			<Hash value={box.id} head={12} tail={10} />
		</div>
		<div class="fact">
			<span class="label">Value</span>
			<Amount nano={box.value} maxFrac={9} />
		</div>
		<div class="fact">
			<span class="label">Address</span>
			{#if box.address}
				<Hash value={box.address} href={`/address/${box.address}`} copy={false} head={12} />
			{:else}
				<span class="muted" title="No P2PK/P2S address — showing the ergo tree hash">
					<Hash value={box.tree_hash} copy={false} head={12} />
				</span>
			{/if}
		</div>
		<div class="fact">
			<span class="label">Created in tx</span>
			<span>
				<Hash value={box.tx_id} href={`/tx/${box.tx_id}`} copy={false} head={12} /> at output {box.index}
			</span>
		</div>
		<div class="fact">
			<span class="label">Creation height</span>
			<a href={`/blocks/${box.creation_height}`}>{box.creation_height}</a>
		</div>
		<div class="fact">
			<span class="label">Size</span>
			<span>{box.size} bytes</span>
		</div>
		<div class="fact">
			<span class="label">Status</span>
			{#if box.spent_by}
				<span>
					<Badge tone="neutral">Spent</Badge>
					by
					<a href={`/tx/${box.spent_by}`}><Hash value={box.spent_by} copy={false} head={10} /></a>
					{#if box.spent_height !== null}
						at <a href={`/blocks/${box.spent_height}`}>{box.spent_height}</a>
					{/if}
				</span>
			{:else}
				<Badge tone="ok">Unspent</Badge>
			{/if}
		</div>
	</div>

	{#if box.ergo_tree}
		<details class="ergo-tree">
			<summary>Ergo tree</summary>
			<div class="ergo-tree-body">
				{#if box.template_hash}
					<div class="fact">
						<span class="label">Template hash</span>
						<Hash value={box.template_hash} copy={false} head={12} />
					</div>
				{/if}
				<pre class="mono hex">{box.ergo_tree}</pre>
			</div>
		</details>
	{/if}
</Panel>

{#if box.tokens.length > 0}
	<Panel title={`Tokens (${box.tokens.length})`}>
		<Table>
			{#snippet head()}
				<tr>
					<th>Token id</th>
					<th>Amount</th>
				</tr>
			{/snippet}
			{#each box.tokens as token, i (i)}
				<tr>
					<td><span class="mono" title={token.id}>{truncateMiddle(token.id)}</span></td>
					<td class="mono">{formatNano(token.amount)}</td>
				</tr>
			{/each}
		</Table>
	</Panel>
{/if}

{#if hasRegisters}
	<Panel title="Registers">
		<div class="registers">
			<RegistersTable registers={box.registers} />
		</div>
	</Panel>
{/if}

<Panel title="Rent">
	<div class="facts">
		<div class="fact">
			<span class="label">Maturity height</span>
			{#if maturityReached}
				<a href={`/blocks/${box.rent.maturity_height}`}>{box.rent.maturity_height}</a>
			{:else}
				<span>{box.rent.maturity_height}</span>
			{/if}
		</div>
		<div class="fact">
			<span class="label">Due rent</span>
			<Amount nano={box.rent.due_nano} maxFrac={9} />
		</div>
	</div>

	<div class="rent-status">
		{#if box.rent.claimable_at_tip}
			<Badge tone="danger">Claimable now</Badge>
		{:else if box.spent_by}
			<span class="muted">Rent no longer applies (box spent)</span>
		{:else if blocksLeft !== null}
			<span
				>Matures in {blocksLeft} blocks (≈ {daysLeft} day{daysLeft === 1 ? '' : 's'} at {MINUTES_PER_BLOCK}
				min/block)</span
			>
		{/if}
	</div>
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
	.muted {
		color: var(--fg-muted);
	}
	.ergo-tree {
		border-top: 1px solid var(--border);
		padding: var(--space-2) var(--space-4);
	}
	.ergo-tree summary {
		cursor: pointer;
		color: var(--fg-muted);
	}
	.ergo-tree-body {
		margin-top: var(--space-2);
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.hex {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		padding: var(--space-2);
		margin: 0;
	}
	.registers {
		padding: 0 var(--space-4);
	}
	.rent-status {
		padding: var(--space-2) var(--space-4);
		border-top: 1px solid var(--border);
	}
</style>
