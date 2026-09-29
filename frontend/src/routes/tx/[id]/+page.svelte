<script lang="ts">
	import { untrack } from 'svelte';
	import { page } from '$app/state';
	import { api } from '$lib/api/endpoints';
	import { createTransactionTracker, type TrackingState } from '$lib/tx/tracker';
	import TransactionTracking from '$lib/components/TransactionTracking.svelte';
	import PendingTransactionDetails from '$lib/components/PendingTransactionDetails.svelte';
	import TransactionReceipt from '$lib/components/TransactionReceipt.svelte';
	import ApplicationWorkflow from '$lib/components/ApplicationWorkflow.svelte';
	import RosenWorkflow from '$lib/components/RosenWorkflow.svelte';
	import { transactionDeposits } from '$lib/apps/rosen';
	import { transactionOrders } from '$lib/tx/spectrum';
	import Panel from '$lib/components/Panel.svelte';
	import PageHead from '$lib/components/PageHead.svelte';
	import Facts from '$lib/components/Facts.svelte';
	import Fact from '$lib/components/Fact.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import BoxCard from '$lib/components/BoxCard.svelte';
	import { sumNano } from '$lib/format/amount';
	import { formatKb } from '$lib/format/size';
	import { absTime } from '$lib/format/time';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
	let tracked = $state<{ id: string; value: TrackingState } | null>(null);
	let controller: ReturnType<typeof createTransactionTracker> | undefined;
	const tracking = $derived(
		tracked?.id === data.id
			? tracked.value
			: {
					tx: data.tx,
					status: data.tracking,
					checking: false,
					error: null,
					unsupported: false
				}
	);
	const tx = $derived(tracking.tx);
	const applicationOrders = $derived(tx ? transactionOrders(tx) : []);
	const bridgeDeposits = $derived(tx ? transactionDeposits(tx) : []);
	// A live inclusion check updates confirmations, not mutable box-spend/rent details.
	const receiptTx = $derived(
		tx && tracking.status?.state === 'confirmed'
			? {
					...tx,
					indexed_height: tracking.status.indexed_height,
					confirmations: tracking.status.inclusion?.confirmations ?? null
				}
			: tx
	);
	function refresh() {
		void controller?.refresh(true);
	}
	$effect(() => {
		const initial = data;
		const current = createTransactionTracker({
			id: initial.id,
			tx: initial.tx,
			status: initial.tracking,
			getStatus: () => api.txStatus(initial.id),
			getTx: () => api.tx(initial.id),
			onChange: (value) => {
				tracked = { id: initial.id, value };
			}
		});
		controller = current;
		const visibility = () => current.setVisible(document.visibilityState === 'visible');
		document.addEventListener('visibilitychange', visibility);
		untrack(visibility);
		return () => {
			current.stop();
			document.removeEventListener('visibilitychange', visibility);
		};
	});
	const tip = $derived(tx?.indexed_height ?? null);

	const knownInputValues = $derived(tx?.inputs.flatMap((i) => (i.box ? [i.box.value] : [])) ?? []);
	// Inputs whose box predates the indexed range have no value we can add up, so the "in"
	// total is explicitly marked partial rather than silently under-reporting.
	const partialIn = $derived(knownInputValues.length !== tx?.inputs.length);

	const totalIn = $derived(sumNano(knownInputValues).toString());
	const totalOut = $derived(sumNano(tx?.outputs.map((o) => o.value) ?? []).toString());
</script>

<svelte:head>
	<title>Transaction {data.id} — Ergo Explorer</title>
</svelte:head>

