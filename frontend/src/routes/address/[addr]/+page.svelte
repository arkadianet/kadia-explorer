<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Panel from '$lib/components/Panel.svelte';
	import Table from '$lib/components/Table.svelte';
	import Tabs from '$lib/components/Tabs.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import RentBadge from '$lib/components/RentBadge.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import ErrorState from '$lib/components/ErrorState.svelte';
	import Skeleton from '$lib/components/Skeleton.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager, type Pager } from '$lib/pager/pager.svelte';
	import { status } from '$lib/status/status.svelte';
	import { formatErg, formatNano, sumNano } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import type { BoxDto, TxDto } from '$lib/api/types';
	import type { PageData } from './$types';

	const PAGE_SIZE = 50;

	const TABS = [
		{ id: 'txs', label: 'Transactions' },
		{ id: 'unspent', label: 'Unspent boxes' },
		{ id: 'boxes', label: 'All boxes' },
		{ id: 'rent', label: 'Rent' }
	];
	const TAB_IDS = TABS.map((t) => t.id);

	let { data }: { data: PageData } = $props();

	const addr = $derived(data.addr);
	const info = $derived(data.info);
	const tip = $derived(status.current?.indexed ?? null);

	/** Active tab, driven by the URL hash so a tab is linkable and survives reload. */
	const active = $derived.by(() => {
		const id = page.url.hash.replace(/^#/, '');
		return TAB_IDS.includes(id) ? id : 'txs';
	});

	function selectTab(id: string) {
		// `replaceState` so flipping between tabs does not pile up history entries.
		void goto(`#${id}`, { replaceState: true, noScroll: true, keepFocus: true });
	}

	// Each tab's data source is created on first activation, so opening the page costs one
	// request (the txs page) rather than four.
	let txPager = $state<Pager<TxDto> | null>(null);
	let unspentPager = $state<Pager<BoxDto> | null>(null);
	let boxPager = $state<Pager<BoxDto> | null>(null);

	let rentItems = $state<BoxDto[] | null>(null);
	let rentTruncated = $state(false);
	let rentLoading = $state(false);
	let rentError = $state<unknown>(null);

	async function loadRent() {
		rentLoading = true;
		rentError = null;
		try {
			const res = await api.addressRent(addr);
			rentItems = [...res.items].sort((a, b) => a.rent.maturity_height - b.rent.maturity_height);
			rentTruncated = res.truncated;
		} catch (e) {
			rentError = e;
		} finally {
			rentLoading = false;
		}
	}

	// Navigating from one address to another reuses this component, so the lazily-created
	// per-tab state has to be dropped explicitly or the new address shows the old one's rows.
	// Plain `let` (not `$state`) so reading it here does not make this effect depend on it.
	let loadedFor: string | null = null;

	$effect(() => {
		if (addr === loadedFor) return;
		loadedFor = addr;
		txPager = null;
		unspentPager = null;
		boxPager = null;
		rentItems = null;
		rentTruncated = false;
		rentLoading = false;
		rentError = null;
	});

	$effect(() => {
		if (info === null) return;
		if (active === 'txs' && !txPager) {
			const p = createPager<TxDto>((c) => api.addressTxs(addr, c, PAGE_SIZE));
			txPager = p;
			void p.loadMore();
		} else if (active === 'unspent' && !unspentPager) {
			const p = createPager<BoxDto>((c) => api.addressBoxes(addr, true, c, PAGE_SIZE));
			unspentPager = p;
			void p.loadMore();
		} else if (active === 'boxes' && !boxPager) {
			const p = createPager<BoxDto>((c) => api.addressBoxes(addr, false, c, PAGE_SIZE));
			boxPager = p;
			void p.loadMore();
		} else if (active === 'rent' && rentItems === null && !rentLoading && rentError === null) {
			void loadRent();
		}
	});

	const unspentTotal = $derived(
		unspentPager ? formatErg(sumNano(unspentPager.items.map((b) => b.value)), { maxFrac: 9 }) : '0'
	);
</script>

<svelte:head>
	<title>Address {truncateMiddle(addr, 10, 8)} — Ergo Explorer</title>
</svelte:head>

{#if info === null}
	<Panel title="Address">
		<div class="not-seen">
			<p class="mono break">{addr}</p>
			<EmptyState
				message="Address not seen yet — no boxes for it in the index. It may be unfunded, or the indexer may not have reached its first transaction."
			/>
		</div>
	</Panel>
{:else}
	<Panel title="Address">
		<div class="head">
			<Hash value={info.address} head={24} tail={16} />
			<div class="balance"><Amount nano={info.balance.nano} maxFrac={9} /></div>
		</div>
		<div class="facts">
			<div class="fact">
				<span class="label">Boxes</span>
				<span>{info.box_count}</span>
			</div>
			<div class="fact">
				<span class="label">Tokens</span>
				<span>{info.balance.tokens.length}</span>
			</div>
			<div class="fact">
				<span class="label">First seen</span>
				<a href={`/blocks/${info.first_seen}`}>{info.first_seen}</a>
			</div>
			<div class="fact">
				<span class="label">Last seen</span>
				<a href={`/blocks/${info.last_seen}`}>{info.last_seen}</a>
			</div>
		</div>
	</Panel>

	{#if info.balance.tokens.length > 0}
		<Panel title={`Tokens (${info.balance.tokens.length})`}>
			<Table>
				{#snippet head()}
					<tr>
						<th>Token id</th>
						<th>Amount</th>
					</tr>
				{/snippet}
				{#each info.balance.tokens as token (token.id)}
					<tr>
						<td><span class="mono" title={token.id}>{truncateMiddle(token.id)}</span></td>
						<td class="mono">{formatNano(token.amount)}</td>
					</tr>
				{/each}
			</Table>
		</Panel>
	{/if}

	<Panel>
		<div class="tabbar">
			<Tabs tabs={TABS} {active} onchange={selectTab} />
		</div>

		<div role="tabpanel" aria-labelledby={`tab-${active}`}>
			{#if active === 'txs'}
				{#if txPager}
					<InfiniteList table columns={4} dense pager={txPager} empty="No transactions.">
						{#snippet head()}
							<tr>
								<th>Id</th>
								<th>Block</th>
								<th>Age</th>
								<th>Fee</th>
							</tr>
						{/snippet}
						{#snippet children(tx: TxDto)}
							<tr>
								<td><Hash value={tx.id} href={`/tx/${tx.id}`} copy={false} /></td>
								<td><a href={`/blocks/${tx.height}`}>{tx.height}</a></td>
								<td><Age ms={tx.timestamp} /></td>
								<td><Amount nano={tx.fee} maxFrac={3} /></td>
							</tr>
						{/snippet}
					</InfiniteList>
				{:else}
					<Skeleton />
				{/if}
			{:else if active === 'unspent'}
				{#if unspentPager}
					<InfiniteList table columns={5} dense pager={unspentPager} empty="No unspent boxes.">
						{#snippet head()}
							<tr>
								<th>Id</th>
								<th>Value</th>
								<th>Created</th>
								<th>Tokens</th>
								<th>Rent</th>
							</tr>
						{/snippet}
						{#snippet children(box: BoxDto)}
							<tr>
								<td><Hash value={box.id} href={`/box/${box.id}`} copy={false} /></td>
								<td><Amount nano={box.value} maxFrac={9} /></td>
								<td><a href={`/blocks/${box.creation_height}`}>{box.creation_height}</a></td>
								<td>{box.tokens.length}</td>
								<td><RentBadge rent={box.rent} {tip} /></td>
							</tr>
						{/snippet}
					</InfiniteList>
					{#if unspentPager.items.length > 0}
						<p class="sum">
							Total{#if !unspentPager.done}&nbsp;loaded so far{/if}:
							<span class="mono">{unspentTotal}</span> ERG
						</p>
					{/if}
				{:else}
					<Skeleton />
				{/if}
			{:else if active === 'boxes'}
				{#if boxPager}
					<InfiniteList table columns={6} dense pager={boxPager} empty="No boxes.">
						{#snippet head()}
							<tr>
								<th>Id</th>
								<th>Value</th>
								<th>Created</th>
								<th>Spent</th>
								<th>Tokens</th>
								<th>Rent</th>
							</tr>
						{/snippet}
						{#snippet children(box: BoxDto)}
							<tr>
								<td><Hash value={box.id} href={`/box/${box.id}`} copy={false} /></td>
								<td><Amount nano={box.value} maxFrac={9} /></td>
								<td><a href={`/blocks/${box.creation_height}`}>{box.creation_height}</a></td>
								<td>
									{#if box.spent_by}
										<Hash value={box.spent_by} href={`/tx/${box.spent_by}`} copy={false} />
									{:else}
										<span class="muted">—</span>
									{/if}
								</td>
								<td>{box.tokens.length}</td>
								<td>
									{#if box.spent_by}
										<span class="muted" title="Rent no longer applies — the box is spent">—</span>
									{:else}
										<RentBadge rent={box.rent} {tip} />
									{/if}
								</td>
							</tr>
						{/snippet}
					</InfiniteList>
				{:else}
					<Skeleton />
				{/if}
			{:else if rentLoading && rentItems === null}
				<Skeleton />
			{:else if rentError}
				<ErrorState error={rentError} retry={() => void loadRent()} />
			{:else if rentItems && rentItems.length > 0}
				{#if rentTruncated}
					<p class="notice">
						Showing the earliest-maturing boxes only — this address has more rent-bearing boxes than
						the API returns in one response.
					</p>
				{/if}
				<Table dense>
					{#snippet head()}
						<tr>
							<th>Id</th>
							<th>Value</th>
							<th>Due rent</th>
							<th>Maturity height</th>
							<th>Matures</th>
						</tr>
					{/snippet}
					{#each rentItems as box (box.id)}
						<tr>
							<td><Hash value={box.id} href={`/box/${box.id}`} copy={false} /></td>
							<td><Amount nano={box.value} maxFrac={9} /></td>
							<td><Amount nano={box.rent.due_nano} maxFrac={9} /></td>
							<td>{box.rent.maturity_height}</td>
							<td>
								{#if box.rent.claimable_at_tip}
									<Badge tone="danger">claimable</Badge>
								{:else if tip !== null}
									in {box.rent.maturity_height - tip} blocks
								{:else}
									<span class="muted">—</span>
								{/if}
							</td>
						</tr>
					{/each}
				</Table>
			{:else if rentItems}
				<EmptyState message="No rent-bearing boxes at this address." />
			{:else}
				<Skeleton />
			{/if}
		</div>
	</Panel>
{/if}

<style>
	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2) var(--space-4);
		padding: var(--space-2) var(--space-4);
	}
	.balance {
		font-size: 20px;
	}
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
	.tabbar {
		padding: 0 var(--space-4);
	}
	.sum {
		padding: var(--space-2) var(--space-4);
		border-top: 1px solid var(--border);
		color: var(--fg-muted);
	}
	.notice {
		padding: var(--space-2) var(--space-4);
		color: var(--warn);
	}
	.not-seen {
		padding: var(--space-2) var(--space-4);
	}
	.break {
		overflow-wrap: anywhere;
	}
</style>
