<script lang="ts">
	interface Props {
		rows?: number;
	}

	let { rows = 5 }: Props = $props();
</script>

<div class="skeleton" aria-hidden="true">
	{#each { length: rows }, i (i)}
		<div class="row"></div>
	{/each}
</div>

<style>
	.skeleton {
		padding: var(--space-2) var(--space-4);
	}
	.row {
		height: 36px;
		border-bottom: 1px solid var(--border);
		background: linear-gradient(90deg, var(--bg-elev), var(--bg-hover), var(--bg-elev));
		background-size: 200% 100%;
		animation: shimmer 1.4s linear infinite;
	}
	@keyframes shimmer {
		from {
			background-position: 200% 0;
		}
		to {
			background-position: -200% 0;
		}
	}
</style>
