<script lang="ts">
	import { goto } from '$app/navigation';
	import Panel from '$lib/components/Panel.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import TokenBadge from '$lib/components/TokenBadge.svelte';
	import Skeleton from '$lib/components/Skeleton.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager, type Pager } from '$lib/pager/pager.svelte';
	import { formatTokenAmount } from '$lib/format/amount';
	import { tokenDisplayName } from '$lib/token/kind';
	import type { TokenInfoDto } from '$lib/api/types';
	import type { PageData } from './$types';

	/** 'newest' | 'holders', as narrowed by the page's load function. */
	type TokenSort = PageData['sort'];

	const PAGE_SIZE = 50;

	const SORTS: { id: TokenSort; label: string; hint: string }[] = [
		{ id: 'newest', label: 'Newest', hint: 'Most recently minted first' },
		{ id: 'holders', label: 'Most held', hint: 'Most holding addresses first' }
	];

	let { data }: { data: PageData } = $props();

	const sort = $derived(data.sort);

	function selectSort(id: TokenSort) {
		// The sort lives in the query string so a link carries it; `replaceState` keeps flipping
		// between the two out of the back button.
		void goto(id === 'newest' ? '/tokens' : `/tokens?sort=${id}`, {
			replaceState: true,
			noScroll: true,
			keepFocus: true
		});
	}

	// One pager per sort: changing the sort is a different query, so the old rows and cursor
	// are dropped rather than appended to.
	let pager = $state<Pager<TokenInfoDto> | null>(null);
	let pagerSort: TokenSort | null = null;

	$effect(() => {
		if (pagerSort === sort) return;
		pagerSort = sort;
		const p = createPager<TokenInfoDto>((cursor) => api.tokens(sort, cursor, PAGE_SIZE));
		pager = p;
		void p.loadMore();
	});
</script>

<svelte:head>
	<title>Tokens — Ergo Explorer</title>
</svelte:head>

<Panel title="Tokens">
	{#snippet actions()}
		<div class="seg" role="group" aria-label="Sort tokens">
			{#each SORTS as option (option.id)}
				<button
					type="button"
					class="seg-btn"
					aria-pressed={option.id === sort}
					title={option.hint}
					onclick={() => selectSort(option.id)}
				>
					{option.label}
				</button>
			{/each}
		</div>
	{/snippet}

	{#if pager}
		<InfiniteList
			table
			columns={5}
			dense
			{pager}
			empty="No tokens indexed yet — they appear as the indexer reaches the blocks that mint them."
		>
			{#snippet head()}
				<tr>
					<th>Name</th>
					<th>Kind</th>
					<th class="num">Supply</th>
					<th class="num">Holders</th>
					<th class="num">Minted at</th>
				</tr>
			{/snippet}
			{#snippet children(token: TokenInfoDto)}
				<tr>
					<td class="name">
						<a href={`/token/${token.id}`} title={token.name.trim() || token.id}>
							{tokenDisplayName(token.name, token.id)}
						</a>
					</td>
					<td><TokenBadge kind={token.kind} /></td>
					<td class="num mono">{formatTokenAmount(token.supply, token.decimals)}</td>
					<td class="num mono">{token.holder_count.toLocaleString('en-US')}</td>
					<td class="num mono"><a href={`/blocks/${token.mint_height}`}>{token.mint_height}</a></td>
				</tr>
			{/snippet}
		</InfiniteList>
	{:else}
		<Skeleton />
	{/if}
</Panel>

<style>
	/* A name is prose, not a hash: it keeps the body face and the row's only real weight. */
	.name a {
		font-weight: 500;
	}
</style>
