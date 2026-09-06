<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label: string;
		children?: Snippet;
	}

	let { label, children }: Props = $props();
</script>

<div class="fact">
	<dt class="label">{label}</dt>
	<dd class="value">{@render children?.()}</dd>
</div>

<style>
	.fact {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--space-3) 0;
		border-bottom: var(--rule);
		font-size: var(--fs-data);
	}

	/* The last row in each column keeps its rule only when a row follows it, so a card never
	   ends on a hairline. */
	.fact:last-child,
	.fact:nth-last-child(2):nth-child(odd) {
		border-bottom: 0;
	}

	.label {
		color: var(--fg-muted);
		white-space: nowrap;
	}

	.value {
		margin: 0;
		min-width: 0;
		text-align: right;
		overflow-wrap: anywhere;
	}
</style>
