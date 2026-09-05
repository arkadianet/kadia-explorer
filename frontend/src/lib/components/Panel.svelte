<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title?: string;
		/** Optional controls shown on the title's baseline, e.g. a horizon selector. */
		actions?: Snippet;
		children?: Snippet;
	}

	let { title, actions, children }: Props = $props();
</script>

<!-- A section, not a card: a title, a hairline under it, and space around it do the grouping.
     Nothing here draws a box — see the design brief's "Surfaces". -->
<section class="panel">
	{#if title || actions}
		<div class="panel-head">
			{#if title}<h2 class="panel-title">{title}</h2>{/if}
			{#if actions}<div class="panel-actions">{@render actions()}</div>{/if}
		</div>
	{/if}
	<div class="panel-body">{@render children?.()}</div>
</section>

<style>
	/* No margin of its own: the page container (or the page's own grid) sets the rhythm, so a
	   panel keeps its position in a two-column layout instead of drifting down. */
	.panel {
		min-width: 0;
	}

	.panel-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-4);
		flex-wrap: wrap;
		padding-bottom: var(--space-2);
		border-bottom: var(--rule);
		margin-bottom: var(--space-3);
	}

	.panel-title {
		font-size: var(--fs-title);
		font-weight: 600;
		color: var(--fg);
	}

	.panel-body {
		min-width: 0;
	}

	.panel-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--fs-data);
		color: var(--fg-muted);
	}
</style>
