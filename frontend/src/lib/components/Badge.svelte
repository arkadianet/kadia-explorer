<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone?: 'neutral' | 'ok' | 'warn' | 'danger';
		title?: string;
		children?: Snippet;
	}

	let { tone = 'neutral', title, children }: Props = $props();
</script>

<span class="badge {tone}" {title}>{@render children?.()}</span>

<style>
	/* A pill washed with its own tone. The neutral badge takes the hover surface instead of a
	   wash — a grey wash on a grey ground is invisible, and neutral is the "nothing to see"
	   state anyway. */
	.badge {
		display: inline-block;
		padding: 0 var(--space-2);
		border-radius: 999px;
		font-size: 12px;
		line-height: 18px;
		white-space: nowrap;
		color: var(--fg-muted);
		background: var(--bg-hover);
	}
	.ok {
		color: var(--ok-ink);
		background: color-mix(in srgb, var(--ok) calc(var(--wash) * 100%), transparent);
	}
	.warn {
		color: var(--warn-ink);
		background: color-mix(in srgb, var(--warn) calc(var(--wash) * 100%), transparent);
	}
	.danger {
		color: var(--danger-ink);
		background: color-mix(in srgb, var(--danger) calc(var(--wash) * 100%), transparent);
	}
</style>
