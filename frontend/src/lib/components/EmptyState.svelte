<script lang="ts">
	interface Props {
		message: string;
	}

	let { message }: Props = $props();
</script>

<p class="empty">{message}</p>

<style>
	.empty {
		color: var(--fg-muted);
		padding: var(--space-5) 0;
		max-width: 62ch;
	}
</style>
