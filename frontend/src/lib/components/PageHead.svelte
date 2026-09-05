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
		<h1>{title}</h1>
		{#if aside}<div class="aside">{@render aside()}</div>{/if}
	</div>
	{#if id}
		<div class="id"><Hash value={id} head={id.length} tail={0} /></div>
	{/if}
</header>

<style>
	.page-head {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-4);
		flex-wrap: wrap;
	}

	h1 {
		font-size: var(--fs-page);
		font-weight: 600;
	}

	.id {
		color: var(--fg-muted);
	}

	/* The id is set inline so a 95-character address wraps instead of running off the page. */
	.id :global(.hash) {
		display: inline;
	}

	.id :global(.mono) {
		overflow-wrap: anywhere;
	}
</style>
