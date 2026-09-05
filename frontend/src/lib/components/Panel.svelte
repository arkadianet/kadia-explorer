<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title?: string;
		children?: Snippet;
	}

	let { title, children }: Props = $props();
</script>

<section class="panel">
	{#if title}<h2 class="panel-title">{title}</h2>{/if}
	<div class="panel-body">{@render children?.()}</div>
</section>

<style>
	.panel {
		background: var(--bg-elev);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.panel-title {
		font-size: 13px;
		font-weight: 600;
		color: var(--fg-muted);
		padding: var(--space-3) var(--space-4);
		border-bottom: 1px solid var(--border);
	}
	.panel-body {
		padding: var(--space-2) 0;
	}
</style>
