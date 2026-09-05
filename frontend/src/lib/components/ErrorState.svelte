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
		padding: var(--space-5) var(--space-4);
		text-align: center;
	}
	.title {
		color: var(--danger);
		font-weight: 600;
	}
	.detail {
		color: var(--fg-muted);
		margin-top: var(--space-1);
	}
	.retry {
		margin-top: var(--space-3);
		background: var(--bg-elev);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		padding: var(--space-1) var(--space-3);
		cursor: pointer;
	}
	.retry:hover {
		background: var(--bg-hover);
	}
</style>
