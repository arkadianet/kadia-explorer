<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Panel from '$lib/components/Panel.svelte';
	import Table from '$lib/components/Table.svelte';
	import Tabs from '$lib/components/Tabs.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import ErrorState from '$lib/components/ErrorState.svelte';
	import Skeleton from '$lib/components/Skeleton.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager } from '$lib/pager/pager.svelte';
	import { status } from '$lib/status/status.svelte';
	import { formatErg, sumNano } from '$lib/format/amount';
	import { normalizeUpcoming } from './upcoming';
	import type { RentItemDto } from '$lib/api/types';
	import type { PageData } from './$types';

	const UPCOMING_OPTIONS = [72, 720, 2160] as const;

	const TABS = [
		{ id: 'upcoming', label: 'Upcoming' },
		{ id: 'eligible', label: 'Eligible' }
	];
	const TAB_IDS = TABS.map((t) => t.id);

	let { data }: { data: PageData } = $props();

	/** Active tab, driven by the URL hash so a tab is linkable and survives reload. */
	const active = $derived.by(() => {
		const id = page.url.hash.replace(/^#/, '');
		return TAB_IDS.includes(id) ? id : 'upcoming';
	});

	function selectTab(id: string) {
		void goto(`#${id}`, { replaceState: true, noScroll: true, keepFocus: true });
	}

	// Upcoming: a plain array reloaded whenever N changes. Seeded from the server load so the
	// default (720) renders without a client round trip.
	let upcomingBlocks = $state(720);
	let upcomingItems = $state<RentItemDto[]>(untrack(() => data.upcoming));
	let upcomingLoading = $state(false);
	let upcomingError = $state<unknown>(null);

	async function loadUpcoming() {
		upcomingLoading = true;
		upcomingError = null;
		try {
			upcomingItems = normalizeUpcoming(await api.rentUpcoming(upcomingBlocks, 100));
		} catch (e) {
			upcomingError = e;
		} finally {
			upcomingLoading = false;
		}
	}

	function onBlocksChange(e: Event) {
		upcomingBlocks = Number((e.target as HTMLSelectElement).value);
		void loadUpcoming();
	}

	const upcomingTotalDue = $derived(
		formatErg(sumNano(upcomingItems.map((i) => i.box.rent.due_nano)))
	);

	const tip = $derived(status.current?.indexed ?? null);

	// Eligible: cursor pager, created lazily on first activation.
	const eligiblePager = createPager<RentItemDto>((cursor) => api.rentEligible(cursor, 50));
	let eligibleStarted = false;

	$effect(() => {
		if (active === 'eligible' && !eligibleStarted) {
			eligibleStarted = true;
			void eligiblePager.loadMore();
		}
	});

	const eligibleTotalDue = $derived(
		formatErg(sumNano(eligiblePager.items.map((i) => i.box.rent.due_nano)))
	);
</script>

<svelte:head>
	<title>Rent — Ergo Explorer</title>
</svelte:head>

<Panel title="Rent">
	<div class="tabbar">
		<Tabs tabs={TABS} {active} onchange={selectTab} />
	</div>

	<div role="tabpanel" aria-labelledby={`tab-${active}`}>
		{#if active === 'upcoming'}
			<div class="controls">
				<label for="blocks">Horizon</label>
				<select id="blocks" value={upcomingBlocks} onchange={onBlocksChange}>
					{#each UPCOMING_OPTIONS as n (n)}
						<option value={n}>{n} blocks</option>
					{/each}
				</select>
			</div>

			{#if upcomingLoading}
				<Skeleton />
			{:else if upcomingError}
				<ErrorState error={upcomingError} retry={() => void loadUpcoming()} />
			{:else if upcomingItems.length > 0}
				<p class="sum">
					{upcomingItems.length} boxes, total due <span class="mono">{upcomingTotalDue}</span> ERG
				</p>
				<Table dense>
					{#snippet head()}
						<tr>
							<th>Matures at</th>
							<th>Box</th>
							<th>Value</th>
							<th>Due rent</th>
							<th>Address</th>
						</tr>
					{/snippet}
					{#each upcomingItems as item (item.box.id)}
						<tr>
							<td>
								<a href={`/blocks/${item.maturity_height}`}>{item.maturity_height}</a>
								{#if tip !== null}
									<span class="muted">in {item.maturity_height - tip} blocks</span>
								{/if}
							</td>
							<td><Hash value={item.box.id} href={`/box/${item.box.id}`} copy={false} /></td>
							<td><Amount nano={item.box.value} maxFrac={9} /></td>
							<td><Amount nano={item.box.rent.due_nano} maxFrac={9} /></td>
							<td>
								{#if item.box.address}
									<Hash
										value={item.box.address}
										href={`/address/${item.box.address}`}
										copy={false}
									/>
								{:else}
									<span class="muted" title="script address unavailable">
										<Hash value={item.box.tree_hash} copy={false} />
									</span>
								{/if}
							</td>
						</tr>
					{/each}
				</Table>
			{:else}
				<EmptyState message="No boxes maturing in this window." />
			{/if}
		{:else}
			{#if eligiblePager.items.length > 0}
				<p class="sum">
					{eligiblePager.items.length} boxes{#if !eligiblePager.done}&nbsp;loaded so far{/if}, total
					due <span class="mono">{eligibleTotalDue}</span> ERG
				</p>
			{/if}
			<InfiniteList
				table
				columns={5}
				dense
				pager={eligiblePager}
				empty="No claimable rent right now."
			>
				{#snippet head()}
					<tr>
						<th>Matures at</th>
						<th>Box</th>
						<th>Value</th>
						<th>Due rent</th>
						<th>Address</th>
					</tr>
				{/snippet}
				{#snippet children(item: RentItemDto)}
					<tr>
						<td>
							<a href={`/blocks/${item.maturity_height}`}>{item.maturity_height}</a>
							<Badge tone="danger">claimable</Badge>
						</td>
						<td><Hash value={item.box.id} href={`/box/${item.box.id}`} copy={false} /></td>
						<td><Amount nano={item.box.value} maxFrac={9} /></td>
						<td><Amount nano={item.box.rent.due_nano} maxFrac={9} /></td>
						<td>
							{#if item.box.address}
								<Hash value={item.box.address} href={`/address/${item.box.address}`} copy={false} />
							{:else}
								<span class="muted" title="script address unavailable">
									<Hash value={item.box.tree_hash} copy={false} />
								</span>
							{/if}
						</td>
					</tr>
				{/snippet}
			</InfiniteList>
		{/if}
	</div>
</Panel>

<style>
	.tabbar {
		padding: 0 var(--space-4);
	}
	.controls {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
	}
	.sum {
		color: var(--fg-muted);
		padding: 0 var(--space-4) var(--space-2);
	}
	.muted {
		color: var(--fg-muted);
	}
</style>
