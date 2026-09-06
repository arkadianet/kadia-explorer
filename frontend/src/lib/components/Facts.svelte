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
	/* One card, two columns of label/value rows: the entity's facts belong together, so they
	   share a surface rather than floating on the page ground. */
	.facts {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 0 var(--space-8);
		margin: 0;
		padding: var(--space-2) var(--space-5) var(--space-3);
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: var(--radius-card);
		box-shadow: var(--shadow-rest);
	}

	@media (min-width: 900px) {
		.facts {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
