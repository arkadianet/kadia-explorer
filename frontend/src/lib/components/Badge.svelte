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
	.badge {
		display: inline-block;
		padding: 1px var(--space-2);
		border-radius: var(--radius);
		border: 1px solid var(--border);
		font-size: 11px;
		line-height: 16px;
		white-space: nowrap;
		color: var(--fg-muted);
		background: var(--bg-elev);
	}
	.ok {
		color: var(--ok);
		border-color: var(--ok);
	}
	.warn {
		color: var(--warn);
		border-color: var(--warn);
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
</style>
