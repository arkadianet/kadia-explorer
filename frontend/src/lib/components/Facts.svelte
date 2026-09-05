<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		children?: Snippet;
	}

	let { children }: Props = $props();
</script>

<!-- The shared label/value grid every detail page hangs its header data on. Rows come from
     Fact.svelte; two columns from 900 px, one below. -->
<dl class="facts">{@render children?.()}</dl>

<style>
	.facts {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 0 var(--space-8);
		margin: 0;
		border-top: var(--rule);
	}

	@media (min-width: 900px) {
		.facts {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
