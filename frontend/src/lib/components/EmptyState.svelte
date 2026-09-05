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
		padding: var(--space-6) var(--space-4);
		text-align: center;
	}
</style>
