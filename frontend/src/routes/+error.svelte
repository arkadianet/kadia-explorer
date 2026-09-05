<script lang="ts">
	import { page } from '$app/state';
</script>

<div class="error">
	<h1>{page.status}</h1>
	<p>{page.error?.message ?? 'Something went wrong.'}</p>
	<a href="/">Back home</a>
</div>

<style>
	.error {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-3);
		padding: var(--space-6) 0;
	}

	.error h1 {
		font-size: 2rem;
		color: var(--danger);
	}
</style>
