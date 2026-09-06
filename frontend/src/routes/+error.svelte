<script lang="ts">
	import { page } from '$app/state';
	import SearchBox from '$lib/components/SearchBox.svelte';
</script>

<div class="error">
	<h1 class="mono">{page.status}</h1>
	<p class="message">{page.error?.message ?? 'Something went wrong.'}</p>
	<p class="hint">Check the height or id, or look for something else.</p>
	<SearchBox id="error-page-search" globalShortcut={false} />
	<p><a href="/">Back to the home page</a></p>
</div>

<style>
	.error {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-3);
		max-width: 62ch;
		padding: var(--space-8) var(--space-6);
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: var(--radius-card);
		box-shadow: var(--shadow-rest);
	}

	.error h1 {
		font-size: var(--fs-key);
		font-weight: 600;
		color: var(--danger-ink);
		line-height: 1.1;
	}

	.message {
		font-size: var(--fs-title);
		font-weight: 600;
	}

	.hint {
		color: var(--fg-muted);
	}
</style>
