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
		padding: var(--space-2) 0;
		border-bottom: var(--rule);
		font-size: var(--fs-data);
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
