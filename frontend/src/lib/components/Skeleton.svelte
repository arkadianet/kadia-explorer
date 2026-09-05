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
	/* Flat bars, no shimmer: a moving gradient is decoration, and it would have to be switched
	   off again for reduced motion anyway. */
	.row {
		height: 30px;
		border-bottom: var(--rule);
		background: var(--bg-elev);
	}
</style>
