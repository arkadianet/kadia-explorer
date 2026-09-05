<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import Table from '$lib/components/Table.svelte';
	import ErrorState from '$lib/components/ErrorState.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import { status as statusStore } from '$lib/status/status.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// Prefer the layout's live-polled status once it has a value, so the banner reflects the
	// same 5 s-fresh state as the header badge instead of going stale after the initial load.
	const status = $derived(statusStore.current ?? data.status.data);
	const statusError = $derived(statusStore.current ? null : data.status.error);

	const tip = $derived(status?.best ?? null);

	const showLagBanner = $derived(
		status !== null && status !== undefined && (status.lag_blocks > 100 || status.halted !== null)
	);
	const showStalledBanner = $derived(status?.stalled ?? null);
</script>

<svelte:head>
	<title>Ergo Explorer</title>
</svelte:head>

{#if showStalledBanner}
	<div class="banner danger" role="alert">
		Indexer is waiting on block {showStalledBanner.height} from the node for {showStalledBanner.since_secs}
		s.
	</div>
{:else if showLagBanner && status}
	<div class="banner warn" role="alert">
		Index is {status.lag_blocks} blocks behind the node.
	</div>
{:else if statusError}
	<div class="banner">
		<ErrorState error={statusError} />
	</div>
{/if}

<div class="grid">
	<Panel title="Latest blocks">
		{#if data.blocks.error}
			<ErrorState error={data.blocks.error} />
		{:else if data.blocks.data && data.blocks.data.items.length === 0}
			<EmptyState message="No blocks yet." />
		{:else if data.blocks.data}
			<Table>
				{#snippet head()}
					<tr>
						<th>Height</th>
						<th>Age</th>
						<th>Txs</th>
						<th>Reward</th>
						<th>Miner</th>
					</tr>
				{/snippet}
				{#each data.blocks.data.items as block (block.id)}
					<tr>
						<td><a href={`/blocks/${block.height}`}>{block.height}</a></td>
						<td><Age ms={block.timestamp} /></td>
						<td>{block.tx_count}</td>
						<td><Amount nano={block.reward} maxFrac={3} /></td>
						<td><Hash value={block.miner_pk} copy={false} /></td>
					</tr>
				{/each}
			</Table>
		{/if}
	</Panel>

	<Panel title="Latest transactions">
		{#if data.txs.error}
			<ErrorState error={data.txs.error} />
		{:else if data.txs.data && data.txs.data.items.length === 0}
			<EmptyState message="No transactions yet." />
		{:else if data.txs.data}
			<Table>
				{#snippet head()}
					<tr>
						<th>Id</th>
						<th>Age</th>
						<th>Fee</th>
						<th>Outputs</th>
					</tr>
				{/snippet}
				{#each data.txs.data.items as tx (tx.id)}
					<tr>
						<td><Hash value={tx.id} href={`/txs/${tx.id}`} copy={false} /></td>
						<td><Age ms={tx.timestamp} /></td>
						<td><Amount nano={tx.fee} maxFrac={3} /></td>
						<td>{tx.outputs.length}</td>
					</tr>
				{/each}
			</Table>
		{/if}
	</Panel>

	<Panel title="Rent maturing soon">
		{#if data.rent.error}
			<ErrorState error={data.rent.error} />
		{:else if data.rent.data && data.rent.data.length === 0}
			<EmptyState message="No rent maturing soon." />
		{:else if data.rent.data}
			<Table>
				{#snippet head()}
					<tr>
						<th>Value</th>
						<th>Due</th>
						<th>Matures in</th>
						<th>Address</th>
					</tr>
				{/snippet}
				{#each data.rent.data as item (item.box.id)}
					<tr>
						<td><Amount nano={item.box.value} maxFrac={3} /></td>
						<td><Amount nano={item.box.rent.due_nano} maxFrac={3} /></td>
						<td>
							{tip !== null ? `${item.maturity_height - tip} blocks` : `at ${item.maturity_height}`}
						</td>
						<td>
							{#if item.box.address}
								<Hash value={item.box.address} copy={false} />
							{:else}
								—
							{/if}
						</td>
					</tr>
				{/each}
			</Table>
		{/if}
	</Panel>
</div>

<style>
	.banner {
		margin-bottom: var(--space-3);
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius);
		border: 1px solid var(--border);
		background: var(--bg-elev);
	}
	.banner.warn {
		border-color: var(--warn);
		color: var(--warn);
	}
	.banner.danger {
		border-color: var(--danger);
		color: var(--danger);
	}
	.grid {
		display: grid;
		gap: var(--space-4);
		grid-template-columns: 1fr;
	}
	@media (min-width: 1024px) {
		.grid {
			grid-template-columns: repeat(2, 1fr);
		}
		.grid > :global(:last-child) {
			grid-column: 1 / -1;
		}
	}
</style>
