<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import PageHead from '$lib/components/PageHead.svelte';
	import Facts from '$lib/components/Facts.svelte';
	import Fact from '$lib/components/Fact.svelte';
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

<div class="head">
	<PageHead title="Box" id={box.id} />

	<Facts>
		<Fact label="Value"><Amount nano={box.value} maxFrac={9} /></Fact>
		<Fact label="Address">
			{#if box.address}
				<Hash value={box.address} href={`/address/${box.address}`} copy={false} head={12} />
			{:else}
				<span class="muted" title="No P2PK/P2S address — showing the ergo tree hash">
					<Hash value={box.tree_hash} copy={false} head={12} />
				</span>
			{/if}
		</Fact>
		<Fact label="Created in tx">
			<Hash value={box.tx_id} href={`/tx/${box.tx_id}`} copy={false} head={12} />
			<span class="muted">at output {box.index}</span>
		</Fact>
		<Fact label="Creation height">
			<a class="mono" href={`/blocks/${box.creation_height}`}>{box.creation_height}</a>
		</Fact>
		<Fact label="Size"><span class="mono">{box.size} bytes</span></Fact>
		<Fact label="Status">
			{#if box.spent_by}
				<span>
					<Badge tone="neutral">Spent</Badge>
					by
					<Hash value={box.spent_by} href={`/tx/${box.spent_by}`} copy={false} head={10} />
					{#if box.spent_height !== null}
						<span class="muted">at</span>
						<a class="mono" href={`/blocks/${box.spent_height}`}>{box.spent_height}</a>
					{/if}
				</span>
			{:else}
				<Badge tone="ok">Unspent</Badge>
			{/if}
		</Fact>
	</Facts>

	{#if box.ergo_tree}
		<details class="ergo-tree">
			<summary>Ergo tree</summary>
			<div class="ergo-tree-body">
				{#if box.template_hash}
					<p class="template">
						<span class="muted">Template hash</span>
						<Hash value={box.template_hash} copy={false} head={12} />
					</p>
				{/if}
				<pre class="mono hex">{box.ergo_tree}</pre>
			</div>
		</details>
	{/if}
</div>

{#if box.tokens.length > 0}
	<Panel title={`Tokens (${box.tokens.length})`}>
		<Table>
			{#snippet head()}
				<tr>
					<th>Token id</th>
					<th class="num">Amount</th>
				</tr>
			{/snippet}
			{#each box.tokens as token, i (i)}
				<tr>
					<td><span class="mono" title={token.id}>{truncateMiddle(token.id)}</span></td>
					<td class="num mono">{formatNano(token.amount)}</td>
				</tr>
			{/each}
		</Table>
	</Panel>
{/if}

{#if hasRegisters}
	<Panel title="Registers">
		<RegistersTable registers={box.registers} />
	</Panel>
{/if}

<Panel title="Rent">
	<Facts>
		<Fact label="Maturity height">
			{#if maturityReached}
				<a class="mono" href={`/blocks/${box.rent.maturity_height}`}>{box.rent.maturity_height}</a>
			{:else}
				<span class="mono">{box.rent.maturity_height}</span>
			{/if}
		</Fact>
		<Fact label="Due rent"><Amount nano={box.rent.due_nano} maxFrac={9} /></Fact>
	</Facts>

	<p class="rent-status">
		{#if box.rent.claimable_at_tip}
			<Badge tone="danger">Claimable now</Badge>
			<span class="muted">A miner may take the rent from this box in the next block.</span>
		{:else if box.spent_by}
			<span class="muted">Rent no longer applies — the box is spent.</span>
		{:else if blocksLeft !== null}
			<span class="muted"
				>Matures in {blocksLeft} blocks, about {daysLeft} day{daysLeft === 1 ? '' : 's'} at {MINUTES_PER_BLOCK}
				min/block.</span
			>
		{/if}
	</p>
</Panel>

<style>
	.head {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.muted {
		color: var(--fg-muted);
	}
	.ergo-tree {
		font-size: var(--fs-data);
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
		background: var(--surface-solid);
		padding: var(--space-3);
		margin: 0;
		border-radius: var(--radius-control);
	}
	.rent-status {
		margin-top: var(--space-3);
		font-size: var(--fs-data);
	}
</style>
