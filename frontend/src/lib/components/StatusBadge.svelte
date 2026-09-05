<script lang="ts">
	import Badge from './Badge.svelte';
	import { status } from '$lib/status/status.svelte';

	const tone = $derived.by((): 'ok' | 'warn' | 'danger' | 'neutral' => {
		const s = status.current;
		if (!s) return 'neutral';
		if (s.halted !== null || s.stalled !== null) return 'danger';
		if (s.lag_blocks > 100) return 'danger';
		if (s.lag_blocks > 3) return 'warn';
		return 'ok';
	});

	const label = $derived(status.current ? `#${status.current.indexed ?? 0}` : '…');

	const title = $derived.by((): string => {
		const s = status.current;
		if (!s) return status.error ? 'Status unavailable' : 'Loading status…';
		if (s.stalled) {
			return `Stalled at height ${s.stalled.height} for ${s.stalled.since_secs}s: ${s.stalled.reason}`;
		}
		if (s.halted) return `Halted: ${s.halted}`;
		return `Indexed ${s.indexed ?? 0} / best ${s.best} (lag ${s.lag_blocks})`;
	});
</script>

<a href="/status" class="status-badge" aria-live="polite">
	<Badge {tone} {title}>{label}</Badge>
</a>

<style>
	.status-badge {
		text-decoration: none;
	}
</style>
