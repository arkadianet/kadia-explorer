<script lang="ts">
	import { ApiError } from '$lib/api/client';

	interface Props {
		error: unknown;
		retry?: () => void;
	}

	let { error, retry }: Props = $props();

	const message = $derived(
		error instanceof ApiError
			? error.detail
			: error instanceof Error
				? error.message
				: 'Something went wrong.'
	);
	const title = $derived(error instanceof ApiError ? `${error.status} ${error.title}` : 'Error');
</script>

<div class="error" role="alert">
	<p class="title">{title}</p>
	<p class="detail">{message}</p>
	{#if retry}
		<button type="button" class="retry" onclick={retry}>Retry</button>
	{/if}
</div>

<style>
	.error {
		padding: var(--space-2) 0;
		max-width: 62ch;
	}
	.title {
		color: var(--danger-ink);
		font-weight: 600;
	}
	.detail {
		color: var(--fg-muted);
		margin-top: var(--space-1);
	}
	.retry {
		margin-top: var(--space-3);
		background: transparent;
		border: 1px solid var(--hairline);
		color: var(--fg);
		border-radius: var(--radius-pill);
		padding: 0 var(--space-4);
		height: 32px;
		font-size: var(--fs-data);
		font-weight: 600;
		cursor: pointer;
	}
	.retry:hover {
		border-color: var(--accent);
		color: var(--accent-ink);
	}
</style>
