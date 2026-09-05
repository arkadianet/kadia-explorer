<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		dense?: boolean;
		head?: Snippet;
		children?: Snippet;
	}

	let { dense = false, head, children }: Props = $props();
</script>

<!-- `.table`/`.table-wrap`/`.dense` are shared, global classes defined in
     $lib/styles/base.css so InfiniteList's table mode can render an identical-looking
     table without duplicating this component's styles. -->
<div class="table-wrap">
	<table class="table" class:dense>
		{#if head}
			<thead>{@render head()}</thead>
		{/if}
		<tbody>{@render children?.()}</tbody>
	</table>
</div>
