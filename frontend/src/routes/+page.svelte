<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import Table from '$lib/components/Table.svelte';
	import ErrorState from '$lib/components/ErrorState.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import BlockStrip from '$lib/components/BlockStrip.svelte';
	import MinerChip from '$lib/components/MinerChip.svelte';
	import { relTime } from '$lib/format/time';
	import { status as statusStore } from '$lib/status/status.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// Prefer the layout's live-polled status once it has a value, so the countdown reflects the
	// same 5 s-fresh state as the header badge instead of going stale after the initial load.
	// The lag/stalled banner lives in +layout.svelte, so every route shows it.
	const status = $derived(statusStore.current ?? data.status.data);

	// `indexed`, not `best`: maturity heights come from the indexer, and the API's
	// `claimable_at_tip` is measured against the indexed tip too — using the node's `best`
	// would make the countdown disagree with every other page by the current lag.
	const tip = $derived(status?.indexed ?? null);

	const blocks = $derived(data.blocks.data?.items ?? []);

	let now = $state(Date.now());
	$effect(() => {
		const id = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(id);
	});

	const latestAge = $derived(blocks.length > 0 ? relTime(blocks[0]!.timestamp, now) : '—');

	// Counted from the blocks already on screen, so it costs no extra request. If the whole
	// loaded window is younger than an hour the count is a floor, and says so.
	const lastHour = $derived.by(() => {
		if (blocks.length === 0) return null;
		const cutoff = blocks[0]!.timestamp - 3_600_000;
		const n = blocks.filter((b) => b.timestamp > cutoff).length;
		return { n, partial: n === blocks.length };
	});
</script>

<svelte:head>
	<title>Ergo Explorer</title>
</svelte:head>

{#if blocks.length > 0}
	<div class="head">
		<BlockStrip {blocks} />

		<dl class="stats">
			<div class="stat">
				<dt>Indexed height</dt>
				<dd class="mono">{tip ?? '—'}</dd>
			</div>
			<div class="stat">
				<dt>Behind the node</dt>
				<dd>
					<span class="mono">{status ? status.lag_blocks : '—'}</span>
					<span class="unit">blocks</span>
				</dd>
			</div>
			<div class="stat">
				<dt>Latest block</dt>
				<dd>{latestAge}</dd>
			</div>
			<div class="stat">
				<dt>Blocks in the last hour</dt>
				<dd class="mono">{lastHour ? `${lastHour.n}${lastHour.partial ? '+' : ''}` : '—'}</dd>
			</div>
		</dl>
	</div>
{/if}

<div class="grid">
	<Panel title="Latest blocks">
		{#if data.blocks.error}
			<ErrorState error={data.blocks.error} />
		{:else if data.blocks.data && data.blocks.data.items.length === 0}
			<EmptyState message="Blocks will appear here as the indexer catches up with the node." />
		{:else if data.blocks.data}
			<Table>
				{#snippet head()}
					<tr>
						<th>Height</th>
						<th>Age</th>
						<th class="num">Txs</th>
						<th class="num">Reward</th>
						<th>Miner</th>
					</tr>
				{/snippet}
				{#each data.blocks.data.items as block (block.id)}
					<tr>
						<td class="mono"><a href={`/blocks/${block.height}`}>{block.height}</a></td>
						<td><Age ms={block.timestamp} /></td>
						<td class="num mono">{block.tx_count}</td>
						<td class="num"><Amount nano={block.reward} maxFrac={3} /></td>
						<td><MinerChip minerPk={block.miner_pk} /></td>
					</tr>
				{/each}
			</Table>
		{/if}
	</Panel>

	<Panel title="Latest transactions">
		{#if data.txs.error}
			<ErrorState error={data.txs.error} />
		{:else if data.txs.data && data.txs.data.items.length === 0}
			<EmptyState message="Transactions will appear here as the indexer catches up." />
		{:else if data.txs.data}
			<Table>
				{#snippet head()}
					<tr>
						<th>Id</th>
						<th>Age</th>
						<th class="num">Fee</th>
						<th class="num">Outputs</th>
					</tr>
				{/snippet}
				{#each data.txs.data.items as tx (tx.id)}
					<tr>
						<td><Hash value={tx.id} href={`/tx/${tx.id}`} copy={false} /></td>
						<td><Age ms={tx.timestamp} /></td>
						<td class="num"><Amount nano={tx.fee} maxFrac={3} /></td>
						<td class="num mono">{tx.outputs.length}</td>
					</tr>
				{/each}
			</Table>
		{/if}
	</Panel>

	<Panel title="Rent maturing soon">
		{#if data.rent.error}
			<ErrorState error={data.rent.error} />
		{:else if data.rent.data && data.rent.data.length === 0}
			<EmptyState message="No box is within a day of its storage-rent maturity." />
		{:else if data.rent.data}
			<Table>
				{#snippet head()}
					<tr>
						<th class="num">Value</th>
						<th class="num">Due</th>
						<th>Matures in</th>
						<th>Address</th>
					</tr>
				{/snippet}
				{#each data.rent.data as item (item.box.id)}
					<tr>
						<td class="num"><Amount nano={item.box.value} maxFrac={3} /></td>
						<td class="num"><Amount nano={item.box.rent.due_nano} maxFrac={3} /></td>
						<td class="mono">
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
	.head {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	/* Label/value pairs on one line, separated by hairlines — the numbers are context for the
	   strip above, not headline statistics, so none of them is set large. */
	.stats {
		display: flex;
		flex-wrap: wrap;
		margin: 0;
		border-top: var(--rule);
		border-bottom: var(--rule);
	}

	.stat {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-4);
		border-left: var(--rule);
		font-size: var(--fs-data);
	}

	.stat:first-child {
		border-left: 0;
		padding-left: 0;
	}

	@media (max-width: 719px) {
		.stats {
			display: block;
		}
		.stat {
			padding: var(--space-2) 0;
			border-left: 0;
			border-top: var(--rule);
		}
		.stat:first-child {
			border-top: 0;
		}
	}

	.stat dt {
		color: var(--fg-muted);
	}

	.stat dd {
		margin: 0;
		color: var(--fg);
	}

	.stat .unit {
		color: var(--fg-muted);
	}

	.grid {
		display: grid;
		gap: var(--space-8);
		grid-template-columns: minmax(0, 1fr);
	}

	@media (min-width: 1024px) {
		.grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: var(--space-8) var(--space-6);
		}
		.grid > :global(:last-child) {
			grid-column: 1 / -1;
		}
	}
</style>
