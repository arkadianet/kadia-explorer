<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		dense?: boolean;
		head?: Snippet;
		children?: Snippet;
	}

	let { dense = false, head, children }: Props = $props();
</script>

<div class="wrap">
	<table class:dense>
		{#if head}
			<thead>{@render head()}</thead>
		{/if}
		<tbody>{@render children?.()}</tbody>
	</table>
</div>

<style>
	.wrap {
		overflow-x: auto;
	}
	thead :global(th) {
		position: sticky;
		top: 0;
		z-index: 1;
		background: var(--bg-elev);
		color: var(--fg-muted);
		font-size: 12px;
		font-weight: 600;
	}
	tbody :global(tr):hover {
		background: var(--bg-hover);
	}
	.dense :global(tr) {
		height: 28px;
	}
</style>
