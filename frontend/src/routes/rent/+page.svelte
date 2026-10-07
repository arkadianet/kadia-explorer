<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Panel from '$lib/components/Panel.svelte';
	import Tabs from '$lib/components/Tabs.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import RentSchedule from '$lib/components/RentSchedule.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager } from '$lib/pager/pager.svelte';
	import { formatErg, sumNano } from '$lib/format/amount';
	import type { RentItemDto } from '$lib/api/types';
	import type { PageData } from './$types';

	const TABS = [
		{ id: 'upcoming', label: 'Upcoming' },
		{ id: 'eligible', label: 'Eligible' }
	];
	let { data }: { data: PageData } = $props();
	const active = $derived(page.url.hash === '#eligible' ? 'eligible' : 'upcoming');
	function selectTab(id: string) {
		void goto(`#${id}`, { replaceState: true, noScroll: true, keepFocus: true });
	}
	const eligiblePager = createPager<RentItemDto>((cursor, snapshot) =>
		api.rentEligible(cursor, 50, undefined, snapshot)
	);
	let eligibleStarted = false;
	$effect(() => {
		if (active === 'eligible' && !eligibleStarted) {
			eligibleStarted = true;
			void eligiblePager.loadMore();
		}
	});
	const eligibleTotalDue = $derived(
		formatErg(
			sumNano(
				eligiblePager.items
					.filter((item) => item.box.rent.collectible)
					.map((item) => item.box.rent.due_nano)
			)
		)
	);
	const eligibleUncollectible = $derived(
		eligiblePager.items.filter((item) => !item.box.rent.collectible).length
	);
</script>

<svelte:head><title>Rent calendar — Ergo Explorer</title></svelte:head>

<Panel title="Rent">
	<Tabs tabs={TABS} {active} onchange={selectTab} label="Rent sections" />
	<div role="tabpanel" id={`panel-${active}`} tabindex="0" aria-labelledby={`tab-${active}`}>
		{#if active === 'upcoming'}
			<RentSchedule query={data.query} initial={data.schedule} error={data.error} />
		{:else}
			{#if eligiblePager.items.length > 0}<p class="sum">
					{eligiblePager.items.length} boxes{#if !eligiblePager.done}&nbsp;loaded so far{/if}, total
					due <span class="mono">{eligibleTotalDue}</span> ERG{#if eligibleUncollectible > 0}<span
							class="note"
						>
							— excludes {eligibleUncollectible} boxes whose consensus fee is non-positive</span
						>{/if}
				</p>{/if}
			<InfiniteList
				table
				columns={5}
				dense
				pager={eligiblePager}
				empty="No box has reached its storage-rent maturity yet."
			>
				{#snippet head()}<tr
						><th>Matures at</th><th>Box</th><th>Value</th><th>Due rent</th><th>Address</th></tr
					>{/snippet}
				{#snippet children(item: RentItemDto)}<tr>
						<td
							><a href={`/blocks/${item.maturity_height}`}>{item.maturity_height}</a><Badge
								tone={item.box.rent.collectible ? 'danger' : 'neutral'}
								>{item.box.rent.collectible ? 'claimable' : 'not collectible'}</Badge
							></td
						>
						<td><Hash value={item.box.id} href={`/box/${item.box.id}`} copy={false} /></td>
						<td><Amount nano={item.box.value} maxFrac={9} /></td><td
							><Amount nano={item.box.rent.due_nano} maxFrac={9} /></td
						>
						<td
							>{#if item.box.address}<Hash
									value={item.box.address}
									href={`/address/${item.box.address}`}
									copy={false}
								/>{:else}<span class="muted" title="script address unavailable"
									><Hash value={item.box.tree_hash} copy={false} /></span
								>{/if}</td
						>
					</tr>{/snippet}
			</InfiniteList>
		{/if}
	</div>
</Panel>

<style>
	.sum {
		color: var(--fg-muted);
		padding-bottom: var(--space-3);
		font-size: var(--fs-data);
	}
	.sum .note,
	.muted {
		color: var(--fg-muted);
	}
</style>