<div class="head">
	<PageHead title="Transaction" id={data.id} />
	<TransactionTracking {tracking} onrefresh={refresh} />
	{#if !tx && tracking.status?.pending && tracking.status.state !== 'confirmed'}
		{#key data.id}<PendingTransactionDetails
				observation={tracking.status}
				stale={!!tracking.error}
			/>{/key}
	{/if}
	{#if tx && receiptTx}
		{#key `${tx.id}:${tx.block_id}:${tx.height}`}<TransactionReceipt
				tx={receiptTx}
				perspectiveAddress={page.url.searchParams.get('address')}
				refreshing={tracking.checking}
				onrefresh={refresh}
			/>{/key}

		<Facts>
			<Fact label="Block"><a class="mono" href={`/blocks/${tx.height}`}>{tx.height}</a></Fact>
			<Fact label="Mined"><span>{absTime(tx.timestamp)}, <Age ms={tx.timestamp} /></span></Fact>
			<Fact label="Index in block"><span class="mono">{tx.index}</span></Fact>
			<Fact label="Size"><span class="mono">{formatKb(tx.size)}</span></Fact>
			<Fact label={partialIn ? 'Total in (known boxes)' : 'Total in'}>
				<Amount nano={totalIn} maxFrac={9} />
			</Fact>
			<Fact label="Total out"><Amount nano={totalOut} maxFrac={9} /></Fact>
			<Fact label="Fee"><Amount nano={tx.fee} maxFrac={9} /></Fact>
			{#if tx.data_inputs.length > 0}
				<Fact label="Data inputs">
					<ul class="data-inputs">
						{#each tx.data_inputs as id, i (i)}
							<li><Hash value={id} href={`/box/${id}`} copy={false} head={12} /></li>
						{/each}
					</ul>
				</Fact>
			{/if}
		</Facts>
	{/if}
</div>

{#key tx?.block_id}{#each applicationOrders as order (order.box.id)}<ApplicationWorkflow
			{order}
		/>{/each}{/key}
{#key tx?.block_id}{#each bridgeDeposits as deposit (deposit.box.id)}<RosenWorkflow
			{deposit}
		/>{/each}{/key}
{#if tx}<p class="investigation-link">
		<a href={`/investigate?kind=tx&id=${tx.id}`}>Investigate this transaction ↗</a> ·
		<a href={`/applications?kind=tx&id=${tx.id}`}>Inspect application activity ↗</a>
	</p>{/if}

<!-- Inputs on the left, outputs on the right, parted by a rule that the two column headings
     label. No arrow glyph: the headings already say which side is which. -->
{#if tx}<p class="detail-snapshot">
		{#if tx.indexed_height != null}Box details were checked at indexed block {tx.indexed_height.toLocaleString(
				'en-US'
			)}.{:else}The box-detail snapshot height is unavailable on this server.{/if} Use Refresh to recheck
		their spend and rent status.
	</p>
	<div class="flow">
		<Panel title={`Inputs (${tx.inputs.length})`}>
			<div class="cards">
				{#each tx.inputs as input (input.id)}
					{#if input.box}
						<BoxCard box={input.box} {tip} role="input" />
					{:else}
						<article class="box unknown">
							<span>Spends a box created before the indexed range</span>
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
{/if}

<style>
	.investigation-link {
		font-size: var(--fs-data);
		grid-column: 1 / -1;
	}
	.investigation-link a {
		color: var(--accent-ink);
		text-decoration: underline;
		text-underline-offset: 0.25em;
	}
	.detail-snapshot {
		font-size: 12px;
		color: var(--fg-muted);
	}
	.head {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.data-inputs li {
		margin-top: 2px;
	}
	.flow {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: var(--space-6);
	}
	@media (min-width: 900px) {
		.flow {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			align-items: start;
			gap: var(--space-8);
		}
		.flow > :global(:last-child) {
			border-left: var(--rule);
			padding-left: calc(var(--space-8) / 2);
			margin-left: calc(var(--space-8) / -2);
		}
	}
	.cards {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.box.unknown {
		border-left: 2px dashed var(--hairline);
		padding: var(--space-2) 0 var(--space-2) var(--space-3);
		display: flex;
		flex-direction: column;
		gap: 2px;
		color: var(--fg-muted);
		font-size: var(--fs-data);
	}
</style>
