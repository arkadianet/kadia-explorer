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
	// Seeded from the load function, which returns an `ApiError` as data rather than throwing
	// so the tabs still render (and the Eligible tab still works) when `/rent/upcoming` is down.
	let upcomingError = $state<unknown>(untrack(() => data.error));

	// Request generation counter: guards against a slower, earlier request (e.g. the user
	// flips the N-selector twice in quick succession) overwriting the result of a later one.
	let upcomingGen = 0;

	async function loadUpcoming() {
		const my = ++upcomingGen;
		upcomingLoading = true;
		upcomingError = null;
		try {
			const page = await api.rentUpcoming(upcomingBlocks, 100);
			if (my !== upcomingGen) return;
			upcomingItems = page.items;
		} catch (e) {
			if (my !== upcomingGen) return;
			upcomingError = e;
		} finally {
			if (my === upcomingGen) upcomingLoading = false;
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
	<Tabs tabs={TABS} {active} onchange={selectTab} label="Rent sections" />

	<div role="tabpanel" id={`panel-${active}`} tabindex="0" aria-labelledby={`tab-${active}`}>
		{#if active === 'upcoming'}
			<div class="controls">
				<label for="blocks">Horizon</label>
				<select
					id="blocks"
					value={upcomingBlocks}
					disabled={upcomingLoading}
					onchange={onBlocksChange}
				>
					{#each UPCOMING_OPTIONS as n (n)}
						<option value={n}>{n} blocks</option>
					{/each}
				</select>
				{#if upcomingLoading}
					<span class="muted loading">Loading…</span>
				{/if}
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
								<!-- Only link once the block exists: a maturity height above the tip has no
								     block page yet, so linking it would be a guaranteed 404. -->
								{#if tip !== null && item.maturity_height <= tip}
									<a href={`/blocks/${item.maturity_height}`}>{item.maturity_height}</a>
								{:else}
									{item.maturity_height}
								{/if}
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
				<EmptyState message="No box matures within this many blocks. Try a longer horizon." />
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
				empty="No box has reached its storage-rent maturity yet."
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
	.controls {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-3) 0;
		font-size: var(--fs-data);
	}
	.controls select {
		font: inherit;
		color: var(--fg);
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: var(--radius-control);
		padding: var(--space-1) var(--space-2);
	}
	.sum {
		color: var(--fg-muted);
		padding-bottom: var(--space-3);
		font-size: var(--fs-data);
	}
	.muted {
		color: var(--fg-muted);
	}
</style>
