<script lang="ts">
	import { status } from '$lib/status/status.svelte';
	import { health } from '$lib/status/health';

	const h = $derived(health(status.current));
	const title = $derived(status.error && !status.current ? 'Status unavailable' : h.detail);
</script>

<!-- The one piece of state that qualifies everything else on the page: is what you are
     reading current? It links to /status, where the same numbers are set out in full. -->
<a href="/status" class="live pill {h.tone}" {title} aria-live="polite">
	<span class="dot" aria-hidden="true"></span>
	{h.live}
</a>

<style>
	.live {
		background: var(--surface-solid);
		border: var(--rule);
		box-shadow: var(--shadow-hair);
		color: var(--fg);
		height: 34px;
		padding: 0 var(--space-4);
	}

	.live:hover,
	.live:focus-visible {
		color: var(--fg);
		border-color: var(--fg-muted);
	}

	.ok .dot,
	.ok.dot {
		color: var(--ok);
	}

	.dot {
		color: var(--fg-muted);
	}

	.ok .dot {
		color: var(--ok);
		box-shadow: 0 0 0 3px var(--accent-wash);
	}

	.warn .dot {
		color: var(--warn);
	}

	.danger .dot {
		color: var(--danger);
	}
</style>
