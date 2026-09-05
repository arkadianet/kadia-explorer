<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import EmptyState from './EmptyState.svelte';
	import ErrorState from './ErrorState.svelte';
	import Skeleton from './Skeleton.svelte';
	import type { Pager } from '$lib/pager/pager.svelte';

	/**
	 * Infinite-scroll list over a `Pager<T>`: renders items via the `children` snippet, a
	 * "Load more" button, and an `IntersectionObserver` sentinel that calls `loadMore()` as it
	 * scrolls into view. Shows `Skeleton` while loading the first page, `ErrorState` with retry
	 * on error, `EmptyState` once done with zero items.
	 *
	 * For tabular lists, set `table` (with `columns` and a `head` snippet for the `<tr>` of
	 * header cells) so InfiniteList renders its own `<table>`/`<thead>`/`<tbody>`/`<tfoot>` —
	 * never place InfiniteList in its default (div-list) mode inside a `<tbody>`; a `<div>` is
	 * not valid `<tbody>` content.
	 */
	interface Props {
		pager: Pager<T>;
		children: Snippet<[T, number]>;
		empty?: string;
		table?: boolean;
		columns?: number;
		head?: Snippet;
		dense?: boolean;
	}

	let {
		pager,
		children,
		empty = 'Nothing here.',
		table = false,
		columns = 1,
		head,
		dense = false
	}: Props = $props();

	let sentinel: HTMLDivElement | undefined = $state();

	const firstLoad = $derived(pager.loading && pager.items.length === 0);
	const showEmpty = $derived(pager.done && pager.items.length === 0 && !pager.error);
	const canLoadMore = $derived(!pager.done && !pager.loading && !pager.error);
	const showFooter = $derived(
		firstLoad || pager.error !== null || showEmpty || (!pager.done && !pager.error)
	);

	$effect(() => {
		const el = sentinel;
		if (!el || typeof IntersectionObserver === 'undefined') return;
		const io = new IntersectionObserver((entries) => {
			if (entries.some((e) => e.isIntersecting)) void pager.loadMore();
		});
		io.observe(el);
		return () => io.disconnect();
	});
</script>

{#snippet footer()}
	{#if firstLoad}
		<Skeleton />
	{/if}

	{#if pager.error}
		<ErrorState error={pager.error} retry={() => pager.loadMore()} />
	{:else if showEmpty}
		<EmptyState message={empty} />
	{/if}

	{#if !pager.done && !pager.error}
		<div class="more">
			<div bind:this={sentinel} class="sentinel" aria-hidden="true"></div>
			<button type="button" class="load" onclick={() => pager.loadMore()} disabled={!canLoadMore}>
				{pager.loading ? 'Loading…' : 'Load more'}
			</button>
		</div>
	{/if}
{/snippet}

{#if table}
	<div class="table-wrap">
		<table class="table" class:dense>
			{#if head}
				<thead>{@render head()}</thead>
			{/if}
			<tbody>
				{#each pager.items as item, i (i)}
					{@render children(item, i)}
				{/each}
			</tbody>
			{#if showFooter}
				<tfoot>
					<tr><td colspan={columns}>{@render footer()}</td></tr>
				</tfoot>
			{/if}
		</table>
	</div>
{:else}
	<div class="list">
		{#each pager.items as item, i (i)}
			{@render children(item, i)}
		{/each}

		{@render footer()}
	</div>
{/if}

<style>
	.more {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: var(--space-3);
	}
	.sentinel {
		height: 1px;
		width: 100%;
	}
	.load {
		background: var(--bg-elev);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		padding: var(--space-2) var(--space-4);
		cursor: pointer;
	}
	.load:hover:not(:disabled) {
		background: var(--bg-hover);
	}
	.load:disabled {
		color: var(--fg-muted);
		cursor: default;
	}
</style>
