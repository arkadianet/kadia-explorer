<script lang="ts">
	import type { Snippet } from 'svelte';
	import Hash from './Hash.svelte';

	interface Props {
		title: string;
		/** The entity's own identifier, shown in full underneath the title. */
		id?: string;
		/** Right-hand content on the title's baseline, e.g. a status badge. */
		aside?: Snippet;
	}

	let { title, id, aside }: Props = $props();
</script>

<header class="page-head">
	<div class="row">
		<h1><bdi dir="auto">{title}</bdi></h1>
		{#if aside}<div class="aside">{@render aside()}</div>{/if}
	</div>
	{#if id}
		<div class="id"><Hash value={id} head={id.length} tail={0} /></div>
	{/if}
</header>

<style>
	/* The entity's nameplate: title, then the identifier in full, on a card of its own so a
	   detail page opens with the same weight as the home page's hero. */
	.page-head {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-5) var(--space-5) var(--space-4);
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: var(--radius-card);
		box-shadow: var(--shadow-rest);
	}

	.row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-4);
		flex-wrap: wrap;
	}

	h1 {
		min-width: 0;
		max-width: 100%;
		overflow-wrap: anywhere;
		font-size: var(--fs-page);
		font-weight: 600;
		letter-spacing: -0.02em;
	}

	.id {
		color: var(--fg-muted);
		font-size: var(--fs-data);
	}

	/* The id is set inline so a 95-character address wraps instead of running off the page. */
	.id :global(.hash) {
		display: inline;
	}

	.id :global(.mono) {
		overflow-wrap: anywhere;
	}
</style>
