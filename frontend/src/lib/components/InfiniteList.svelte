<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import EmptyState from './EmptyState.svelte';
	import ErrorState from './ErrorState.svelte';
	import Skeleton from './Skeleton.svelte';
	import type { Pager } from '$lib/pager/pager.svelte';

	interface Props {
		pager: Pager<T>;
		children: Snippet<[T, number]>;
		empty?: string;
	}

	let { pager, children, empty = 'Nothing here.' }: Props = $props();

	let sentinel: HTMLDivElement | undefined = $state();

	const firstLoad = $derived(pager.loading && pager.items.length === 0);
	const showEmpty = $derived(pager.done && pager.items.length === 0 && !pager.error);
	const canLoadMore = $derived(!pager.done && !pager.loading && !pager.error);

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

<div class="list">
	{#each pager.items as item, i (i)}
		{@render children(item, i)}
	{/each}

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
</div>

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
