<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title?: string;
		/** Optional controls shown beside the title, e.g. a horizon selector or a link. */
		actions?: Snippet;
		children?: Snippet;
	}

	let { title, actions, children }: Props = $props();
</script>

<!-- The card every inner page is built from: a solid surface, a hairline head, and a body
     that lets tables run edge to edge while prose keeps the card's own margin. -->
<section class="panel card">
	{#if title || actions}
		<div class="panel-head card-head">
			{#if title}<h2 class="panel-title card-title">{title}</h2>{/if}
			{#if actions}<div class="panel-actions">{@render actions()}</div>{/if}
		</div>
	{/if}
	<div class="panel-body">{@render children?.()}</div>
</section>

<style>
	.panel {
		min-width: 0;
		overflow: hidden;
	}

	.panel-head {
		flex-wrap: wrap;
	}

	.panel-body {
		min-width: 0;
		padding: var(--space-4) var(--space-5);
	}

	/* A table is the card's full width; only its cells carry the inset, so column rules line
	   up with the card edge instead of floating inside a second margin. */
	.panel-body :global(.table-wrap) {
		margin: calc(var(--space-4) * -1) calc(var(--space-5) * -1);
	}

	.panel-actions {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		font-size: var(--fs-data);
		color: var(--fg-muted);
	}
</style>
