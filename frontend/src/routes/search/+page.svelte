<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import SearchBox from '$lib/components/SearchBox.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const hints = {
		empty: 'Enter a block height, an id or an address.',
		'unknown-format': 'Enter a block height, an id or an address.',
		'not-found': '64-hex ids can be block, transaction or box ids. Addresses start with 9.'
	} as const;

	const hint = $derived(hints[data.reason]);
</script>

<svelte:head>
	<title>Search — Ergo Explorer</title>
</svelte:head>

<Panel title="Search">
	{#if data.q}
		<EmptyState message={`No match for "${data.q}". ${hint}`} />
	{:else}
		<EmptyState message={hint} />
	{/if}
	<div class="retry">
		<!-- A distinct id keeps `#global-search` unique to the header box, and the header box
		     keeps sole ownership of the "/" shortcut. -->
		<SearchBox id="search-page-search" globalShortcut={false} />
	</div>
</Panel>

<style>
	.retry {
		padding: 0 var(--space-4) var(--space-4);
	}
</style>
